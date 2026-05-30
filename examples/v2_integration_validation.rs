// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 Integration Validation
//!
//! This example validates all v2 configuration without requiring network access.
//! It serves as both a CI test and living documentation of the v2 API.
//!
//! Run with: `cargo run --example v2_integration_validation`
//!
//! Expected output: All checks pass in <1 second with green checkmarks.

use alloy_chains::NamedChain;
use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::ProviderBuilder;
use cctp_rs::{CctpV2, CctpV2Bridge, DomainId, FastTransferFee, TransferMode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 CCTP v2 Integration Validation");
    println!("==================================\n");

    // Test Section 1: Chain Support Matrix
    validate_chain_support()?;

    // Test Section 2: Domain ID Mappings
    validate_domain_ids()?;

    // Test Section 3: Contract Addresses
    validate_contract_addresses()?;

    // Test Section 4: Bridge Configurations
    validate_bridge_configurations()?;

    // Test Section 5: API Endpoint Construction
    validate_api_endpoints()?;

    // Test Section 6: Fast Transfer Support
    validate_fast_transfer_support()?;

    // Test Section 7: Error Handling
    validate_error_handling()?;

    // Test Section 8: Cross-Chain Compatibility
    validate_cross_chain_compatibility()?;

    println!("\n✅ All validations passed!");
    println!("   v2 implementation is correctly configured and ready for use.");

    Ok(())
}

fn validate_chain_support() -> Result<(), Box<dyn std::error::Error>> {
    println!("1️⃣  Validating Chain Support Matrix");
    println!("   --------------------------------");

    // All chains that should support v2
    let v2_chains = vec![
        // v1 mainnet chains (all support v2)
        NamedChain::Mainnet,
        NamedChain::Arbitrum,
        NamedChain::Base,
        NamedChain::Optimism,
        NamedChain::Avalanche,
        NamedChain::Polygon,
        NamedChain::Unichain,
        // v2-only priority chains
        NamedChain::Linea,
        NamedChain::Sonic,
        NamedChain::Sei,
        NamedChain::Hyperliquid,
        // Testnet
        NamedChain::Sepolia,
        NamedChain::BaseSepolia,
        NamedChain::ArbitrumSepolia,
        NamedChain::OptimismSepolia,
        NamedChain::AvalancheFuji,
        NamedChain::PolygonAmoy,
    ];

    let mut mainnet_count = 0;
    let mut testnet_count = 0;

    for chain in v2_chains.iter() {
        // Verify v2 support
        assert!(
            chain.supports_cctp_v2(),
            "Chain {chain} should support v2 but doesn't"
        );

        // Count network types
        if chain.is_testnet() {
            testnet_count += 1;
        } else {
            mainnet_count += 1;
        }

        print!("   ✓ {:<20} ", format!("{chain}"));

        // Verify we can get domain ID
        let domain = chain.cctp_v2_domain_id()?;
        print!("Domain: {:>2}  ", domain.as_u32());

        // Verify contract addresses
        let tm = chain.token_messenger_v2_address()?;
        let mt = chain.message_transmitter_v2_address()?;

        println!(
            "TM: {}... MT: {}...",
            &tm.to_string()[..10],
            &mt.to_string()[..10]
        );
    }

    println!("\n   Summary:");
    println!("   • Mainnet chains: {mainnet_count}");
    println!("   • Testnet chains: {testnet_count}");
    println!("   • Total v2 chains: {}\n", mainnet_count + testnet_count);

    Ok(())
}

fn validate_domain_ids() -> Result<(), Box<dyn std::error::Error>> {
    println!("2️⃣  Validating Domain ID Mappings");
    println!("   ------------------------------");

    // Test specific known mappings from Circle's docs
    let known_mappings = vec![
        // v1 and v2 chains
        (NamedChain::Mainnet, DomainId::Ethereum, 0),
        (NamedChain::Avalanche, DomainId::Avalanche, 1),
        (NamedChain::Optimism, DomainId::Optimism, 2),
        (NamedChain::Arbitrum, DomainId::Arbitrum, 3),
        (NamedChain::Base, DomainId::Base, 6),
        (NamedChain::Polygon, DomainId::Polygon, 7),
        (NamedChain::Unichain, DomainId::Unichain, 10),
        // v2-only priority chains
        (NamedChain::Linea, DomainId::Linea, 11),
        (NamedChain::Sonic, DomainId::Sonic, 13),
        (NamedChain::Sei, DomainId::Sei, 16),
        (NamedChain::Hyperliquid, DomainId::HyperEvm, 19),
        // Testnet - share domain IDs with mainnet
        (NamedChain::Sepolia, DomainId::Ethereum, 0),
        (NamedChain::AvalancheFuji, DomainId::Avalanche, 1),
        (NamedChain::OptimismSepolia, DomainId::Optimism, 2),
        (NamedChain::ArbitrumSepolia, DomainId::Arbitrum, 3),
        (NamedChain::BaseSepolia, DomainId::Base, 6),
        (NamedChain::PolygonAmoy, DomainId::Polygon, 7),
    ];

    for (chain, expected_domain, expected_id) in known_mappings {
        let actual_domain = chain.cctp_v2_domain_id()?;

        assert_eq!(
            actual_domain, expected_domain,
            "Domain mismatch for {chain}: expected {expected_domain}, got {actual_domain}"
        );

        assert_eq!(
            actual_domain.as_u32(),
            expected_id,
            "Domain ID mismatch for {chain}: expected {expected_id}, got {}",
            actual_domain.as_u32()
        );

        println!(
            "   ✓ {:<20} → {:>2} ({})",
            format!("{chain}"),
            expected_id,
            expected_domain
        );
    }

    println!();
    Ok(())
}

