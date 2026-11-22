// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Shared domain primitives for CCTP applications.

use alloy_chains::NamedChain;
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::chain::addresses;
use crate::{
    AmountError, CctpError, CctpV2, ConfigError, DomainId, InvalidDestinationCaller, Result,
};

const USDC_SCALE: u128 = 1_000_000;
const FORWARDING_SERVICE_HOOK_NAME: &[u8] = b"cctp-forward";
const FORWARDING_SERVICE_HOOK_VERSION: u32 = 1;

/// Canonical version-1 hook data for Circle's Forwarding Service.
///
/// Values of this type can only be constructed from a validated
/// [`ForwardingServiceRoute`]. The encoded frame contains Circle's
/// `cctp-forward` hook name, version `1`, and an empty payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ForwardingServiceHookData {
    bytes32: FixedBytes<32>,
    bytes: Bytes,
}

impl ForwardingServiceHookData {
    fn canonical() -> Self {
        let mut bytes = [0_u8; 32];
        bytes[..FORWARDING_SERVICE_HOOK_NAME.len()].copy_from_slice(FORWARDING_SERVICE_HOOK_NAME);
        bytes[24..28].copy_from_slice(&FORWARDING_SERVICE_HOOK_VERSION.to_be_bytes());
        Self {
            bytes32: FixedBytes::from(bytes),
            bytes: Bytes::copy_from_slice(&bytes),
        }
    }

    /// Returns the canonical 32-byte version-1 forwarding frame.
    #[must_use]
    pub const fn as_bytes32(&self) -> FixedBytes<32> {
        self.bytes32
    }

    /// Borrows the canonical frame as hook bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Returns the forwarding frame as hook bytes accepted by
    /// [`TransferMode`](crate::TransferMode).
    #[must_use]
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

impl From<ForwardingServiceHookData> for Bytes {
    fn from(hook: ForwardingServiceHookData) -> Self {
        hook.into_bytes()
    }
}

/// A nonzero CCTP `destinationCaller` word that restricts destination-chain
/// message execution to one caller.
///
/// The protocol field is canonically `bytes32`, not an EVM `address`. Keeping
/// the full word avoids locking the type to EVM destinations. It does not by
/// itself enable non-EVM bridge routes or construct chain-specific forwarding
/// payloads. Use [`Self::from_evm_address`] for the currently supported EVM
/// restricted-call flow.
///
/// The all-zero word is deliberately rejected because CCTP assigns it the
/// distinct meaning “permissionless.” Existing bridge helpers continue to use
/// that permissionless default; this type is for the advanced restricted-call
/// contract APIs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RestrictedDestinationCaller(FixedBytes<32>);

impl RestrictedDestinationCaller {
    /// Validates a canonical 32-byte destination caller.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDestinationCaller`] for the all-zero word, which CCTP
    /// interprets as permissionless rather than restricted.
    pub fn new(caller: FixedBytes<32>) -> std::result::Result<Self, InvalidDestinationCaller> {
        if caller == FixedBytes::ZERO {
            return Err(InvalidDestinationCaller);
        }
        Ok(Self(caller))
    }

    /// Converts and validates an EVM address as a left-padded CCTP `bytes32`
    /// destination caller.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidDestinationCaller`] when `caller` is the zero address.
    pub fn from_evm_address(
        caller: Address,
    ) -> std::result::Result<Self, InvalidDestinationCaller> {
        Self::new(caller.into_word())
    }

    /// Returns the canonical CCTP `bytes32` wire value.
    #[must_use]
    pub const fn as_bytes32(self) -> FixedBytes<32> {
        self.0
    }
}

impl TryFrom<FixedBytes<32>> for RestrictedDestinationCaller {
    type Error = InvalidDestinationCaller;

    fn try_from(caller: FixedBytes<32>) -> std::result::Result<Self, Self::Error> {
        Self::new(caller)
    }
}

impl TryFrom<Address> for RestrictedDestinationCaller {
    type Error = InvalidDestinationCaller;

