// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 transfer fee types.

use alloy_primitives::U256;
use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

use super::FinalityThreshold;

const BPS_HUNDREDTH_DENOMINATOR: u64 = 1_000_000;
const BUFFER_PERCENT_DENOMINATOR: u64 = 100;

/// A CCTP transfer fee in basis points, stored as hundredths of a basis point.
///
/// Circle's fee API returns `minimumFee` in basis points. Some published routes
/// use fractional values such as `1.3` bps, so this type avoids forcing callers
/// through integer-only basis points when calculating `maxFee`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeeBps {
    hundredths: u32,
}

impl FeeBps {
    /// Creates a fee from hundredths of a basis point.
    ///
    /// `FeeBps::from_hundredths(130)` represents `1.3` bps.
    #[must_use]
    pub const fn from_hundredths(hundredths: u32) -> Self {
        Self { hundredths }
    }

    /// Returns the fee in hundredths of a basis point.
    #[must_use]
    pub const fn as_hundredths(self) -> u32 {
        self.hundredths
    }

    /// Returns the whole-basis-point component of the fee.
    ///
    /// Fractional basis points are truncated. Use [`Self::as_hundredths`] for
    /// exact calculations.
    #[must_use]
    pub const fn whole_bps(self) -> u32 {
        self.hundredths / 100
    }

    /// Calculates the fee amount in burn-token atomic units.
    ///
    /// The result is rounded up to avoid returning a cap below the route's
    /// minimum fee when the percentage produces a fractional atomic unit.
    #[must_use]
    pub fn apply_to_amount(self, amount: U256) -> U256 {
        ceil_div(
            amount * U256::from(self.hundredths),
            U256::from(BPS_HUNDREDTH_DENOMINATOR),
        )
    }

    /// Calculates the fee amount with a percentage buffer.
    ///
    /// `buffer_percent = 20` returns `fee * 1.2`, rounded up to an atomic unit.
    #[must_use]
    pub fn apply_to_amount_with_buffer_percent(self, amount: U256, buffer_percent: u32) -> U256 {
        let base_fee = self.apply_to_amount(amount);
        let multiplier = U256::from(u64::from(buffer_percent) + BUFFER_PERCENT_DENOMINATOR);
        ceil_div(
            base_fee * multiplier,
            U256::from(BUFFER_PERCENT_DENOMINATOR),
        )
    }
}

impl fmt::Display for FeeBps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.hundredths / 100;
        let fractional = self.hundredths % 100;

        if fractional == 0 {
            write!(f, "{whole}")
        } else if fractional.is_multiple_of(10) {
            write!(f, "{}.{}", whole, fractional / 10)
        } else {
            write!(f, "{whole}.{fractional:02}")
        }
    }
}

impl Serialize for FeeBps {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if self.hundredths.is_multiple_of(100) {
            serializer.serialize_u32(self.hundredths / 100)
        } else {
            serializer.serialize_f64(f64::from(self.hundredths) / 100.0)
        }
    }
}

impl<'de> Deserialize<'de> for FeeBps {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FeeBpsVisitor;

        impl Visitor<'_> for FeeBpsVisitor {
            type Value = FeeBps;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a non-negative basis point number")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let bps = u32::try_from(value).map_err(|_| {
                    de::Error::invalid_value(Unexpected::Unsigned(value), &"u32-sized fee")
                })?;
                bps.checked_mul(100)
                    .map(FeeBps::from_hundredths)
                    .ok_or_else(|| de::Error::custom("fee basis points overflowed u32"))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let unsigned = u64::try_from(value).map_err(|_| {
                    de::Error::invalid_value(
                        Unexpected::Signed(value),
                        &"a non-negative basis point number",
                    )
                })?;
                self.visit_u64(unsigned)
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if !value.is_finite() || value.is_sign_negative() {
                    return Err(de::Error::invalid_value(
                        Unexpected::Float(value),
                        &"a non-negative finite basis point number",
                    ));
                }

                parse_fee_hundredths(&format!("{value}")).map_err(de::Error::custom)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                parse_fee_hundredths(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(FeeBpsVisitor)
    }
}

/// A single fee entry from Circle's CCTP v2 transfer-fee API.
///
/// The API returns an array of entries shaped like:
///
/// ```json
/// { "finalityThreshold": 1000, "minimumFee": 1.3 }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferFee {
    /// Finality threshold this fee applies to.
    pub finality_threshold: u32,
    /// Minimum fee in basis points.
    pub minimum_fee: FeeBps,
}

impl TransferFee {
    /// Creates a transfer fee entry.
    #[must_use]
    pub const fn new(finality_threshold: u32, minimum_fee: FeeBps) -> Self {
        Self {
            finality_threshold,
            minimum_fee,
        }
    }