fn validate_contract_addresses() -> Result<(), Box<dyn std::error::Error>> {
    println!("3️⃣  Validating Contract Addresses");
    println!("   ------------------------------");

    // Circle's official v2 mainnet addresses (unified across all chains)
    let expected_mainnet_tm: Address = "0x28b5a0e9C621a5BadaA536219b3a228C8168cf5d".parse()?;
    let expected_mainnet_mt: Address = "0x81D40F21F12A8F0E3252Bccb954D722d4c464B64".parse()?;

    // Testnet addresses
    let expected_testnet_tm: Address = "0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA".parse()?;
    let expected_testnet_mt: Address = "0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275".parse()?;

    println!("   Mainnet Address Consistency:");
    let mainnet_chains = vec![
        NamedChain::Mainnet,
        NamedChain::Arbitrum,
        NamedChain::Base,
        NamedChain::Optimism,
        NamedChain::Avalanche,
        NamedChain::Polygon,
        NamedChain::Unichain,
        NamedChain::Linea,
        NamedChain::Sonic,
        NamedChain::Sei,
        NamedChain::Hyperliquid,
    ];

    for chain in mainnet_chains {
        let tm = chain.token_messenger_v2_address()?;
        let mt = chain.message_transmitter_v2_address()?;

        assert_eq!(
            tm, expected_mainnet_tm,
            "{chain} TokenMessenger doesn't match expected mainnet address"
        );

        assert_eq!(
            mt, expected_mainnet_mt,
            "{chain} MessageTransmitter doesn't match expected mainnet address"
        );

        println!("   ✓ {:<20} Uses unified v2 addresses", format!("{chain}"));
    }

    println!("\n   Testnet Address Consistency:");
    let testnet_chains = vec![
        NamedChain::Sepolia,
        NamedChain::BaseSepolia,
        NamedChain::ArbitrumSepolia,
        NamedChain::OptimismSepolia,
        NamedChain::AvalancheFuji,
        NamedChain::PolygonAmoy,
    ];

    for chain in testnet_chains {
        let tm = chain.token_messenger_v2_address()?;
        let mt = chain.message_transmitter_v2_address()?;

        assert_eq!(
            tm, expected_testnet_tm,
            "{chain} TokenMessenger doesn't match expected testnet address"
        );

        assert_eq!(
            mt, expected_testnet_mt,
            "{chain} MessageTransmitter doesn't match expected testnet address"
        );

        println!(
            "   ✓ {:<20} Uses unified testnet addresses",
            format!("{chain}")
        );
    }

    println!();
    Ok(())
}

