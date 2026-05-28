// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Shared domain primitives for CCTP applications.

use alloy_chains::NamedChain;
use alloy_primitives::U256;

use crate::{CctpError, CctpV2, DomainId, Result};

const USDC_SCALE: u128 = 1_000_000;

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
