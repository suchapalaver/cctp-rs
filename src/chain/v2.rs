// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 chain configuration trait
//!
//! This module defines the `CctpV2` trait which provides v2-specific
//! chain capabilities including Fast Transfer support, hooks, and
//! v2 contract addresses.

use alloy_chains::NamedChain;
use alloy_primitives::Address;

use super::addresses::{
    CCTP_V2_MESSAGE_TRANSMITTER_MAINNET, CCTP_V2_MESSAGE_TRANSMITTER_TESTNET,
    CCTP_V2_TOKEN_MESSENGER_MAINNET, CCTP_V2_TOKEN_MESSENGER_TESTNET,
};
use crate::{CctpError, DomainId, Result};

/// Static Fast Transfer fee metadata for a CCTP v2 chain, in basis points.
///
/// This enum is retained for chain-level/static metadata. Current CCTP v2
/// fees are route-aware and should be fetched with
/// [`CctpV2Bridge::get_transfer_fees`](crate::CctpV2Bridge::get_transfer_fees)
/// or
/// [`CctpV2Bridge::calculate_fast_transfer_max_fee`](crate::CctpV2Bridge::calculate_fast_transfer_max_fee)
/// before quoting or sending a Fast Transfer. Until a chain's static fee has
/// been confirmed against an authoritative source, this SDK represents it as
/// [`FastTransferFee::Unknown`] rather than asserting a numeric value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FastTransferFee {
    /// Fee is confirmed at this value in basis points (0-14).
    ///
    /// A `Known(0)` is a sourced zero fee, semantically distinct from
    /// [`FastTransferFee::Unknown`].
    Known(u32),
    /// Fee data has not yet been confirmed for this chain.
    ///
    /// Callers must not assume zero. Coercing `Unknown` to zero would
    /// reintroduce the placeholder behavior this enum exists to
    /// prevent — Circle's published range is 0-14 bps, so a default
    /// of zero is plausible enough to mask a real fee from downstream
    /// consumers.
    Unknown,
}

/// CCTP v2 chain configuration trait
///
/// Implemented on `alloy_chains::NamedChain` to provide v2-specific
/// configuration for each supported blockchain network.
///
/// # v2 Features
///
/// - **Fast Transfer**: Chains that support fast transfer (finality threshold 1000)
/// - **Dynamic Fees**: Some chains charge fees for fast transfer (0-14 bps)
/// - **v2 Contracts**: Updated contract addresses for `TokenMessengerV2` and `MessageTransmitterV2`
/// - **Expanded Chains**: Bridge SDK routes 11 v2-capable chain families
///   (the 7 v1 families plus Linea, Sonic, Sei, HyperEVM) with testnets,
///   versus the 7 v1 chain families. Note that this trait covers bridge
///   SDK reach; the protocol parser (`DomainId`, `ParsedV2Message`) can decode
///   all 31 domain IDs in Circle's current CCTP domain table
///   independently of bridge support.
///
/// # Example
///
/// ```rust
/// use cctp_rs::CctpV2;
/// use alloy_chains::NamedChain;
///
/// let chain = NamedChain::Mainnet;
/// assert!(chain.supports_cctp_v2());
/// assert!(chain.supports_fast_transfer().unwrap());
/// ```
pub trait CctpV2 {
    /// Returns true if this chain supports CCTP v2
    ///
    /// This covers the crate's validated bridge SDK routes, not every
    /// parser-modeled CCTP domain.
    fn supports_cctp_v2(&self) -> bool;

    /// Returns true if this chain supports Fast Transfer
    ///
    /// Fast Transfer is a source-chain capability. Circle marks it N/A when a
    /// chain's Standard Transfer attestation is already fast enough that the
    /// lower finality threshold provides no meaningful improvement.
    fn supports_fast_transfer(&self) -> Result<bool>;

    /// Returns true if this source deployment exposes Circle's Standard
    /// Transfer fee switch and its `getMinFeeAmount(uint256)` selector.
    ///
    /// This is deliberately deployment-gated: calling the selector on an
    /// older `TokenMessengerV2` deployment reverts. Circle currently lists
    /// this capability for Sei only.
    ///
    /// See <https://developers.circle.com/cctp/concepts/fees>.
    fn supports_standard_transfer_fee_switch(&self) -> Result<bool> {
        Ok(false)
    }

