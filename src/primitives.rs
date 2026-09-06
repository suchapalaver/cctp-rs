// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Shared domain primitives for CCTP applications.

use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::chain::addresses;
use crate::{CctpError, CctpV2, DomainId, Result};

const USDC_SCALE: u128 = 1_000_000;

/// CCTP transfer asset modeled by the SDK.
///
/// `CctpV2Bridge` still exposes raw token-address methods for direct contract
/// workflows. Use this enum with the asset-aware helpers when applications want
/// the SDK to validate that the selected token is a Circle-supported CCTP asset
/// for the route before a burn transaction is submitted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum CctpTransferAsset {
    /// Circle USD Coin.
    #[default]
    Usdc,
    /// Circle Euro Coin / EURC.
    Eurc,
    /// Circle US Yield Coin.
    Usyc,
}

impl CctpTransferAsset {
    /// Returns the canonical token symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Usdc => "USDC",
            Self::Eurc => "EURC",
            Self::Usyc => "USYC",
        }
    }

    /// Returns true when this asset is configured for the given bridge chain.
    ///
    /// This is the bridge SDK support boundary, not Circle's full protocol
    /// domain table. In particular, USYC is not enabled here because Circle's
    /// documented USYC route depends on BNB Smart Chain, which this crate does
    /// not yet support as an EVM bridge route.
    #[must_use]
    pub fn is_supported_on_v2_bridge_chain(self, chain: NamedChain) -> bool {
        match self {
            Self::Usdc => chain.supports_cctp_v2(),
            Self::Eurc => matches!(
                chain,
                NamedChain::Mainnet
                    | NamedChain::Sepolia
                    | NamedChain::Base
                    | NamedChain::BaseSepolia
            ),
            Self::Usyc => false,
        }
    }

    /// Returns the EVM token address for this asset on a supported CCTP v2
    /// bridge chain.
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::UnsupportedAssetChain`] when the crate has no
    /// validated EVM token address for the asset on `chain`.
    pub fn v2_bridge_token_address(self, chain: NamedChain) -> Result<Address> {
        match self {
            Self::Usdc => usdc_evm_token_address(chain).ok_or_else(|| {
                unsupported_asset_chain(self, chain, "USDC is not configured for this bridge chain")
            }),
            Self::Eurc => eurc_evm_token_address(chain).ok_or_else(|| {
                unsupported_asset_chain(
                    self,
                    chain,
                    "EURC CCTP is currently modeled only for Ethereum and Base",
                )
            }),
            Self::Usyc => Err(unsupported_asset_chain(
                self,
                chain,
                "USYC routes require BNB Smart Chain, which the EVM bridge SDK does not support",
            )),
        }
    }

    /// Validates that this asset is supported for the route.
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::UnsupportedAssetRoute`] when the route is not a
    /// supported bridge route for the selected asset.
    pub fn validate_v2_route(self, route: CctpV2Route) -> Result<()> {
        match self {
            Self::Usdc => {
                let source = route.source_chain();
                let destination = route.destination_chain();
                if !self.is_supported_on_v2_bridge_chain(source) {
                    return Err(unsupported_asset_route(
                        self,
                        source,
                        destination,
                        "USDC is not configured for the source bridge chain",
                    ));
                }
                if !self.is_supported_on_v2_bridge_chain(destination) {
                    return Err(unsupported_asset_route(
                        self,
                        source,
                        destination,
                        "USDC is not configured for the destination bridge chain",
                    ));
                }
                Ok(())
            }
            Self::Eurc => {
                let source = route.source_chain();
                let destination = route.destination_chain();
                if matches!(
                    (source, destination),
                    (NamedChain::Mainnet, NamedChain::Base)
                        | (NamedChain::Base, NamedChain::Mainnet)
                        | (NamedChain::Sepolia, NamedChain::BaseSepolia)
                        | (NamedChain::BaseSepolia, NamedChain::Sepolia)
                ) {
                    Ok(())
                } else {
                    Err(unsupported_asset_route(
                        self,
                        source,
                        destination,
                        "EURC CCTP bridge routes are currently modeled only for Ethereum <-> Base",
                    ))
                }
            }
            Self::Usyc => Err(unsupported_asset_route(
                self,
                route.source_chain(),
                route.destination_chain(),
                "USYC is Circle-documented only for Ethereum and BNB Smart Chain; BNB routing is not implemented in this EVM bridge SDK",
            )),
        }
    }

    pub(crate) const fn iris_fee_path_segment(self) -> Option<&'static str> {
        match self {
            Self::Usdc => Some("USDC"),
            Self::Eurc | Self::Usyc => None,
        }
    }

    pub(crate) const fn fee_endpoint_unavailable_reason(self) -> &'static str {
        match self {
            Self::Usdc => "USDC fee endpoint is expected to be available",
            Self::Eurc => {
                "Circle's public Iris fee API is still documented as USDC-only for burn fee lookups"
            }
            Self::Usyc => "Circle's public Iris fee API does not publish a USYC burn fee endpoint",
        }
    }
}

