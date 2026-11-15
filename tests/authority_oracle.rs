// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Independent authority-oracle checks for the Lean model and production Rust.
//!
//! Unlike `lean_model_correspondence`, the expected values here come from the
//! committed Circle authority snapshot, not from Lean. The two tests keep the
//! evidence paths separate: one checks Lean's executable fixture surface and
//! the other checks production APIs directly.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use alloy_chains::NamedChain;
use alloy_primitives::Address;
use cctp_rs::{CctpTransferAsset, CctpV2, DomainId};
use serde::Deserialize;

const AUTHORITY_JSON: &str = include_str!("../scripts/fixtures/circle_cctp_protocol_snapshot.json");
const LEAN_JSON: &str = include_str!("fixtures/lean/cctp_v2_vectors.json");

#[derive(Clone, Deserialize)]
struct AuthoritySnapshot {
    schema_version: u32,
    captured_at: String,
    authority_sources: HashMap<String, AuthoritySource>,
    field_provenance: HashMap<String, String>,
    domains: Vec<AuthorityDomain>,
    non_evm_domain_ids: HashSet<u32>,
    bridge_domain_ids: HashSet<u32>,
    sdk_domain_names: HashMap<String, String>,
    sdk_networks: Vec<AuthorityNetwork>,
    eurc_bridge_networks: HashSet<String>,
    capabilities: Vec<Capability>,
    address_tables: HashMap<String, Vec<AddressRow>>,
}

#[derive(Clone, Deserialize)]
struct AuthoritySource {
    kind: String,
    url: Option<String>,
    retrieved_on: Option<String>,
    references: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
struct AuthorityDomain {
    domain: u32,
    blockchain: String,
}

#[derive(Clone, Deserialize)]
struct AuthorityNetwork {
    key: String,
    environment: String,
    domain: Option<u32>,
    bridge_supported: bool,
    capability_name: Option<String>,
    contract_name: Option<String>,
    token_name: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Capability {
    blockchain: String,
    fast_transfer: Option<bool>,
    forwarding_service: bool,
}

#[derive(Clone, Deserialize)]
struct AddressRow {
    blockchain: String,
    domain: Option<u32>,
    address: String,
}

#[derive(Clone, Deserialize)]
struct LeanFixtures {
    domain_vectors: Vec<LeanDomain>,
    chain_vectors: Vec<LeanChain>,
}

#[derive(Clone, Deserialize)]
struct LeanDomain {
    #[serde(rename = "u32")]
    value: u32,
    valid: bool,
    name: Option<String>,
    is_evm: Option<bool>,
    bridge_supported: Option<bool>,
}

#[derive(Clone, Deserialize)]
struct LeanChain {
    chain: String,
    supports_cctp_v2: bool,
    is_testnet: bool,
    supports_fast_transfer: bool,
    supports_forwarding_service: bool,
    supports_usdc: bool,
    supports_eurc: bool,
    domain_id: Option<u32>,
}

fn authority() -> AuthoritySnapshot {
    serde_json::from_str(AUTHORITY_JSON).expect("authority snapshot should deserialize")
}

fn lean() -> LeanFixtures {
    serde_json::from_str(LEAN_JSON).expect("Lean fixtures should deserialize")
}

#[test]
fn lean_executable_tables_match_independent_authority() {
    let authority = authority();
    assert_snapshot_provenance(&authority);
    let lean = lean();
    check_lean_tables(&authority, &lean);
}

fn check_lean_tables(authority: &AuthoritySnapshot, lean: &LeanFixtures) {
    let authority_domain_ids = authority
        .domains
        .iter()
        .map(|row| row.domain)
        .collect::<HashSet<_>>();
    let lean_domains = lean
        .domain_vectors
        .iter()
        .filter(|row| row.valid)
        .map(|row| (row.value, row))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        lean_domains.keys().copied().collect::<HashSet<_>>(),
        authority_domain_ids,
        "Lean and the Circle authority must contain exactly the same current domains"
    );