fn validate_bridge_configurations() -> Result<(), Box<dyn std::error::Error>> {
    println!("4️⃣  Validating Bridge Configurations");
    println!("   ---------------------------------");

    // Create a dummy provider (won't be used for network calls)
    let provider = ProviderBuilder::new().connect_http("http://localhost:8545".parse()?);

    // Test 1: Standard Transfer
    println!("   Standard Transfer:");
    let standard = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider.clone())
        .recipient(Address::ZERO)
        .build();

    assert!(
        !standard.is_fast_transfer(),
        "Standard should not have fast_transfer"
    );
    assert!(
        standard.hook_data().is_none(),
        "Standard should not have hooks"
    );
    assert_eq!(standard.finality_threshold().as_u32(), 2000);
    assert_eq!(standard.max_fee(), None);
    println!("   ✓ Finality: 2000 (finalized)");
    println!("   ✓ Fast transfer: disabled");
    println!("   ✓ Max fee: None");
    println!("   ✓ Hooks: None\n");

    // Test 2: Fast Transfer
    println!("   Fast Transfer:");
    let fast = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider.clone())
        .recipient(Address::ZERO)
        .transfer_mode(TransferMode::Fast {
            max_fee: U256::from(1000),
        })
        .build();

    assert!(
        fast.is_fast_transfer(),
        "Fast should have fast_transfer enabled"
    );
    assert!(
        fast.hook_data().is_none(),
        "Fast (without hooks) should not have hooks"
    );
    assert_eq!(fast.finality_threshold().as_u32(), 1000);
    assert_eq!(fast.max_fee(), Some(U256::from(1000)));
    println!("   ✓ Finality: 1000 (confirmed)");
    println!("   ✓ Fast transfer: enabled");
    println!("   ✓ Max fee: 1000 (0.001 USDC)");
    println!("   ✓ Hooks: None\n");

    // Test 3: Standard + Hooks
    println!("   Standard + Hooks:");
    let hook_data = Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]);
    let hooks = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider.clone())
        .recipient(Address::ZERO)
        .transfer_mode(TransferMode::StandardWithHook {
            hook_data: hook_data.clone(),
        })
        .build();

    assert!(
        !hooks.is_fast_transfer(),
        "StandardWithHook should not be fast"
    );
    assert_eq!(hooks.hook_data(), Some(&hook_data));
    assert_eq!(hooks.finality_threshold().as_u32(), 2000);
    println!("   ✓ Finality: 2000 (finalized)");
    println!("   ✓ Fast transfer: disabled");
    println!("   ✓ Hooks: Present (4 bytes)");
    println!("   ✓ Hook data: 0xdeadbeef\n");

    // Test 4: Fast + Hooks
    println!("   Fast + Hooks:");
    let fast_hooks = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .transfer_mode(TransferMode::FastWithHook {
            max_fee: U256::from(1000),
            hook_data: hook_data.clone(),
        })
        .build();

    assert!(fast_hooks.is_fast_transfer(), "Should have fast_transfer");
    assert_eq!(
        fast_hooks.hook_data(),
        Some(&hook_data),
        "Should have hooks"
    );
    assert_eq!(
        fast_hooks.finality_threshold().as_u32(),
        1000,
        "FastWithHook reports — and sends — fast finality"
    );
    assert_eq!(fast_hooks.max_fee(), Some(U256::from(1000)));
    println!("   ✓ Finality: 1000 (fast finality with hooks)");
    println!("   ✓ Fast transfer: enabled");
    println!("   ✓ Hooks: Present");
    println!("   ✓ Max fee: 1000 (passed through with hook)\n");

    Ok(())
}

fn validate_api_endpoints() -> Result<(), Box<dyn std::error::Error>> {
    println!("5️⃣  Validating API Endpoint Construction");
    println!("   -------------------------------------");

    let provider = ProviderBuilder::new().connect_http("http://localhost:8545".parse()?);

    // Test mainnet API
    println!("   Mainnet API:");
    let mainnet_bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider.clone())
        .recipient(Address::ZERO)
        .build();

    let mainnet_api = mainnet_bridge.api_url();
    assert!(
        mainnet_api
            .as_str()
            .starts_with("https://iris-api.circle.com"),
        "Mainnet should use production API"
    );
    println!(
        "   ✓ Base URL: {}",
        mainnet_api.as_str().trim_end_matches('/')
    );

    // Test URL construction with transaction hash
    let test_hash = [0xab; 32];
    let url = mainnet_bridge.create_url(test_hash.into())?;

    assert!(
        url.as_str().contains("/v2/messages/"),
        "Should use v2 messages path"
    );
    assert!(
        url.as_str().contains("transactionHash=0xabab"),
        "Should include transaction hash query param with 0x prefix"
    );
    assert!(url
        .as_str()
        .starts_with("https://iris-api.circle.com/v2/messages/"));

    println!("   ✓ Full URL format: {url}");
    println!("   ✓ Path: /v2/messages/{{domain}}?transactionHash=…");
    println!("   ✓ Hash format: 0x-prefixed\n");

    // Test testnet API
    println!("   Testnet API:");
    let testnet_bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Sepolia)
        .destination_chain(NamedChain::BaseSepolia)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .build();

    let testnet_api = testnet_bridge.api_url();
    assert!(
        testnet_api
            .as_str()
            .starts_with("https://iris-api-sandbox.circle.com"),
        "Testnet should use sandbox API"
    );
    println!(
        "   ✓ Base URL: {}",
        testnet_api.as_str().trim_end_matches('/')
    );

    let testnet_url = testnet_bridge.create_url(test_hash.into())?;
    assert!(
        testnet_url
            .as_str()
            .starts_with("https://iris-api-sandbox.circle.com/v2/messages/"),
        "Testnet should use sandbox v2 messages path"
    );
    assert!(
        testnet_url.as_str().contains("transactionHash=0xabab"),
        "Should include transaction hash query param with 0x prefix"
    );
    println!("   ✓ Uses sandbox environment");
    println!("   ✓ Full URL format: {testnet_url}\n");

    Ok(())
}

