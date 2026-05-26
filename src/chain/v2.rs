// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
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

/// Fast Transfer fee for a CCTP v2 chain, in basis points.
///
/// Circle's documentation states fast transfer fees range from 0 to 14
/// basis points and are configured per chain. Until a chain's fee has
/// been confirmed against an authoritative source, this SDK represents
/// it as [`FastTransferFee::Unknown`] rather than asserting a numeric
/// value. Callers handling user funds should treat `Unknown` as a
/// signal to fetch the live fee on-chain or via Circle's APIs before
/// quoting it as zero.
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
/// - **Expanded Chains**: Bridge SDK routes 10 v2-capable chain families
///   (the 7 v1 families plus Linea, Sonic, Sei) with testnets, versus the
///   7 v1 chain families. Note that this trait covers bridge SDK reach —
///   Circle has announced 21 CCTP v2 domain IDs in total, which the
///   protocol parser (`DomainId`, `ParsedV2Message`) can decode
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
    /// All v1 chains support v2, plus 19 additional v2-only chains.
    fn supports_cctp_v2(&self) -> bool;

    /// Returns true if this chain supports Fast Transfer
    ///
    /// Fast Transfer enables ~30 second settlement times vs 13-19 minutes.
    fn supports_fast_transfer(&self) -> Result<bool>;

    /// Reports whether a fast transfer fee has been sourced for this
    /// chain, and if so its value in basis points.
    ///
    /// Returns [`FastTransferFee::Unknown`] when the chain's fee has
    /// not been confirmed against an authoritative source. This is
    /// the current state for every v2 chain in this SDK — Circle's
    /// docs name a 0-14 bps range, but per-chain values must be
    /// sourced before being claimed here. Callers handling user
    /// funds must not coerce `Unknown` to zero; fetch the live fee
    /// from Circle or on-chain instead.
    ///
    /// Errors if the chain doesn't support CCTP v2.
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
    /// Typical times:
    /// - Ethereum: ~20 seconds (2 block confirmations)
    /// - Most L2s and alt-L1s: ~8 seconds (1 block confirmation)
    /// - High-performance chains (Sonic, Sei): ~5 seconds
    ///
    /// See: <https://developers.circle.com/stablecoins/required-block-confirmations>
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
        )
    }

    fn supports_fast_transfer(&self) -> Result<bool> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
        }

        // All v2 chains support fast transfer
        Ok(true)
    }

    fn fast_transfer_fee_bps(&self) -> Result<FastTransferFee> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
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
            // This is unreachable due to supports_cctp_v2() check above
            _ => return Err(CctpError::UnsupportedChain(*self)),
        })
    }

    fn fast_transfer_confirmation_time_seconds(&self) -> Result<u64> {
        if !self.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(*self));
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
            // Avalanche: ~8 seconds (1 block confirmation)
            Self::Avalanche | Self::AvalancheFuji => 8,
            // Polygon: ~8 seconds (1 block confirmation)
            Self::Polygon | Self::PolygonAmoy => 8,
            // Unichain: ~8 seconds (1 block confirmation)
            Self::Unichain => 8,
            // Linea: ~8 seconds (vs 6-32 hours for Standard!)
            Self::Linea => 8,
            // Sonic: ~5 seconds (high-performance chain)
            Self::Sonic => 5,
            // Sei: ~5 seconds (parallel EVM)
            Self::Sei => 5,
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
    #[case(NamedChain::BinanceSmartChain, false)]
    #[case(NamedChain::Moonbeam, false)]
    fn test_v2_chain_support(#[case] chain: NamedChain, #[case] expected: bool) {
        assert_eq!(chain.supports_cctp_v2(), expected);
    }

    #[test]
    fn test_fast_transfer_support() {
        // All v2 chains support fast transfer
        assert!(NamedChain::Mainnet.supports_fast_transfer().unwrap());
        assert!(NamedChain::Linea.supports_fast_transfer().unwrap());
        assert!(NamedChain::Sonic.supports_fast_transfer().unwrap());

        // Unsupported chain returns error
        assert!(NamedChain::Moonbeam.supports_fast_transfer().is_err());
    }

    #[rstest]
    // v1 mainnets that also support v2
    #[case(NamedChain::Mainnet)]
    #[case(NamedChain::Arbitrum)]
    #[case(NamedChain::Base)]
    #[case(NamedChain::Optimism)]
    #[case(NamedChain::Avalanche)]
    #[case(NamedChain::Polygon)]
    #[case(NamedChain::Unichain)]
    // v2-only mainnets
    #[case(NamedChain::Linea)]
    #[case(NamedChain::Sonic)]
    #[case(NamedChain::Sei)]
    // v1 testnets that also support v2
    #[case(NamedChain::Sepolia)]
    #[case(NamedChain::ArbitrumSepolia)]
    #[case(NamedChain::BaseSepolia)]
    #[case(NamedChain::OptimismSepolia)]
    #[case(NamedChain::AvalancheFuji)]
    #[case(NamedChain::PolygonAmoy)]
    fn fast_transfer_fee_is_unknown_until_sourced(#[case] chain: NamedChain) {
        // Per-chain values are not sourced yet, so every supported v2
        // chain must report Unknown — never a placeholder Known(0)
        // that would look like a confirmed fee to callers. Exhaustive
        // over every variant matched by `supports_cctp_v2()` so a
        // future variant that accidentally returns Err is caught.
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
        // High-performance chains: 5 seconds
        assert_eq!(
            NamedChain::Sonic
                .fast_transfer_confirmation_time_seconds()
                .unwrap(),
            5
        );
        assert_eq!(
            NamedChain::Sei
                .fast_transfer_confirmation_time_seconds()
                .unwrap(),
            5
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
    }
}