    fn try_from(caller: Address) -> std::result::Result<Self, Self::Error> {
        Self::from_evm_address(caller)
    }
}

impl From<RestrictedDestinationCaller> for FixedBytes<32> {
    fn from(caller: RestrictedDestinationCaller) -> Self {
        caller.as_bytes32()
    }
}

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
    /// Values with more than six decimal places, or whose atomic-unit form
    /// exceeds `uint256`, are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::InvalidAmount`] with a typed [`AmountError`] when
    /// the input is malformed, too precise, too large, or resolves to zero.
    pub fn parse_decimal(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(AmountError::InvalidDecimal.into());
        }
        if trimmed.starts_with('-') || trimmed.starts_with('+') {
            return Err(AmountError::InvalidDecimal.into());
        }

        let mut parts = trimmed.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next();
        if parts.next().is_some() {
            return Err(AmountError::InvalidDecimal.into());
        }

        if whole.is_empty() && fraction.is_none_or(str::is_empty) {
            return Err(AmountError::InvalidDecimal.into());
        }
        if !whole.chars().all(|c| c.is_ascii_digit()) {
            return Err(AmountError::InvalidDecimal.into());
        }

        let whole_units = if whole.is_empty() {
            U256::ZERO
        } else {
            U256::from_str_radix(whole, 10).map_err(|_| AmountError::TooLarge)?
        };

        let fractional_units = match fraction {
            Some(value) => parse_usdc_fraction(value)?,
            None => 0,
        };

        let atomic_units = whole_units
            .checked_mul(U256::from(USDC_SCALE))
            .and_then(|value| value.checked_add(U256::from(fractional_units)))
            .ok_or(AmountError::TooLarge)?;