fn validate_fast_transfer_support() -> Result<(), Box<dyn std::error::Error>> {
    println!("6️⃣  Validating Fast Transfer Support");
    println!("   ---------------------------------");

    // All v2 chains should support fast transfer
    let test_chains = vec![
        NamedChain::Mainnet,
        NamedChain::Arbitrum,
        NamedChain::Base,
        NamedChain::Optimism,
        NamedChain::Linea,
        NamedChain::Sonic,
        NamedChain::Avalanche,
        NamedChain::Polygon,
        NamedChain::Sei,
        NamedChain::Unichain,
        NamedChain::Hyperliquid,
        NamedChain::Sepolia,
        NamedChain::BaseSepolia,
    ];

    for chain in test_chains {
        let supports_fast = chain.supports_fast_transfer()?;
        assert!(supports_fast, "{chain} should support fast transfer");

        let fee_bps = chain.fast_transfer_fee_bps()?;

        print!("   ✓ {:<20} Fast: Yes  ", format!("{chain}"));

        match fee_bps {
            FastTransferFee::Known(bps) => {
                assert!(bps <= 14, "Fee should be 0-14 bps, got {bps}");
                println!("Fee: {bps} bps");
            }
            FastTransferFee::Unknown => {
                println!("Static fee: unknown (use live route lookup for quotes)")
            }
            _ => println!("Fee: unrecognized FastTransferFee variant"),
        }
    }

    println!("   Live route fees are fetched with CctpV2Bridge::get_transfer_fees()");
    println!();
    Ok(())
}

fn validate_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("7️⃣  Validating Error Handling");
    println!("   --------------------------");

    // Test unsupported chain
    println!("   Testing unsupported chain error:");
    let result = NamedChain::Moonbeam.cctp_v2_domain_id();
    assert!(result.is_err(), "Moonbeam should not support v2");
    println!("   ✓ Unsupported chain returns error");

    let result = NamedChain::Moonbeam.token_messenger_v2_address();
    assert!(result.is_err(), "Should error on unsupported chain");
    println!("   ✓ Contract address query fails appropriately");

    let result = NamedChain::Moonbeam.supports_fast_transfer();
    assert!(result.is_err(), "Should error on unsupported chain");
    println!("   ✓ Fast transfer query fails appropriately\n");

    Ok(())
}

fn validate_cross_chain_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    println!("8️⃣  Validating Cross-Chain Compatibility");
    println!("   -------------------------------------");

    // Test various chain pairs
    let test_pairs = vec![
        (NamedChain::Mainnet, NamedChain::Linea, "Ethereum → Linea"),
        (NamedChain::Arbitrum, NamedChain::Base, "Arbitrum → Base"),
        (NamedChain::Base, NamedChain::Sonic, "Base → Sonic"),
        (
            NamedChain::Sepolia,
            NamedChain::BaseSepolia,
            "Sepolia → Base Sepolia",
        ),
    ];

    let provider = ProviderBuilder::new().connect_http("http://localhost:8545".parse()?);

    for (source, dest, description) in test_pairs {
        println!("   Testing: {description}");

        // Verify both chains support v2
        assert!(source.supports_cctp_v2());
        assert!(dest.supports_cctp_v2());

        // Create bridge
        let bridge = CctpV2Bridge::builder()
            .source_chain(source)
            .destination_chain(dest)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();

        // Verify domain IDs
        let source_domain = bridge.source_chain().cctp_v2_domain_id()?;
        let dest_domain = bridge.destination_domain_id()?;

        println!("   ✓ Source domain: {source_domain}");
        println!("   ✓ Dest domain: {dest_domain}");

        // Verify contract addresses
        let _tm = bridge.token_messenger_v2_contract()?;
        let _mt = bridge.message_transmitter_v2_contract()?;

        println!("   ✓ Contract addresses resolved");

        // Verify API URL is correct for source chain's network
        let api_url = bridge.api_url();
        if source.is_testnet() {
            assert!(api_url.as_str().contains("sandbox"));
        } else {
            assert!(!api_url.as_str().contains("sandbox"));
        }
        println!("   ✓ API environment matches network\n");
    }

    Ok(())
}