    /// Reports whether static fast transfer fee metadata has been sourced for
    /// this chain, and if so its value in basis points.
    ///
    /// This helper is not the production path for current route fees. CCTP v2
    /// fees are route-aware; use
    /// [`CctpV2Bridge::get_transfer_fees`](crate::CctpV2Bridge::get_transfer_fees),
    /// [`CctpV2Bridge::get_fast_transfer_fee`](crate::CctpV2Bridge::get_fast_transfer_fee),
    /// or
    /// [`CctpV2Bridge::calculate_fast_transfer_max_fee`](crate::CctpV2Bridge::calculate_fast_transfer_max_fee)
    /// when preparing a `maxFee` for user funds.
    ///
    /// Returns [`FastTransferFee::Unknown`] when the chain-level/static fee has
    /// not been confirmed against an authoritative source. This is the current
    /// state for every v2 chain in this SDK. Callers handling user funds must
    /// not coerce `Unknown` to zero.
    ///
    /// Errors if the chain doesn't support CCTP v2 or Fast Transfer is N/A for
    /// the chain as a transfer source.
    #[must_use = "ignoring the fast transfer fee can mis-quote a transfer; \
                  Unknown must not be coerced to zero"]
    fn fast_transfer_fee_bps(&self) -> Result<FastTransferFee>;

    /// Returns the `TokenMessengerV2` contract address for this chain
    ///
    /// Returns an error if the chain doesn't support CCTP v2 or if
    /// contracts haven't been deployed yet.
    fn token_messenger_v2_address(&self) -> Result<Address>;

    /// Returns the `MessageTransmitterV2` contract address for this chain
    ///
    /// Returns an error if the chain doesn't support CCTP v2 or if
    /// contracts haven't been deployed yet.
    fn message_transmitter_v2_address(&self) -> Result<Address>;

    /// Returns the CCTP domain ID for this chain
    ///
    /// Note: Domain IDs are the same in v1 and v2 for chains that
    /// existed in v1. New v2-only chains have domain IDs >= 11.
    fn cctp_v2_domain_id(&self) -> Result<DomainId>;

    /// Returns the average Fast Transfer attestation time in seconds
    ///
    /// Fast Transfer uses a lower finality threshold (≤1000) to achieve
    /// rapid attestations at the cost of a small fee on some chains.
    ///
    /// Typical times on Fast-capable sources:
    /// - Ethereum: ~20 seconds (2 block confirmations)
    /// - Supported L2s: ~8 seconds (1 block confirmation)
    ///
    /// See: <https://developers.circle.com/stablecoins/required-block-confirmations>
    ///
    /// Returns [`CctpError::FastTransferUnsupported`] when Circle marks Fast
    /// Transfer N/A for this source chain.
    fn fast_transfer_confirmation_time_seconds(&self) -> Result<u64>;

    /// Returns the average Standard Transfer attestation time in seconds
    ///
    /// Standard Transfer waits for full chain finality before Circle's Iris
    /// service provides an attestation. This is the default behavior.
    ///
    /// Typical times:
    /// - Ethereum + L2s settling to Ethereum: 13-19 minutes (~65 ETH blocks)
    /// - Avalanche, Polygon: 5-20 seconds (native finality)
    /// - Sei, Sonic: ~5 seconds (high-performance chains)
    /// - Linea: 6-32 hours (zkEVM proof generation)
    ///
    /// See: <https://developers.circle.com/stablecoins/required-block-confirmations>
    fn standard_transfer_confirmation_time_seconds(&self) -> Result<u64>;
}

impl CctpV2 for NamedChain {
    fn supports_cctp_v2(&self) -> bool {
        matches!(
            self,
            // v1 chains (all support v2)
            Self::Mainnet
                | Self::Sepolia
                | Self::Arbitrum
                | Self::ArbitrumSepolia
                | Self::Base
                | Self::BaseSepolia
                | Self::Optimism
                | Self::OptimismSepolia
                | Self::Avalanche
                | Self::AvalancheFuji
                | Self::Polygon
                | Self::PolygonAmoy
                | Self::Unichain
                // v2-only priority chains
                // (BNB Smart Chain / domain 17 omitted: USYC-only on this domain)
                | Self::Linea
                | Self::Sonic
                | Self::Sei
                | Self::Hyperliquid
        )
    }