    for row in &authority.domains {
        let modeled = lean_domains
            .get(&row.domain)
            .unwrap_or_else(|| panic!("Lean is missing {} ({})", row.blockchain, row.domain));
        let expected_name = authority
            .sdk_domain_names
            .get(&row.domain.to_string())
            .unwrap_or_else(|| panic!("authority lacks SDK name for domain {}", row.domain));
        assert_eq!(
            modeled.name.as_ref(),
            Some(expected_name),
            "domain {}",
            row.domain
        );
        assert_eq!(
            modeled.is_evm,
            Some(!authority.non_evm_domain_ids.contains(&row.domain)),
            "domain {} EVM classification",
            row.domain
        );
        assert_eq!(
            modeled.bridge_supported,
            Some(authority.bridge_domain_ids.contains(&row.domain)),
            "domain {} bridge classification",
            row.domain
        );
    }

    let authority_networks = authority
        .sdk_networks
        .iter()
        .map(|row| (row.key.as_str(), row))
        .collect::<HashMap<_, _>>();
    let lean_networks = lean
        .chain_vectors
        .iter()
        .map(|row| (row.chain.as_str(), row))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        lean_networks.keys().copied().collect::<HashSet<_>>(),
        authority_networks.keys().copied().collect::<HashSet<_>>(),
        "Lean and the reviewed authority mapping must contain exactly the same networks"
    );

    for (key, expected) in authority_networks {
        let modeled = lean_networks[key];
        assert_eq!(modeled.supports_cctp_v2, expected.bridge_supported, "{key}");
        assert_eq!(
            modeled.is_testnet,
            expected.environment == "testnet",
            "{key}"
        );
        assert_eq!(modeled.domain_id, expected.domain, "{key}");
        let capability = expected.capability_name.as_deref().and_then(|name| {
            authority
                .capabilities
                .iter()
                .find(|row| row.blockchain == name)
        });
        assert_eq!(
            modeled.supports_fast_transfer,
            capability
                .and_then(|row| row.fast_transfer)
                .unwrap_or(false),
            "{key} Fast Transfer capability"
        );
        assert_eq!(
            modeled.supports_forwarding_service,
            capability.is_some_and(|row| row.forwarding_service),
            "{key} Forwarding Service capability"
        );
        assert_eq!(
            modeled.supports_usdc, expected.bridge_supported,
            "{key} USDC support"
        );
        assert_eq!(
            modeled.supports_eurc,
            authority.eurc_bridge_networks.contains(key),
            "{key} EURC support"
        );
    }
}

#[test]
fn production_tables_match_independent_authority() {
    let authority = authority();
    assert_snapshot_provenance(&authority);
    check_rust_tables(&authority);
}

fn check_rust_tables(authority: &AuthoritySnapshot) {
    for row in &authority.domains {
        let domain = DomainId::from_u32(row.domain)
            .unwrap_or_else(|| panic!("Rust is missing {} ({})", row.blockchain, row.domain));
        assert_eq!(domain.as_u32(), row.domain);
        assert_eq!(
            serde_name(&domain),
            authority.sdk_domain_names[&row.domain.to_string()]
        );
        assert_eq!(
            domain.is_evm(),
            !authority.non_evm_domain_ids.contains(&row.domain)
        );
    }

    let modeled_domains = (0..=1024)
        .filter_map(DomainId::from_u32)
        .map(DomainId::as_u32)
        .collect::<HashSet<_>>();
    assert_eq!(
        modeled_domains,
        authority.domains.iter().map(|row| row.domain).collect(),
        "Rust must not add or omit authority domains"
    );

    for network in &authority.sdk_networks {
        check_rust_network(authority, network);
    }

    for chain in NamedChain::VARIANTS {
        if chain.supports_cctp_v2() {
            assert!(
                authority
                    .sdk_networks
                    .iter()
                    .any(|network| named_chain(&network.key) == *chain),
                "production-supported chain {chain:?} is absent from the authority mapping"
            );
        }
    }
}

#[test]
fn intentional_lean_and_rust_mismatches_are_detected() {
    let authority = authority();

    let mut mismatched_lean = lean();
    mismatched_lean
        .chain_vectors
        .iter_mut()
        .find(|row| row.chain == "ethereum_mainnet")
        .expect("Ethereum Lean vector")
        .domain_id = Some(999);
    assert!(
        std::panic::catch_unwind(|| check_lean_tables(&authority, &mismatched_lean)).is_err(),
        "the Lean/authority comparator must reject a changed Lean domain"
    );

    let mut mismatched_authority = authority.clone();
    mismatched_authority
        .sdk_networks
        .iter_mut()
        .find(|row| row.key == "ethereum_mainnet")
        .expect("Ethereum authority network")
        .domain = Some(999);
    assert!(
        std::panic::catch_unwind(|| check_rust_tables(&mismatched_authority)).is_err(),
        "the Rust/authority comparator must reject a changed authority domain"
    );
}