impl fmt::Display for CctpTransferAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// A USDC amount stored in atomic units.
///
/// CCTP burns and mints USDC in atomic units, but CLIs and wallet UIs normally
/// accept decimal USDC. This type keeps decimal parsing at the SDK boundary so
/// applications do not each reinvent six-decimal validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct UsdcAmount {
    atomic: U256,
}

impl UsdcAmount {
    /// Creates an amount from atomic USDC units.
    #[must_use]
    pub const fn from_atomic(atomic: U256) -> Self {
        Self { atomic }
    }

    /// Parses a decimal USDC amount using USDC's six decimal places.
    ///
    /// Examples: `"1"`, `"1.25"`, `".5"`, and `"0.000001"` are valid.
    /// Values with more than six decimal places are rejected.
    pub fn parse_decimal(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(invalid_amount("amount must not be empty"));
        }
        if trimmed.starts_with('-') || trimmed.starts_with('+') {
            return Err(invalid_amount("amount must be unsigned"));
        }

        let mut parts = trimmed.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if parts.next().is_some() {
            return Err(invalid_amount(
                "amount must contain at most one decimal point",
            ));
        }

        if whole.is_empty() && fraction.is_none_or(str::is_empty) {
            return Err(invalid_amount("amount must include digits"));
        }
        if !whole.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid_amount(
                "amount whole-number part must contain only digits",
            ));
        }

        let whole_units = if whole.is_empty() {
            0
        } else {
            whole
                .parse::<u128>()
                .map_err(|_| invalid_amount("amount whole-number part is too large"))?
        };

        let fractional_units = match fraction {
            Some(value) => parse_usdc_fraction(value)?,
            None => 0,
        };

        let atomic_units = whole_units
            .checked_mul(USDC_SCALE)
            .and_then(|value| value.checked_add(fractional_units))
            .ok_or_else(|| invalid_amount("amount is too large"))?;

        if atomic_units == 0 {
            return Err(invalid_amount("amount must be greater than zero"));
        }

        Ok(Self {
            atomic: U256::from(atomic_units),
        })
    }

    /// Returns the amount in atomic USDC units.
    #[must_use]
    pub const fn atomic(self) -> U256 {
        self.atomic
    }
}

impl std::fmt::Display for UsdcAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scale = U256::from(USDC_SCALE);
        let whole = self.atomic / scale;
        let fraction = self.atomic % scale;

        if fraction == U256::ZERO {
            return write!(f, "{whole}");
        }

        let mut fraction_text = format!("{fraction:06}");
        while fraction_text.ends_with('0') {
            fraction_text.pop();
        }

        write!(f, "{whole}.{fraction_text}")
    }
}

impl From<UsdcAmount> for U256 {
    fn from(amount: UsdcAmount) -> Self {
        amount.atomic
    }
}

/// A validated CCTP v2 route between two supported `NamedChain` values.
///
/// The bridge builder still accepts chains directly for backwards
/// compatibility. Use this primitive in applications that want to validate
/// route configuration before constructing providers or prompting for wallet
/// signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CctpV2Route {
    source: NamedChain,
    destination: NamedChain,
}