    /// Returns this entry's typed finality threshold, if it is one of the
    /// thresholds currently recognized by CCTP v2.
    #[must_use]
    pub const fn finality(self) -> Option<FinalityThreshold> {
        FinalityThreshold::from_u32(self.finality_threshold)
    }

    /// Returns true when this entry applies to fast transfers.
    #[must_use]
    pub const fn is_fast_transfer(self) -> bool {
        matches!(self.finality(), Some(FinalityThreshold::Fast))
    }

    /// Returns true when this entry applies to standard transfers.
    #[must_use]
    pub const fn is_standard_transfer(self) -> bool {
        matches!(self.finality(), Some(FinalityThreshold::Standard))
    }

    /// Calculates a `maxFee` cap in USDC atomic units with the given buffer.
    #[must_use]
    pub fn max_fee_with_buffer_percent(self, amount: U256, buffer_percent: u32) -> U256 {
        self.minimum_fee
            .apply_to_amount_with_buffer_percent(amount, buffer_percent)
    }
}

fn ceil_div(numerator: U256, denominator: U256) -> U256 {
    if numerator == U256::ZERO {
        U256::ZERO
    } else {
        ((numerator - U256::from(1)) / denominator) + U256::from(1)
    }
}

fn parse_fee_hundredths(input: &str) -> Result<FeeBps, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("fee cannot be empty".to_string());
    }
    if input.starts_with('-') {
        return Err("fee cannot be negative".to_string());
    }

    let (whole, fractional) = input.split_once('.').unwrap_or((input, ""));
    if whole.is_empty() && fractional.is_empty() {
        return Err("fee must contain digits".to_string());
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) {
        return Err("fee whole component must be numeric".to_string());
    }
    if !fractional.chars().all(|c| c.is_ascii_digit()) {
        return Err("fee fractional component must be numeric".to_string());
    }

    let whole_bps = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<u32>()
            .map_err(|_| "fee whole component overflowed u32".to_string())?
    };
    let whole_hundredths = whole_bps
        .checked_mul(100)
        .ok_or_else(|| "fee basis points overflowed u32".to_string())?;

    let mut chars = fractional.chars();
    let tenths = chars.next().and_then(|c| c.to_digit(10)).unwrap_or(0);
    let hundredths = chars.next().and_then(|c| c.to_digit(10)).unwrap_or(0);
    let needs_round_up = chars.any(|c| c != '0');

    let fractional_hundredths = (tenths * 10) + hundredths + u32::from(needs_round_up);
    let total = whole_hundredths
        .checked_add(fractional_hundredths)
        .ok_or_else(|| "fee basis points overflowed u32".to_string())?;

    Ok(FeeBps::from_hundredths(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_fee_deserializes_circle_response_shape() {
        let json = r#"[
            { "finalityThreshold": 1000, "minimumFee": 1 },
            { "finalityThreshold": 2000, "minimumFee": 0 }
        ]"#;

        let fees: Vec<TransferFee> = serde_json::from_str(json).unwrap();

        assert_eq!(
            fees,
            vec![
                TransferFee::new(1000, FeeBps::from_hundredths(100)),
                TransferFee::new(2000, FeeBps::from_hundredths(0))
            ]
        );
    }

    #[test]
    fn fee_bps_preserves_fractional_basis_points() {
        let fee: FeeBps = serde_json::from_str("1.3").unwrap();

        assert_eq!(fee.as_hundredths(), 130);
        assert_eq!(fee.to_string(), "1.3");
    }

    #[test]
    fn fee_bps_rounds_tiny_extra_precision_up() {
        let fee: FeeBps = serde_json::from_str("1.301").unwrap();

        assert_eq!(fee.as_hundredths(), 131);
    }

    #[test]
    fn fee_bps_rejects_negative_values() {
        let result = serde_json::from_str::<FeeBps>("-1");

        assert!(result.is_err());
    }

    #[test]
    fn fee_calculation_uses_usdc_atomic_units() {
        let amount = U256::from(10_500_000u64);
        let fee = FeeBps::from_hundredths(100);

        assert_eq!(fee.apply_to_amount(amount), U256::from(1050u64));
        assert_eq!(
            fee.apply_to_amount_with_buffer_percent(amount, 20),
            U256::from(1260u64)
        );
    }

    #[test]
    fn fee_calculation_rounds_up_to_avoid_underquoting() {
        let amount = U256::from(1u64);
        let fee = FeeBps::from_hundredths(100);

        assert_eq!(fee.apply_to_amount(amount), U256::from(1u64));
    }

    #[test]
    fn transfer_fee_identifies_known_finality_thresholds() {
        let fast = TransferFee::new(1000, FeeBps::from_hundredths(100));
        let standard = TransferFee::new(2000, FeeBps::from_hundredths(0));
        let unknown = TransferFee::new(1500, FeeBps::from_hundredths(100));

        assert!(fast.is_fast_transfer());
        assert!(standard.is_standard_transfer());
        assert_eq!(unknown.finality(), None);
    }
}