        if atomic_units == U256::ZERO {
            return Err(AmountError::Zero.into());
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

/// A validated USDC CCTP v2 route whose destination accepts Circle's
/// Forwarding Service.
///
/// This type covers Circle's generic Forwarding Service for bridge-supported
/// EVM destinations. Stellar's separate `CctpForwarder` flow is intentionally
/// outside this type because it requires a non-EVM provider, seven-decimal
/// USDC amounts, full-width Stellar addresses, and Soroban submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForwardingServiceRoute(CctpV2Route);

impl ForwardingServiceRoute {
    /// Validates a USDC route and its destination Forwarding Service
    /// capability.
    pub fn new(source: NamedChain, destination: NamedChain) -> Result<Self> {
        let route = CctpV2Route::for_asset(source, destination, CctpTransferAsset::Usdc)?;
        if !destination.supports_forwarding_service()? {
            return Err(CctpError::ForwardingServiceUnsupported {
                destination_chain: destination,
            });
        }
        Ok(Self(route))
    }

    /// Underlying validated CCTP v2 route.
    #[must_use]
    pub const fn route(self) -> CctpV2Route {
        self.0
    }

    /// Source chain.
    #[must_use]
    pub const fn source_chain(self) -> NamedChain {
        self.0.source_chain()
    }

    /// Destination chain.
    #[must_use]
    pub const fn destination_chain(self) -> NamedChain {
        self.0.destination_chain()
    }

    /// Canonical version-1 hook data for the Forwarding Service request.
    #[must_use]
    pub fn hook_data(self) -> ForwardingServiceHookData {
        ForwardingServiceHookData::canonical()
    }
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
            return Err(ConfigError::RouteSameChain { chain: source }.into());
        }
        if source.is_testnet() != destination.is_testnet() {
            return Err(ConfigError::RouteEnvironmentMismatch {
                source_chain: source,
                destination_chain: destination,
            }
            .into());
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
        NamedChain::UnichainSepolia => addresses::UNICHAIN_SEPOLIA_USDC_ADDRESS,
        NamedChain::Linea => addresses::LINEA_USDC_ADDRESS,
        NamedChain::LineaSepolia => addresses::LINEA_SEPOLIA_USDC_ADDRESS,
        NamedChain::Sonic => addresses::SONIC_USDC_ADDRESS,
        NamedChain::SonicTestnet => addresses::SONIC_TESTNET_USDC_ADDRESS,
        NamedChain::Sei => addresses::SEI_USDC_ADDRESS,
        NamedChain::SeiTestnet => addresses::SEI_TESTNET_USDC_ADDRESS,
        NamedChain::Hyperliquid => addresses::HYPEREVM_USDC_ADDRESS,
        NamedChain::HyperliquidTestnet => addresses::HYPEREVM_TESTNET_USDC_ADDRESS,
        NamedChain::World => addresses::WORLD_CHAIN_USDC_ADDRESS,
        NamedChain::WorldSepolia => addresses::WORLD_CHAIN_SEPOLIA_USDC_ADDRESS,
        NamedChain::Monad => addresses::MONAD_USDC_ADDRESS,
        NamedChain::MonadTestnet => addresses::MONAD_TESTNET_USDC_ADDRESS,
        NamedChain::XdcMainnet => addresses::XDC_USDC_ADDRESS,
        NamedChain::XdcTestnet => addresses::XDC_TESTNET_USDC_ADDRESS,
        NamedChain::Ink => addresses::INK_USDC_ADDRESS,
        NamedChain::InkSepolia => addresses::INK_SEPOLIA_USDC_ADDRESS,
        NamedChain::Plume => addresses::PLUME_USDC_ADDRESS,
        NamedChain::PlumeTestnet => addresses::PLUME_TESTNET_USDC_ADDRESS,
        NamedChain::Arc => addresses::ARC_USDC_ADDRESS,
        NamedChain::ArcTestnet => addresses::ARC_TESTNET_USDC_ADDRESS,
        NamedChain::Injective => addresses::INJECTIVE_USDC_ADDRESS,
        NamedChain::InjectiveTestnet => addresses::INJECTIVE_TESTNET_USDC_ADDRESS,
        NamedChain::Pharos => addresses::PHAROS_USDC_ADDRESS,
        NamedChain::PharosTestnet => addresses::PHAROS_TESTNET_USDC_ADDRESS,
        NamedChain::Cronos => addresses::CRONOS_USDC_ADDRESS,
        NamedChain::CronosTestnet => addresses::CRONOS_TESTNET_USDC_ADDRESS,
        NamedChain::Plasma => addresses::PLASMA_USDC_ADDRESS,
        NamedChain::PlasmaTestnet => addresses::PLASMA_TESTNET_USDC_ADDRESS,
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
    if !input.chars().all(|c| c.is_ascii_digit()) {
        return Err(AmountError::InvalidDecimal.into());
    }
    if input.len() > 6 {
        return Err(AmountError::TooManyDecimalPlaces {
            actual: input.len(),
        }
        .into());
    }

    let value = input
        .bytes()
        .fold(0_u128, |value, digit| value * 10 + u128::from(digit - b'0'));
    Ok(value * 10_u128.pow((6 - input.len()) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarding_service_route_constructs_canonical_version_one_hook() {
        let route = ForwardingServiceRoute::new(NamedChain::Base, NamedChain::Avalanche)
            .expect("Avalanche accepts Circle Forwarding Service requests");

        assert_eq!(route.source_chain(), NamedChain::Base);
        assert_eq!(route.destination_chain(), NamedChain::Avalanche);
        assert_eq!(
            alloy_primitives::hex::encode(route.hook_data().as_bytes32()),
            "636374702d666f72776172640000000000000000000000000000000100000000"
        );
    }

    #[test]
    fn forwarding_service_route_rejects_unsupported_destination() {
        assert!(matches!(
            ForwardingServiceRoute::new(NamedChain::Mainnet, NamedChain::Injective),
            Err(CctpError::ForwardingServiceUnsupported {
                destination_chain: NamedChain::Injective
            })
        ));
    }

    #[test]
    fn restricted_destination_caller_preserves_canonical_bytes32() {
        let word = FixedBytes::repeat_byte(0x42);
        let caller = RestrictedDestinationCaller::new(word)
            .expect("nonzero bytes32 is a restricted destination caller");

        assert_eq!(caller.as_bytes32(), word);
        assert_eq!(FixedBytes::<32>::from(caller), word);
    }

    #[test]
    fn restricted_evm_destination_caller_is_left_padded() {
        let address = Address::repeat_byte(0x42);
        let caller = RestrictedDestinationCaller::from_evm_address(address)
            .expect("nonzero EVM address is a restricted destination caller");

        assert_eq!(caller.as_bytes32(), address.into_word());
    }

    #[test]
    fn zero_destination_caller_cannot_be_misrepresented_as_restricted() {
        assert_eq!(
            RestrictedDestinationCaller::new(FixedBytes::ZERO),
            Err(InvalidDestinationCaller)
        );
        assert_eq!(
            RestrictedDestinationCaller::from_evm_address(Address::ZERO),
            Err(InvalidDestinationCaller)
        );
    }

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
            UsdcAmount::parse_decimal("1.")
                .expect("valid amount")
                .atomic(),
            U256::from(1_000_000u64)
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
    fn rejects_malformed_usdc_decimals_with_typed_error() {
        for amount in ["", "-1", "+1", ".", "1.2.3", "abc", "1.a", "1.abcdefg"] {
            assert!(
                matches!(
                    UsdcAmount::parse_decimal(amount),
                    Err(CctpError::InvalidAmount(AmountError::InvalidDecimal))
                ),
                "{amount} should be rejected as a malformed decimal"
            );
        }
    }

    #[test]
    fn rejects_excess_usdc_precision_with_actual_count() {
        assert!(matches!(
            UsdcAmount::parse_decimal("1.0000001"),
            Err(CctpError::InvalidAmount(
                AmountError::TooManyDecimalPlaces { actual: 7 }
            ))
        ));
    }

    #[test]
    fn rejects_usdc_amounts_outside_atomic_range() {
        let first_whole_amount_above_range = (U256::MAX / U256::from(USDC_SCALE))
            .checked_add(U256::from(1))
            .expect("scaled maximum leaves room for one whole-unit increment")
            .to_string();

        assert!(matches!(
            UsdcAmount::parse_decimal(&first_whole_amount_above_range),
            Err(CctpError::InvalidAmount(AmountError::TooLarge))
        ));
    }

    #[test]
    fn parses_full_uint256_atomic_range() {
        let formatted = UsdcAmount::from_atomic(U256::MAX).to_string();
        let reparsed = UsdcAmount::parse_decimal(&formatted).expect("U256::MAX must round-trip");

        assert_eq!(reparsed.atomic(), U256::MAX);
    }

    #[test]
    fn rejects_zero_usdc_amounts_with_typed_error() {
        for amount in ["0", "0.0", ".0", " 0.000000 "] {
            assert!(matches!(
                UsdcAmount::parse_decimal(amount),
                Err(CctpError::InvalidAmount(AmountError::Zero))
            ));
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
        assert_eq!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(NamedChain::World)
                .expect("World Chain USDC address"),
            addresses::WORLD_CHAIN_USDC_ADDRESS
        );
        assert_eq!(
            CctpTransferAsset::Usdc
                .v2_bridge_token_address(NamedChain::XdcTestnet)
                .expect("XDC testnet USDC address"),
            addresses::XDC_TESTNET_USDC_ADDRESS
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

        let expanded_route = CctpV2Route::for_asset(
            NamedChain::World,
            NamedChain::Plasma,
            CctpTransferAsset::Usdc,
        )
        .expect("World Chain -> Plasma USDC route");
        assert_eq!(
            expanded_route.source_domain_id().expect("source domain"),
            DomainId::WorldChain
        );
        assert_eq!(
            expanded_route
                .destination_domain_id()
                .expect("destination domain"),
            DomainId::Plasma
        );
    }

    #[test]
    fn rejects_invalid_cctp_v2_routes() {
        assert!(CctpV2Route::new(NamedChain::Mainnet, NamedChain::Moonbeam).is_err());
        assert!(matches!(
            CctpV2Route::new(NamedChain::Mainnet, NamedChain::Mainnet),
            Err(CctpError::InvalidConfig(ConfigError::RouteSameChain {
                chain: NamedChain::Mainnet
            }))
        ));
        assert!(matches!(
            CctpV2Route::new(NamedChain::Mainnet, NamedChain::BaseSepolia),
            Err(CctpError::InvalidConfig(
                ConfigError::RouteEnvironmentMismatch {
                    source_chain: NamedChain::Mainnet,
                    destination_chain: NamedChain::BaseSepolia,
                }
            ))
        ));
    }
}