impl CctpV2Route {
    /// Validates and creates a CCTP v2 route.
    pub fn new(source: NamedChain, destination: NamedChain) -> Result<Self> {
        if !source.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(source));
        }
        if !destination.supports_cctp_v2() {
            return Err(CctpError::UnsupportedChain(destination));
        }
        if source == destination {
            return Err(CctpError::InvalidConfig(
                "source and destination chains must differ".to_string(),
            ));
        }
        if source.is_testnet() != destination.is_testnet() {
            return Err(CctpError::InvalidConfig(
                "source and destination chains must both be mainnet or both be testnet".to_string(),
            ));
        }

        Ok(Self {
            source,
            destination,
        })
    }

    /// Validates and creates a CCTP v2 route for a specific transfer asset.
    ///
    /// This keeps route validation and asset support validation at the same API
    /// boundary. For example, `USDC` is available on all bridge-supported v2
    /// chains except BNB Smart Chain (which the bridge SDK does not route),
    /// while `EURC` is currently modeled only for Ethereum <-> Base.
    pub fn for_asset(
        source: NamedChain,
        destination: NamedChain,
        asset: CctpTransferAsset,
    ) -> Result<Self> {
        let route = Self::new(source, destination)?;
        asset.validate_v2_route(route)?;
        Ok(route)
    }

    /// Source chain.
    #[must_use]
    pub const fn source_chain(self) -> NamedChain {
        self.source
    }

    /// Destination chain.
    #[must_use]
    pub const fn destination_chain(self) -> NamedChain {
        self.destination
    }

    /// Source CCTP v2 domain ID.
    pub fn source_domain_id(self) -> Result<DomainId> {
        self.source.cctp_v2_domain_id()
    }

    /// Destination CCTP v2 domain ID.
    pub fn destination_domain_id(self) -> Result<DomainId> {
        self.destination.cctp_v2_domain_id()
    }

    /// Validates that the route supports the selected transfer asset.
    pub fn validate_asset(self, asset: CctpTransferAsset) -> Result<()> {
        asset.validate_v2_route(self)
    }

    /// Returns the source-chain EVM token address for the selected asset.
    pub fn source_token_address(self, asset: CctpTransferAsset) -> Result<Address> {
        self.validate_asset(asset)?;
        asset.v2_bridge_token_address(self.source)
    }

    /// Returns the destination-chain EVM token address for the selected asset.
    pub fn destination_token_address(self, asset: CctpTransferAsset) -> Result<Address> {
        self.validate_asset(asset)?;
        asset.v2_bridge_token_address(self.destination)
    }
}

fn usdc_evm_token_address(chain: NamedChain) -> Option<Address> {
    Some(match chain {
        NamedChain::Mainnet => addresses::ETHEREUM_USDC_ADDRESS,
        NamedChain::Sepolia => addresses::ETHEREUM_SEPOLIA_USDC_ADDRESS,
        NamedChain::Arbitrum => addresses::ARBITRUM_USDC_ADDRESS,
        NamedChain::ArbitrumSepolia => addresses::ARBITRUM_SEPOLIA_USDC_ADDRESS,
        NamedChain::Avalanche => addresses::AVALANCHE_USDC_ADDRESS,
        NamedChain::AvalancheFuji => addresses::AVALANCHE_FUJI_USDC_ADDRESS,
        NamedChain::Base => addresses::BASE_USDC_ADDRESS,
        NamedChain::BaseSepolia => addresses::BASE_SEPOLIA_USDC_ADDRESS,
        NamedChain::Optimism => addresses::OPTIMISM_USDC_ADDRESS,
        NamedChain::OptimismSepolia => addresses::OPTIMISM_SEPOLIA_USDC_ADDRESS,
        NamedChain::Polygon => addresses::POLYGON_USDC_ADDRESS,
        NamedChain::PolygonAmoy => addresses::POLYGON_AMOY_USDC_ADDRESS,
        NamedChain::Unichain => addresses::UNICHAIN_USDC_ADDRESS,
        NamedChain::Linea => addresses::LINEA_USDC_ADDRESS,
        NamedChain::Sonic => addresses::SONIC_USDC_ADDRESS,
        NamedChain::Sei => addresses::SEI_USDC_ADDRESS,
        NamedChain::Hyperliquid => addresses::HYPEREVM_USDC_ADDRESS,
        _ => return None,
    })
}