    fn supports_fast_transfer(&self) -> Result<bool> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        // Fast Transfer is a source-chain capability, not a property shared by
        // every CCTP v2 deployment. Circle marks chains with already-fast
        // standard attestation as N/A.
        Ok(matches!(
            self,
            Self::Mainnet
                | Self::Sepolia
                | Self::Arbitrum
                | Self::ArbitrumSepolia
                | Self::Base
                | Self::BaseSepolia
                | Self::Optimism
                | Self::OptimismSepolia
                | Self::Unichain
                | Self::Linea
        ))
    }

    fn supports_standard_transfer_fee_switch(&self) -> Result<bool> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        Ok(matches!(self, Self::Sei))
    }

    fn fast_transfer_fee_bps(&self) -> Result<FastTransferFee> {
        if !self.supports_fast_transfer()? {
            return Err(CctpError::FastTransferUnsupported {
                source_chain: *self,
            });
        }

        // Per-chain fees have not been sourced against Circle's
        // published values; until they are, every chain reports
        // Unknown rather than a placeholder zero.
        Ok(FastTransferFee::Unknown)
    }

    fn token_messenger_v2_address(&self) -> Result<Address> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        // V2 uses unified addresses across all chains within each environment
        Ok(if self.is_testnet() {
            CCTP_V2_TOKEN_MESSENGER_TESTNET
        } else {
            CCTP_V2_TOKEN_MESSENGER_MAINNET
        })
    }

    fn message_transmitter_v2_address(&self) -> Result<Address> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        // V2 uses unified addresses across all chains within each environment
        Ok(if self.is_testnet() {
            CCTP_V2_MESSAGE_TRANSMITTER_TESTNET
        } else {
            CCTP_V2_MESSAGE_TRANSMITTER_MAINNET
        })
    }

    fn cctp_v2_domain_id(&self) -> Result<DomainId> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        Ok(match self {
            // v1 and v2 chains
            Self::Mainnet | Self::Sepolia => DomainId::Ethereum,
            Self::Avalanche | Self::AvalancheFuji => DomainId::Avalanche,
            Self::Optimism | Self::OptimismSepolia => DomainId::Optimism,
            Self::Arbitrum | Self::ArbitrumSepolia => DomainId::Arbitrum,
            Self::Base | Self::BaseSepolia => DomainId::Base,
            Self::Polygon | Self::PolygonAmoy => DomainId::Polygon,
            Self::Unichain => DomainId::Unichain,
            // v2-only priority chains
            Self::Linea => DomainId::Linea,
            Self::Sonic => DomainId::Sonic,
            Self::Sei => DomainId::Sei,
            Self::Hyperliquid => DomainId::HyperEvm,
            // This is unreachable due to supports_cctp_v2() check above
            _ => return Err(CctpError::UnsupportedChain(*self)),
        })
    }

    fn fast_transfer_confirmation_time_seconds(&self) -> Result<u64> {
        if !self.supports_fast_transfer()? {
            return Err(CctpError::FastTransferUnsupported {
                source_chain: *self,
            });
        }

        // Fast Transfer attestation times (1-2 block confirmations)
        // Based on Circle docs: https://developers.circle.com/stablecoins/required-block-confirmations
        Ok(match self {
            // Ethereum: ~20 seconds (2 block confirmations)
            Self::Mainnet | Self::Sepolia => 20,
            // Arbitrum: ~8 seconds (1 block confirmation)
            Self::Arbitrum | Self::ArbitrumSepolia => 8,
            // Base: ~8 seconds (1 block confirmation)
            Self::Base | Self::BaseSepolia => 8,
            // Optimism: ~8 seconds (1 block confirmation)
            Self::Optimism | Self::OptimismSepolia => 8,
            // Unichain: ~8 seconds (1 block confirmation)
            Self::Unichain => 8,
            // Linea: ~8 seconds (vs 6-32 hours for Standard!)
            Self::Linea => 8,
            _ => return Err(CctpError::UnsupportedChain(*self)),
        })
    }

    fn standard_transfer_confirmation_time_seconds(&self) -> Result<u64> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        // Standard Transfer attestation times (full finality)
        // Based on Circle docs: https://developers.circle.com/stablecoins/required-block-confirmations
        Ok(match self {
            // Ethereum L1 + L2s settling to Ethereum: 13-19 minutes (~65 ETH blocks)
            Self::Mainnet | Self::Sepolia => 19 * 60,
            Self::Arbitrum | Self::ArbitrumSepolia => 19 * 60,
            Self::Base | Self::BaseSepolia => 19 * 60,
            Self::Optimism | Self::OptimismSepolia => 19 * 60,
            Self::Unichain => 19 * 60,
            // Avalanche: ~20 seconds (native finality)
            Self::Avalanche | Self::AvalancheFuji => 20,
            // Polygon: ~8 minutes (PoS finality)
            Self::Polygon | Self::PolygonAmoy => 8 * 60,
            // Linea: 6-32 hours (zkEVM proof generation) - use conservative 8 hours
            Self::Linea => 8 * 60 * 60,
            // Sonic: ~5 seconds (high-performance chain, native finality)
            Self::Sonic => 5,
            // Sei: ~5 seconds (parallel EVM, native finality)
            Self::Sei => 5,
            // HyperEVM: ~5 seconds (1 block confirmation, native finality)
            Self::Hyperliquid => 5,
            _ => return Err(CctpError::UnsupportedChain(*self)),
        })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(NamedChain::Mainnet, true)]
    #[case(NamedChain::Arbitrum, true)]
    #[case(NamedChain::Base, true)]
    #[case(NamedChain::Linea, true)]
    #[case(NamedChain::Sonic, true)]
    #[case(NamedChain::Sei, true)]
    #[case(NamedChain::Hyperliquid, true)]
    #[case(NamedChain::BinanceSmartChain, false)]
    #[case(NamedChain::Moonbeam, false)]
    fn test_v2_chain_support(#[case] chain: NamedChain, #[case] expected: bool) {
        assert_eq!(chain.supports_cctp_v2(), expected);
    }

    #[rstest]
    #[case(NamedChain::Mainnet, true)]
    #[case(NamedChain::Sepolia, true)]
    #[case(NamedChain::Arbitrum, true)]
    #[case(NamedChain::ArbitrumSepolia, true)]
    #[case(NamedChain::Base, true)]
    #[case(NamedChain::BaseSepolia, true)]
    #[case(NamedChain::Optimism, true)]
    #[case(NamedChain::OptimismSepolia, true)]
    #[case(NamedChain::Unichain, true)]
    #[case(NamedChain::Linea, true)]
    #[case(NamedChain::Avalanche, false)]
    #[case(NamedChain::AvalancheFuji, false)]
    #[case(NamedChain::Polygon, false)]
    #[case(NamedChain::PolygonAmoy, false)]
    #[case(NamedChain::Sonic, false)]
    #[case(NamedChain::Sei, false)]
    #[case(NamedChain::Hyperliquid, false)]
    fn fast_transfer_is_a_source_chain_capability(
        #[case] chain: NamedChain,
        #[case] expected: bool,
    ) {
        assert_eq!(chain.supports_fast_transfer().unwrap(), expected);
    }

    #[rstest]
    #[case(NamedChain::Mainnet, false)]
    #[case(NamedChain::Sepolia, false)]
    #[case(NamedChain::Arbitrum, false)]
    #[case(NamedChain::ArbitrumSepolia, false)]
    #[case(NamedChain::Base, false)]
    #[case(NamedChain::BaseSepolia, false)]
    #[case(NamedChain::Optimism, false)]
    #[case(NamedChain::OptimismSepolia, false)]
    #[case(NamedChain::Avalanche, false)]
    #[case(NamedChain::AvalancheFuji, false)]
    #[case(NamedChain::Polygon, false)]
    #[case(NamedChain::PolygonAmoy, false)]
    #[case(NamedChain::Unichain, false)]
    #[case(NamedChain::Linea, false)]
    #[case(NamedChain::Sonic, false)]
    #[case(NamedChain::Sei, true)]
    #[case(NamedChain::Hyperliquid, false)]
    fn standard_transfer_fee_switch_is_a_source_deployment_capability(
        #[case] chain: NamedChain,
        #[case] expected: bool,
    ) {
        assert_eq!(
            chain.supports_standard_transfer_fee_switch().unwrap(),
            expected
        );
    }

    #[test]
    fn standard_transfer_fee_switch_rejects_unsupported_chain() {
        assert!(NamedChain::Moonbeam
            .supports_standard_transfer_fee_switch()
            .is_err());
    }

    #[test]
    fn fast_transfer_support_matches_circle_capability_snapshot() {
        let snapshot: serde_json::Value = serde_json::from_str(include_str!(
            "../../scripts/fixtures/circle_cctp_protocol_snapshot.json"
        ))
        .expect("checked-in Circle capability snapshot must be valid JSON");
        let rows = snapshot["capabilities"]
            .as_array()
            .expect("snapshot capabilities must be an array");
        let supported_mainnets = [
            ("Ethereum", NamedChain::Mainnet),
            ("Arbitrum", NamedChain::Arbitrum),
            ("Base", NamedChain::Base),
            ("OP Mainnet", NamedChain::Optimism),
            ("Avalanche", NamedChain::Avalanche),
            ("Polygon PoS", NamedChain::Polygon),
            ("Unichain", NamedChain::Unichain),
            ("Linea", NamedChain::Linea),
            ("Sonic", NamedChain::Sonic),
            ("Sei", NamedChain::Sei),
            ("HyperEVM", NamedChain::Hyperliquid),
        ];

        for (published_name, chain) in supported_mainnets {
            let row = rows
                .iter()
                .find(|row| row["blockchain"] == published_name)
                .unwrap_or_else(|| panic!("missing {published_name} capability row"));
            let published_support = match &row["fast_transfer"] {
                serde_json::Value::Bool(value) => *value,
                serde_json::Value::Null => false,
                value => panic!("unexpected Fast Transfer value for {published_name}: {value}"),
            };

            assert_eq!(
                chain.supports_fast_transfer().unwrap(),
                published_support,
                "{chain} must match Circle's source-chain Fast Transfer capability"
            );
        }

        assert!(NamedChain::Moonbeam.supports_fast_transfer().is_err());
    }

    #[rstest]
    // Fast-capable v1 mainnets that also support v2
    #[case(NamedChain::Mainnet)]
    #[case(NamedChain::Arbitrum)]
    #[case(NamedChain::Base)]
    #[case(NamedChain::Optimism)]
    #[case(NamedChain::Unichain)]
    // Fast-capable v2-only mainnets
    #[case(NamedChain::Linea)]
    // Fast-capable v1 testnets that also support v2
    #[case(NamedChain::Sepolia)]
    #[case(NamedChain::ArbitrumSepolia)]
    #[case(NamedChain::BaseSepolia)]
    #[case(NamedChain::OptimismSepolia)]
    fn fast_transfer_fee_is_unknown_until_sourced(#[case] chain: NamedChain) {
        // Per-chain values are not sourced yet, so every fast-capable
        // chain must report Unknown — never a placeholder Known(0)
        // that would look like a confirmed fee to callers.
        assert_eq!(
            chain.fast_transfer_fee_bps().unwrap(),
            FastTransferFee::Unknown
        );
    }

    #[test]
    fn fast_transfer_fee_unknown_is_distinct_from_known_zero() {
        // Sanity: Known(0) and Unknown are not equal. This is the
        // invariant issue #215 cared about — a confirmed-zero fee must
        // not collide with the "we haven't sourced this" state.
        assert_ne!(FastTransferFee::Known(0), FastTransferFee::Unknown);
    }

    #[test]
    fn fast_transfer_fee_errors_for_unsupported_chain() {
        assert!(NamedChain::Moonbeam.fast_transfer_fee_bps().is_err());
    }

    #[rstest]
    #[case(NamedChain::Avalanche)]
    #[case(NamedChain::Polygon)]
    #[case(NamedChain::Sonic)]
    #[case(NamedChain::Sei)]
    #[case(NamedChain::Hyperliquid)]
    fn fast_transfer_metadata_rejects_n_a_source(#[case] chain: NamedChain) {
        assert!(matches!(
            chain.fast_transfer_fee_bps(),
            Err(CctpError::FastTransferUnsupported { source_chain }) if source_chain == chain
        ));
        assert!(matches!(
            chain.fast_transfer_confirmation_time_seconds(),
            Err(CctpError::FastTransferUnsupported { source_chain }) if source_chain == chain
        ));
    }

    #[test]
    fn test_domain_id_mapping() {
        // v1 chains
        assert_eq!(
            NamedChain::Mainnet.cctp_v2_domain_id().unwrap(),
            DomainId::Ethereum
        );
        assert_eq!(
            NamedChain::Arbitrum.cctp_v2_domain_id().unwrap(),
            DomainId::Arbitrum
        );

        // v2-only chains
        assert_eq!(
            NamedChain::Linea.cctp_v2_domain_id().unwrap(),
            DomainId::Linea
        );
        assert_eq!(
            NamedChain::Sonic.cctp_v2_domain_id().unwrap(),
            DomainId::Sonic
        );
        assert_eq!(NamedChain::Sei.cctp_v2_domain_id().unwrap(), DomainId::Sei);
        assert_eq!(
            NamedChain::Hyperliquid.cctp_v2_domain_id().unwrap(),
            DomainId::HyperEvm
        );
    }

    #[test]
    fn test_contract_addresses() {
        // Mainnet chains should return mainnet addresses
        let linea_tm = NamedChain::Linea.token_messenger_v2_address().unwrap();
        let linea_mt = NamedChain::Linea.message_transmitter_v2_address().unwrap();
        assert_eq!(linea_tm, CCTP_V2_TOKEN_MESSENGER_MAINNET);
        assert_eq!(linea_mt, CCTP_V2_MESSAGE_TRANSMITTER_MAINNET);

        let sonic_tm = NamedChain::Sonic.token_messenger_v2_address().unwrap();
        let sonic_mt = NamedChain::Sonic.message_transmitter_v2_address().unwrap();
        assert_eq!(sonic_tm, CCTP_V2_TOKEN_MESSENGER_MAINNET);
        assert_eq!(sonic_mt, CCTP_V2_MESSAGE_TRANSMITTER_MAINNET);

        // All mainnet chains should have the same v2 addresses
        assert_eq!(linea_tm, sonic_tm);
        assert_eq!(linea_mt, sonic_mt);
    }

    #[test]
    fn test_fast_transfer_confirmation_times() {
        // Fast Transfer: 1-2 block confirmations
        // Ethereum: 20 seconds (2 blocks)
        assert_eq!(
            NamedChain::Mainnet
                .fast_transfer_confirmation_time_seconds()
                .unwrap(),
            20
        );
        // L2s and most chains: 8 seconds (1 block)
        assert_eq!(
            NamedChain::Arbitrum
                .fast_transfer_confirmation_time_seconds()
                .unwrap(),
            8
        );
        assert_eq!(
            NamedChain::Linea
                .fast_transfer_confirmation_time_seconds()
                .unwrap(),
            8
        );
    }

    #[test]
    fn test_standard_transfer_confirmation_times() {
        // Standard Transfer: full finality required
        // Ethereum + L2s: 19 minutes (~65 ETH blocks)
        assert_eq!(
            NamedChain::Mainnet
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            19 * 60
        );
        assert_eq!(
            NamedChain::Arbitrum
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            19 * 60
        );
        assert_eq!(
            NamedChain::Base
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            19 * 60
        );
        // Avalanche: 20 seconds (native finality)
        assert_eq!(
            NamedChain::Avalanche
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            20
        );
        // Polygon: 8 minutes
        assert_eq!(
            NamedChain::Polygon
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            8 * 60
        );
        // Linea: 8 hours (zkEVM proof generation)
        assert_eq!(
            NamedChain::Linea
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            8 * 60 * 60
        );
        // High-performance chains: same as fast (already fast natively)
        assert_eq!(
            NamedChain::Sonic
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            5
        );
        assert_eq!(
            NamedChain::Sei
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            5
        );
        assert_eq!(
            NamedChain::Hyperliquid
                .standard_transfer_confirmation_time_seconds()
                .unwrap(),
            5
        );
    }
}