fn assert_snapshot_provenance(authority: &AuthoritySnapshot) {
    assert_eq!(authority.schema_version, 2);
    for source in [
        "circle_supported_chains",
        "circle_cctp_contracts",
        "circle_usdc_addresses",
        "circle_eurc_addresses",
        "sdk_bridge_policy",
    ] {
        let entry = authority
            .authority_sources
            .get(source)
            .unwrap_or_else(|| panic!("missing authority source {source}"));
        match entry.kind.as_str() {
            "published_authority" => {
                assert!(
                    entry
                        .url
                        .as_ref()
                        .is_some_and(|url| url.starts_with("https://developers.circle.com/")),
                    "{source} needs an official Circle URL"
                );
                assert_eq!(
                    entry.retrieved_on.as_deref(),
                    Some(authority.captured_at.as_str()),
                    "{source} retrieval date must match the snapshot capture date"
                );
            }
            "maintainer_policy" => {
                assert!(entry
                    .references
                    .as_ref()
                    .is_some_and(|refs| !refs.is_empty()))
            }
            other => panic!("unknown authority source kind: {other}"),
        }
    }
    for field in [
        "/domains",
        "/capabilities",
        "/address_tables/cctp_v2_*",
        "/address_tables/usdc_*",
        "/address_tables/eurc_*",
        "/sdk_networks",
    ] {
        assert!(
            authority.field_provenance.contains_key(field),
            "missing provenance for {field}"
        );
    }
}

fn check_rust_network(authority: &AuthoritySnapshot, network: &AuthorityNetwork) {
    let chain = named_chain(&network.key);
    assert_eq!(
        chain.supports_cctp_v2(),
        network.bridge_supported,
        "{}",
        network.key
    );
    assert_eq!(
        chain.is_testnet(),
        network.environment == "testnet",
        "{}",
        network.key
    );

    if !network.bridge_supported {
        assert!(chain.cctp_v2_domain_id().is_err(), "{}", network.key);
        assert!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(chain)
                .is_err(),
            "{}",
            network.key
        );
        return;
    }

    let domain = network.domain.expect("supported network carries a domain");
    assert_eq!(
        chain
            .cctp_v2_domain_id()
            .expect("supported domain")
            .as_u32(),
        domain,
        "{}",
        network.key
    );

    let capability_name = network
        .capability_name
        .as_deref()
        .expect("supported capability name");
    let capability = authority
        .capabilities
        .iter()
        .find(|row| row.blockchain == capability_name)
        .unwrap_or_else(|| panic!("missing capability row for {capability_name}"));
    assert_eq!(
        chain.supports_fast_transfer().expect("supported chain"),
        capability.fast_transfer.unwrap_or(false),
        "{}",
        network.key
    );
    assert_eq!(
        chain
            .supports_forwarding_service()
            .expect("supported chain"),
        capability.forwarding_service,
        "{}",
        network.key
    );

    let contract_name = network
        .contract_name
        .as_deref()
        .expect("supported contract name");
    let environment = &network.environment;
    let token_messenger = authority_address(
        authority,
        &format!("cctp_v2_token_messenger_{environment}"),
        contract_name,
        Some(domain),
    );
    let message_transmitter = authority_address(
        authority,
        &format!("cctp_v2_message_transmitter_{environment}"),
        contract_name,
        Some(domain),
    );
    assert_eq!(
        chain
            .token_messenger_v2_address()
            .expect("supported contract"),
        token_messenger,
        "{}",
        network.key
    );
    assert_eq!(
        chain
            .message_transmitter_v2_address()
            .expect("supported contract"),
        message_transmitter,
        "{}",
        network.key
    );

    let token_name = network.token_name.as_deref().expect("supported token name");
    let usdc = authority_address(authority, &format!("usdc_{environment}"), token_name, None);
    assert_eq!(
        CctpTransferAsset::Usdc
            .v2_bridge_token_address(chain)
            .expect("supported USDC"),
        usdc,
        "{}",
        network.key
    );

    let eurc_supported = authority.eurc_bridge_networks.contains(&network.key);
    assert_eq!(
        CctpTransferAsset::Eurc.is_supported_on_v2_bridge_chain(chain),
        eurc_supported,
        "{}",
        network.key
    );
    if eurc_supported {
        let eurc = authority_address(authority, &format!("eurc_{environment}"), token_name, None);
        assert_eq!(
            CctpTransferAsset::Eurc
                .v2_bridge_token_address(chain)
                .expect("supported EURC"),
            eurc,
            "{}",
            network.key
        );
    } else {
        assert!(
            CctpTransferAsset::Eurc
                .v2_bridge_token_address(chain)
                .is_err(),
            "{}",
            network.key
        );
    }
}