fn eurc_evm_token_address(chain: NamedChain) -> Option<Address> {
    Some(match chain {
        NamedChain::Mainnet => addresses::ETHEREUM_EURC_ADDRESS,
        NamedChain::Sepolia => addresses::ETHEREUM_SEPOLIA_EURC_ADDRESS,
        NamedChain::Base => addresses::BASE_EURC_ADDRESS,
        NamedChain::BaseSepolia => addresses::BASE_SEPOLIA_EURC_ADDRESS,
        _ => return None,
    })
}

fn unsupported_asset_chain(
    asset: CctpTransferAsset,
    chain: NamedChain,
    reason: impl Into<String>,
) -> CctpError {
    CctpError::UnsupportedAssetChain {
        asset,
        chain,
        reason: reason.into(),
    }
}

fn unsupported_asset_route(
    asset: CctpTransferAsset,
    source_chain: NamedChain,
    destination_chain: NamedChain,
    reason: impl Into<String>,
) -> CctpError {
    CctpError::UnsupportedAssetRoute {
        asset,
        source_chain,
        destination_chain,
        reason: reason.into(),
    }
}

fn parse_usdc_fraction(input: &str) -> Result<u128> {
    if input.len() > 6 {
        return Err(invalid_amount(
            "USDC amounts support at most 6 decimal places",
        ));
    }
    if !input.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid_amount(
            "amount fractional part must contain only digits",
        ));
    }

    let mut padded = input.to_owned();
    while padded.len() < 6 {
        padded.push('0');
    }

    if padded.is_empty() {
        return Ok(0);
    }

    padded
        .parse::<u128>()
        .map_err(|_| invalid_amount("amount fractional part is too large"))
}