fn authority_address(
    authority: &AuthoritySnapshot,
    table: &str,
    blockchain: &str,
    domain: Option<u32>,
) -> Address {
    let row = authority
        .address_tables
        .get(table)
        .unwrap_or_else(|| panic!("missing authority table {table}"))
        .iter()
        .find(|row| row.blockchain == blockchain && domain.is_none_or(|id| row.domain == Some(id)))
        .unwrap_or_else(|| panic!("missing {blockchain} in authority table {table}"));
    Address::from_str(&row.address)
        .unwrap_or_else(|error| panic!("invalid {blockchain} address in {table}: {error}"))
}

fn serde_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializable")
        .as_str()
        .expect("domain serializes as a string")
        .to_owned()
}

fn named_chain(name: &str) -> NamedChain {
    match name {
        "ethereum_mainnet" => NamedChain::Mainnet,
        "ethereum_sepolia" => NamedChain::Sepolia,
        "arbitrum_mainnet" => NamedChain::Arbitrum,
        "arbitrum_sepolia" => NamedChain::ArbitrumSepolia,
        "base_mainnet" => NamedChain::Base,
        "base_sepolia" => NamedChain::BaseSepolia,
        "optimism_mainnet" => NamedChain::Optimism,
        "optimism_sepolia" => NamedChain::OptimismSepolia,
        "avalanche_mainnet" => NamedChain::Avalanche,
        "avalanche_fuji" => NamedChain::AvalancheFuji,
        "polygon_mainnet" => NamedChain::Polygon,
        "polygon_amoy" => NamedChain::PolygonAmoy,
        "unichain_mainnet" => NamedChain::Unichain,
        "unichain_sepolia" => NamedChain::UnichainSepolia,
        "linea_mainnet" => NamedChain::Linea,
        "linea_sepolia" => NamedChain::LineaSepolia,
        "sonic_mainnet" => NamedChain::Sonic,
        "sonic_testnet" => NamedChain::SonicTestnet,
        "sei_mainnet" => NamedChain::Sei,
        "sei_testnet" => NamedChain::SeiTestnet,
        "hyper_evm_mainnet" => NamedChain::Hyperliquid,
        "hyper_evm_testnet" => NamedChain::HyperliquidTestnet,
        "world_mainnet" => NamedChain::World,
        "world_sepolia" => NamedChain::WorldSepolia,
        "monad_mainnet" => NamedChain::Monad,
        "monad_testnet" => NamedChain::MonadTestnet,
        "xdc_mainnet" => NamedChain::XdcMainnet,
        "xdc_testnet" => NamedChain::XdcTestnet,
        "ink_mainnet" => NamedChain::Ink,
        "ink_sepolia" => NamedChain::InkSepolia,
        "plume_mainnet" => NamedChain::Plume,
        "plume_testnet" => NamedChain::PlumeTestnet,
        "arc_mainnet" => NamedChain::Arc,
        "arc_testnet" => NamedChain::ArcTestnet,
        "injective_mainnet" => NamedChain::Injective,
        "injective_testnet" => NamedChain::InjectiveTestnet,
        "pharos_mainnet" => NamedChain::Pharos,
        "pharos_testnet" => NamedChain::PharosTestnet,
        "cronos_mainnet" => NamedChain::Cronos,
        "cronos_testnet" => NamedChain::CronosTestnet,
        "plasma_mainnet" => NamedChain::Plasma,
        "plasma_testnet" => NamedChain::PlasmaTestnet,
        "bnb_smart_chain_mainnet" => NamedChain::BinanceSmartChain,
        "moonbeam_mainnet" => NamedChain::Moonbeam,
        other => panic!("unknown authority network: {other}"),
    }
}