fn invalid_amount(message: &str) -> CctpError {
    CctpError::InvalidAmount(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usdc_amounts() {
        assert_eq!(
            UsdcAmount::parse_decimal("1")
                .expect("valid amount")
                .atomic(),
            U256::from(1_000_000u64)
        );
        assert_eq!(
            UsdcAmount::parse_decimal("1.25")
                .expect("valid amount")
                .atomic(),
            U256::from(1_250_000u64)
        );
        assert_eq!(
            UsdcAmount::parse_decimal(".5")
                .expect("valid amount")
                .atomic(),
            U256::from(500_000u64)
        );
        assert_eq!(
            UsdcAmount::parse_decimal("0.000001")
                .expect("valid amount")
                .atomic(),
            U256::from(1u64)
        );
    }

    #[test]
    fn rejects_invalid_usdc_amounts() {
        for amount in ["", "0", "-1", "+1", "1.0000001", "1.2.3", "abc", "1.a"] {
            assert!(
                UsdcAmount::parse_decimal(amount).is_err(),
                "{amount} should fail"
            );
        }
    }

    #[test]
    fn displays_usdc_amounts_without_unnecessary_trailing_zeroes() {
        assert_eq!(
            UsdcAmount::from_atomic(U256::from(1_000_000u64)).to_string(),
            "1"
        );
        assert_eq!(
            UsdcAmount::from_atomic(U256::from(1_250_000u64)).to_string(),
            "1.25"
        );
        assert_eq!(
            UsdcAmount::from_atomic(U256::from(1u64)).to_string(),
            "0.000001"
        );
    }

    #[test]
    fn transfer_asset_symbols_are_stable() {
        assert_eq!(CctpTransferAsset::default(), CctpTransferAsset::Usdc);
        assert_eq!(CctpTransferAsset::Usdc.to_string(), "USDC");
        assert_eq!(CctpTransferAsset::Eurc.to_string(), "EURC");
        assert_eq!(CctpTransferAsset::Usyc.to_string(), "USYC");
    }

    #[test]
    fn validates_eurc_cctp_v2_bridge_routes() {
        let mainnet_route = CctpV2Route::for_asset(
            NamedChain::Mainnet,
            NamedChain::Base,
            CctpTransferAsset::Eurc,
        )
        .expect("Ethereum -> Base EURC route is supported");
        assert_eq!(
            mainnet_route
                .source_token_address(CctpTransferAsset::Eurc)
                .expect("source token address"),
            addresses::ETHEREUM_EURC_ADDRESS
        );
        assert_eq!(
            mainnet_route
                .destination_token_address(CctpTransferAsset::Eurc)
                .expect("destination token address"),
            addresses::BASE_EURC_ADDRESS
        );

        let testnet_route = CctpV2Route::for_asset(
            NamedChain::BaseSepolia,
            NamedChain::Sepolia,
            CctpTransferAsset::Eurc,
        )
        .expect("Base Sepolia -> Sepolia EURC route is supported");
        assert_eq!(
            testnet_route
                .source_token_address(CctpTransferAsset::Eurc)
                .expect("source token address"),
            addresses::BASE_SEPOLIA_EURC_ADDRESS
        );
        assert_eq!(
            testnet_route
                .destination_token_address(CctpTransferAsset::Eurc)
                .expect("destination token address"),
            addresses::ETHEREUM_SEPOLIA_EURC_ADDRESS
        );
    }

    #[test]
    fn rejects_unannounced_eurc_bridge_routes() {
        let err = CctpV2Route::for_asset(
            NamedChain::Mainnet,
            NamedChain::Linea,
            CctpTransferAsset::Eurc,
        )
        .expect_err("EURC CCTP is not modeled for Ethereum -> Linea");

        assert!(matches!(
            err,
            CctpError::UnsupportedAssetRoute {
                asset: CctpTransferAsset::Eurc,
                source_chain: NamedChain::Mainnet,
                destination_chain: NamedChain::Linea,
                ..
            }
        ));
    }

    #[test]
    fn rejects_usyc_until_bnb_bridge_routing_exists() {
        let route =
            CctpV2Route::new(NamedChain::Mainnet, NamedChain::Base).expect("valid USDC route");
        let err = route
            .validate_asset(CctpTransferAsset::Usyc)
            .expect_err("USYC depends on a BNB route this bridge does not support");

        assert!(matches!(
            err,
            CctpError::UnsupportedAssetRoute {
                asset: CctpTransferAsset::Usyc,
                source_chain: NamedChain::Mainnet,
                destination_chain: NamedChain::Base,
                ..
            }
        ));
    }

    #[test]
    fn returns_known_usdc_addresses_for_bridge_supported_chains() {
        assert_eq!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(NamedChain::Mainnet)
                .expect("Ethereum USDC address"),
            addresses::ETHEREUM_USDC_ADDRESS
        );
        assert_eq!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(NamedChain::Hyperliquid)
                .expect("HyperEVM USDC address"),
            addresses::HYPEREVM_USDC_ADDRESS
        );
        assert_eq!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(NamedChain::PolygonAmoy)
                .expect("Polygon Amoy USDC address"),
            addresses::POLYGON_AMOY_USDC_ADDRESS
        );
    }

    #[test]
    fn token_address_lookup_rejects_unsupported_asset_chain_pairs() {
        let err = CctpTransferAsset::Eurc
            .v2_bridge_token_address(NamedChain::Avalanche)
            .expect_err("Avalanche EURC exists but is not a modeled CCTP EURC route");

        assert!(matches!(
            err,
            CctpError::UnsupportedAssetChain {
                asset: CctpTransferAsset::Eurc,
                chain: NamedChain::Avalanche,
                ..
            }
        ));
    }

    #[test]
    fn validates_cctp_v2_route() {
        let route =
            CctpV2Route::new(NamedChain::Mainnet, NamedChain::Hyperliquid).expect("valid route");

        assert_eq!(route.source_chain(), NamedChain::Mainnet);
        assert_eq!(route.destination_chain(), NamedChain::Hyperliquid);
        assert_eq!(
            route.source_domain_id().expect("source domain"),
            DomainId::Ethereum
        );
        assert_eq!(
            route.destination_domain_id().expect("destination domain"),
            DomainId::HyperEvm
        );
    }

    #[test]
    fn rejects_invalid_cctp_v2_routes() {
        assert!(CctpV2Route::new(NamedChain::Mainnet, NamedChain::Moonbeam).is_err());
        assert!(CctpV2Route::new(NamedChain::Mainnet, NamedChain::Mainnet).is_err());
        assert!(CctpV2Route::new(NamedChain::Mainnet, NamedChain::BaseSepolia).is_err());
    }
}
