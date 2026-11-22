// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 transfer fee types.

use alloy_primitives::U256;
use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt, str::FromStr};
use thiserror::Error;

use crate::error::AmountError;

use super::FinalityThreshold;

const BPS_HUNDREDTH_DENOMINATOR: u64 = 1_000_000;
const BUFFER_PERCENT_DENOMINATOR: u64 = 100;
const USDC_ATOMIC_SCALE: u64 = 1_000_000;

/// A validated RFC 3339 timestamp returned by Circle Iris.
///
/// Iris timestamps are parsed at the JSON boundary, so an
/// `IrisTimestamp` always identifies a concrete instant. Its JSON and display
/// representations use UTC RFC 3339 syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IrisTimestamp(jiff::Timestamp);

impl IrisTimestamp {
    /// Parses an Iris timestamp from RFC 3339 text.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIrisTimestamp`] when `value` does not identify an
    /// RFC 3339 instant.
    pub fn parse(value: &str) -> Result<Self, InvalidIrisTimestamp> {
        value.parse()
    }
}

impl FromStr for IrisTimestamp {
    type Err = InvalidIrisTimestamp;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value
            .parse::<jiff::Timestamp>()
            .map(Self)
            .map_err(|_| InvalidIrisTimestamp)
    }
}

impl fmt::Display for IrisTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for IrisTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for IrisTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

/// An Iris timestamp did not identify an RFC 3339 instant.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("invalid Iris timestamp: expected an RFC 3339 instant")]
pub struct InvalidIrisTimestamp;

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
    ///
    /// # Errors
    ///
    /// Returns [`AmountError::FeeOverflow`] when the exact rounded fee exceeds
    /// the on-chain `uint256` range.
    pub fn apply_to_amount(self, amount: U256) -> Result<U256, AmountError> {
        checked_mul_div_ceil(
            amount,
            U256::from(self.hundredths),
            U256::from(BPS_HUNDREDTH_DENOMINATOR),
        )
        .ok_or(AmountError::FeeOverflow {
            amount,
            fee_hundredths: self.hundredths,
        })
    }

    /// Calculates the fee amount with a percentage buffer.
    ///
    /// `buffer_percent = 20` returns `fee * 1.2`, rounded up to an atomic unit.
    ///
    /// # Errors
    ///
    /// Returns [`AmountError::FeeOverflow`] when the unbuffered fee exceeds
    /// `uint256`, or [`AmountError::FeeBufferOverflow`] when only the buffered
    /// fee exceeds it.
    pub fn apply_to_amount_with_buffer_percent(
        self,
        amount: U256,
        buffer_percent: u32,
    ) -> Result<U256, AmountError> {
        let base_fee = self.apply_to_amount(amount)?;
        let multiplier = U256::from(u64::from(buffer_percent) + BUFFER_PERCENT_DENOMINATOR);
        checked_mul_div_ceil(base_fee, multiplier, U256::from(BUFFER_PERCENT_DENOMINATOR)).ok_or(
            AmountError::FeeBufferOverflow {
                fee: base_fee,
                buffer_percent,
            },
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

    /// Calculates a `maxFee` cap in burn-token atomic units with the given buffer.
    ///
    /// # Errors
    ///
    /// Returns a typed [`AmountError`] when the fee cannot fit in `uint256`.
    pub fn max_fee_with_buffer_percent(
        self,
        amount: U256,
        buffer_percent: u32,
    ) -> Result<U256, AmountError> {
        self.minimum_fee
            .apply_to_amount_with_buffer_percent(amount, buffer_percent)
    }
}

/// Forwarding Service fee tier selected from Circle's route quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ForwardingFeeLevel {
    /// Lowest quoted forwarding fee; may provide less execution headroom.
    Low,
    /// Circle's recommended default forwarding fee tier.
    Medium,
    /// Highest quoted forwarding fee; provides the most execution headroom.
    High,
}

/// Atomic-USDC fee tiers returned by Circle's Forwarding Service quote.
///
/// These values are fixed atomic amounts, not basis points. They must be added
/// to the percentage protocol fee when constructing `maxFee`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardingFee {
    low: U256,
    medium: U256,
    high: U256,
}

impl ForwardingFee {
    /// Creates forwarding fee tiers from atomic USDC amounts.
    #[must_use]
    pub const fn new(low: U256, medium: U256, high: U256) -> Self {
        Self { low, medium, high }
    }

    /// Lowest quoted forwarding fee in atomic USDC units.
    #[must_use]
    pub const fn low(self) -> U256 {
        self.low
    }

    /// Medium quoted forwarding fee in atomic USDC units.
    #[must_use]
    pub const fn medium(self) -> U256 {
        self.medium
    }

    /// Highest quoted forwarding fee in atomic USDC units.
    #[must_use]
    pub const fn high(self) -> U256 {
        self.high
    }

    /// Returns the atomic fee for the selected tier.
    #[must_use]
    pub const fn at(self, level: ForwardingFeeLevel) -> U256 {
        match level {
            ForwardingFeeLevel::Low => self.low,
            ForwardingFeeLevel::Medium => self.medium,
            ForwardingFeeLevel::High => self.high,
        }
    }
}

impl<'de> Deserialize<'de> for ForwardingFee {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireForwardingFee {
            low: Box<RawValue>,
            med: Box<RawValue>,
            high: Box<RawValue>,
        }

        let wire = WireForwardingFee::deserialize(deserializer)?;
        Ok(Self {
            low: parse_atomic_units_raw(wire.low.get()).map_err(de::Error::custom)?,
            medium: parse_atomic_units_raw(wire.med.get()).map_err(de::Error::custom)?,
            high: parse_atomic_units_raw(wire.high.get()).map_err(de::Error::custom)?,
        })
    }
}

/// Checked fee cap and burn amount for a Forwarding Service transfer.
///
/// Constructed by [`ForwardingTransferFee::minimum_burn_plan`] so the burn
/// amount is guaranteed to equal the desired recipient amount plus `max_fee`
/// without overflowing the on-chain `uint256` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardingBurnPlan {
    max_fee: U256,
    burn_amount: U256,
}

impl ForwardingBurnPlan {
    /// Combined protocol and Forwarding Service fee cap.
    #[must_use]
    pub const fn max_fee(self) -> U256 {
        self.max_fee
    }

    /// Total amount to burn so the recipient receives the intended amount.
    #[must_use]
    pub const fn burn_amount(self) -> U256 {
        self.burn_amount
    }
}

/// A single entry from Circle's Forwarding Service fee query.
///
/// The protocol fee is percentage-based while the forwarding fee is a fixed
/// atomic-USDC amount. Use [`Self::minimum_max_fee`] when only the cap is
/// needed, or [`Self::minimum_burn_plan`] to construct the related cap and burn
/// amount without floating-point money math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwardingTransferFee {
    finality_threshold: u32,
    minimum_fee: FeeBps,
    forward_fee: ForwardingFee,
}

impl ForwardingTransferFee {
    /// Creates a forwarding quote entry.
    #[must_use]
    pub const fn new(
        finality_threshold: u32,
        minimum_fee: FeeBps,
        forward_fee: ForwardingFee,
    ) -> Self {
        Self {
            finality_threshold,
            minimum_fee,
            forward_fee,
        }
    }

    /// Finality threshold this quote applies to.
    #[must_use]
    pub const fn finality_threshold(self) -> u32 {
        self.finality_threshold
    }

    /// Typed finality threshold, when currently recognized by CCTP v2.
    #[must_use]
    pub const fn finality(self) -> Option<FinalityThreshold> {
        FinalityThreshold::from_u32(self.finality_threshold)
    }

    /// Minimum protocol fee rate.
    #[must_use]
    pub const fn minimum_fee(self) -> FeeBps {
        self.minimum_fee
    }

    /// Fixed Forwarding Service fee tiers.
    #[must_use]
    pub const fn forward_fee(self) -> ForwardingFee {
        self.forward_fee
    }

    /// Minimum `maxFee` covering both the percentage protocol fee on the
    /// desired recipient amount and the selected fixed forwarding tier.
    /// Use [`Self::minimum_burn_plan`] for the checked total that leaves the
    /// intended amount with the destination recipient after fees.
    ///
    /// # Errors
    ///
    /// Returns [`AmountError::ForwardingProtocolFeeOverflow`] when the exact
    /// percentage fee exceeds the on-chain `uint256` range, or
    /// [`AmountError::ForwardingFeeOverflow`] when the protocol and forwarding
    /// fee sum exceeds that range.
    pub fn minimum_max_fee(
        self,
        recipient_amount: U256,
        level: ForwardingFeeLevel,
    ) -> Result<U256, AmountError> {
        let protocol_fee = self
            .minimum_fee
            .apply_to_amount(recipient_amount)
            .map_err(|_| AmountError::ForwardingProtocolFeeOverflow {
                recipient_amount,
                fee_hundredths: self.minimum_fee.as_hundredths(),
            })?;
        let forwarding_fee = self.forward_fee.at(level);
        protocol_fee
            .checked_add(forwarding_fee)
            .ok_or(AmountError::ForwardingFeeOverflow {
                protocol_fee,
                forwarding_fee,
            })
    }

    /// Checked fee cap and burn amount that leave `recipient_amount` after the
    /// selected protocol and Forwarding Service fees.
    ///
    /// # Errors
    ///
    /// Returns the same typed overflow errors as [`Self::minimum_max_fee`], or
    /// [`AmountError::ForwardingBurnAmountOverflow`] when the fee fits but its
    /// sum with the recipient amount does not.
    pub fn minimum_burn_plan(
        self,
        recipient_amount: U256,
        level: ForwardingFeeLevel,
    ) -> Result<ForwardingBurnPlan, AmountError> {
        let max_fee = self.minimum_max_fee(recipient_amount, level)?;
        let burn_amount = recipient_amount.checked_add(max_fee).ok_or(
            AmountError::ForwardingBurnAmountOverflow {
                recipient_amount,
                max_fee,
            },
        )?;
        Ok(ForwardingBurnPlan {
            max_fee,
            burn_amount,
        })
    }
}

/// Fast Transfer allowance returned by Circle Iris.
///
/// Circle's endpoint exposes a global USDC allowance, not a per-route value.
/// The SDK stores it as atomic USDC units so callers can compare it directly
/// against burn amounts without floating-point money math.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FastTransferAllowance {
    /// Remaining global Fast Transfer allowance in atomic USDC units.
    #[serde(serialize_with = "serialize_usdc_atomic_decimal")]
    pub allowance: U256,
    /// Server timestamp from Iris.
    pub last_updated: IrisTimestamp,
}

impl FastTransferAllowance {
    /// Creates an allowance response from atomic USDC units and an Iris timestamp.
    #[must_use]
    pub const fn new(allowance: U256, last_updated: IrisTimestamp) -> Self {
        Self {
            allowance,
            last_updated,
        }
    }

    /// Returns true when this allowance can cover `amount` atomic USDC units.
    #[must_use]
    pub fn is_sufficient_for(&self, amount: U256) -> bool {
        self.allowance >= amount
    }
}

impl<'de> Deserialize<'de> for FastTransferAllowance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireAllowance {
            allowance: Box<RawValue>,
            last_updated: IrisTimestamp,
        }

        let wire = WireAllowance::deserialize(deserializer)?;
        let allowance = parse_usdc_atomic_raw(wire.allowance.get()).map_err(de::Error::custom)?;

        Ok(Self {
            allowance,
            last_updated: wire.last_updated,
        })
    }
}

fn checked_mul_div_ceil(value: U256, multiplier: U256, denominator: U256) -> Option<U256> {
    // Split before multiplying so intermediate `uint256` overflow cannot
    // corrupt a mathematically representable quotient.
    let whole = (value / denominator).checked_mul(multiplier)?;
    let remainder = (value % denominator).checked_mul(multiplier)?;
    whole.checked_add(remainder.div_ceil(denominator))
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

fn parse_usdc_atomic_raw(input: &str) -> Result<U256, String> {
    let decimal = parse_raw_decimal_text(input, "allowance")?;
    parse_usdc_atomic_decimal(&decimal)
}

fn parse_atomic_units_raw(input: &str) -> Result<U256, String> {
    let decimal = parse_raw_decimal_text(input, "atomic-unit")?;

    if decimal.is_empty() {
        return Err("atomic-unit amount cannot be empty".to_string());
    }
    if !decimal.chars().all(|character| character.is_ascii_digit()) {
        return Err("atomic-unit amount must be an unsigned integer".to_string());
    }

    U256::from_str_radix(&decimal, 10).map_err(|_| "atomic-unit amount overflowed U256".to_string())
}

fn parse_raw_decimal_text<'a>(input: &'a str, subject: &str) -> Result<Cow<'a, str>, String> {
    let trimmed = input.trim();
    if trimmed.starts_with('"') {
        serde_json::from_str::<String>(trimmed)
            .map(Cow::Owned)
            .map_err(|error| format!("{subject} string is not valid JSON: {error}"))
    } else {
        Ok(Cow::Borrowed(trimmed))
    }
}

fn parse_usdc_atomic_decimal(input: &str) -> Result<U256, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("allowance cannot be empty".to_string());
    }
    if input.starts_with('-') || input.starts_with('+') {
        return Err("allowance must be unsigned".to_string());
    }

    let (whole, fractional) = input.split_once('.').unwrap_or((input, ""));
    if whole.is_empty() && fractional.is_empty() {
        return Err("allowance must include digits".to_string());
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) {
        return Err("allowance whole component must be numeric".to_string());
    }
    if !fractional.chars().all(|c| c.is_ascii_digit()) {
        return Err("allowance fractional component must be numeric".to_string());
    }
    if fractional.len() > 6 {
        return Err("allowance cannot have more than six decimal places".to_string());
    }

    let whole_units = if whole.is_empty() {
        U256::ZERO
    } else {
        U256::from_str_radix(whole, 10)
            .map_err(|_| "allowance whole component overflowed U256".to_string())?
    };

    let mut padded_fraction = fractional.to_string();
    while padded_fraction.len() < 6 {
        padded_fraction.push('0');
    }
    let fractional_units = if padded_fraction.is_empty() {
        0
    } else {
        padded_fraction
            .parse::<u64>()
            .map_err(|_| "allowance fractional component overflowed u64".to_string())?
    };

    whole_units
        .checked_mul(U256::from(USDC_ATOMIC_SCALE))
        .and_then(|value| value.checked_add(U256::from(fractional_units)))
        .ok_or_else(|| "allowance overflowed U256".to_string())
}

fn serialize_usdc_atomic_decimal<S>(allowance: &U256, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_usdc_atomic(*allowance))
}

fn format_usdc_atomic(allowance: U256) -> String {
    let scale = U256::from(USDC_ATOMIC_SCALE);
    let whole = allowance / scale;
    let fraction = allowance % scale;

    if fraction == U256::ZERO {
        return whole.to_string();
    }

    let mut fraction_text = format!("{fraction:06}");
    while fraction_text.ends_with('0') {
        fraction_text.pop();
    }

    format!("{whole}.{fraction_text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iris_timestamp(value: &str) -> IrisTimestamp {
        IrisTimestamp::parse(value).unwrap()
    }

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
    fn transfer_fee_deserializes_with_optional_forward_fee_fields() {
        let json = r#"[
            {
                "finalityThreshold": 1000,
                "minimumFee": 1.3,
                "forwardFee": {
                    "relayFee": "123",
                    "destinationGasOverhead": "456"
                }
            }
        ]"#;

        let fees: Vec<TransferFee> = serde_json::from_str(json).unwrap();

        assert_eq!(
            fees,
            vec![TransferFee::new(1000, FeeBps::from_hundredths(130))]
        );
    }

    #[test]
    fn forwarding_transfer_fee_preserves_atomic_tiers_and_combines_protocol_fee() {
        let json = r#"{
            "finalityThreshold": 1000,
            "minimumFee": 1.3,
            "forwardFee": {
                "low": 56035,
                "med": 57543,
                "high": "59052"
            }
        }"#;

        let fee: ForwardingTransferFee = serde_json::from_str(json).unwrap();

        assert_eq!(fee.finality(), Some(FinalityThreshold::Fast));
        assert_eq!(fee.minimum_fee(), FeeBps::from_hundredths(130));
        assert_eq!(fee.forward_fee().low(), U256::from(56_035_u64));
        assert_eq!(fee.forward_fee().medium(), U256::from(57_543_u64));
        assert_eq!(fee.forward_fee().high(), U256::from(59_052_u64));
        assert_eq!(
            fee.minimum_max_fee(U256::from(10_000_000_u64), ForwardingFeeLevel::Medium)
                .unwrap(),
            U256::from(58_843_u64)
        );
    }

    #[test]
    fn forwarding_transfer_fee_rejects_combined_fee_overflow() {
        let fee = ForwardingTransferFee::new(
            FinalityThreshold::Standard.as_u32(),
            FeeBps::from_hundredths(1),
            ForwardingFee::new(U256::MAX, U256::MAX, U256::MAX),
        );

        assert!(matches!(
            fee.minimum_max_fee(U256::from(1), ForwardingFeeLevel::Medium),
            Err(AmountError::ForwardingFeeOverflow {
                protocol_fee,
                forwarding_fee: U256::MAX,
            }) if protocol_fee == U256::from(1)
        ));
    }

    #[test]
    fn forwarding_transfer_fee_avoids_intermediate_percentage_overflow() {
        let fee = ForwardingTransferFee::new(
            FinalityThreshold::Fast.as_u32(),
            FeeBps::from_hundredths(130),
            ForwardingFee::new(U256::ZERO, U256::ZERO, U256::ZERO),
        );
        let recipient_amount = U256::MAX / U256::from(130_u64) + U256::from(1_u64);
        let expected = U256::from_str_radix(
            "115792089237316195423570985008687907853269984665640564039457584007913130",
            10,
        )
        .unwrap();

        assert_eq!(
            fee.minimum_max_fee(recipient_amount, ForwardingFeeLevel::Medium)
                .unwrap(),
            expected
        );
        assert!(recipient_amount.checked_add(expected).is_some());
    }

    #[test]
    fn forwarding_transfer_fee_rejects_protocol_fee_overflow() {
        let fee = ForwardingTransferFee::new(
            FinalityThreshold::Fast.as_u32(),
            FeeBps::from_hundredths(u32::MAX),
            ForwardingFee::new(U256::ZERO, U256::ZERO, U256::ZERO),
        );

        assert!(matches!(
            fee.minimum_max_fee(U256::MAX, ForwardingFeeLevel::Medium),
            Err(AmountError::ForwardingProtocolFeeOverflow {
                recipient_amount: U256::MAX,
                fee_hundredths: u32::MAX,
            })
        ));
    }

    #[test]
    fn forwarding_transfer_fee_rejects_fractional_atomic_tier() {
        let json = r#"{
            "finalityThreshold": 2000,
            "minimumFee": 0,
            "forwardFee": { "low": 1.5, "med": 2, "high": 3 }
        }"#;

        assert!(serde_json::from_str::<ForwardingTransferFee>(json).is_err());
    }

    #[test]
    fn fast_transfer_allowance_deserializes_numeric_response_as_atomic_units() {
        let json = r#"{
            "allowance": 52455615.650179,
            "lastUpdated": "2026-09-23T12:42:40.722Z"
        }"#;

        let allowance: FastTransferAllowance = serde_json::from_str(json).unwrap();

        assert_eq!(
            allowance,
            FastTransferAllowance::new(
                U256::from(52_455_615_650_179_u64),
                iris_timestamp("2026-09-23T12:42:40.722Z")
            )
        );
        assert!(allowance.is_sufficient_for(U256::from(52_455_615_650_179_u64)));
        assert!(!allowance.is_sufficient_for(U256::from(52_455_615_650_180_u64)));
    }

    #[test]
    fn fast_transfer_allowance_accepts_string_response_shape() {
        let json = r#"{
            "allowance": "0.000001",
            "lastUpdated": "2026-09-23T12:42:40.722Z"
        }"#;

        let allowance: FastTransferAllowance = serde_json::from_str(json).unwrap();

        assert_eq!(allowance.allowance, U256::from(1_u64));
    }

    #[test]
    fn fast_transfer_allowance_rejects_more_than_six_decimals() {
        let json = r#"{
            "allowance": "1.0000001",
            "lastUpdated": "2026-09-23T12:42:40.722Z"
        }"#;

        let result = serde_json::from_str::<FastTransferAllowance>(json);

        assert!(result.is_err());
    }

    #[test]
    fn fast_transfer_allowance_serializes_atomic_units_as_decimal_string() {
        let allowance = FastTransferAllowance::new(
            U256::from(52_455_615_650_179_u64),
            iris_timestamp("2026-09-23T12:42:40.722Z"),
        );

        let json = serde_json::to_value(allowance).unwrap();

        assert_eq!(json["allowance"], "52455615.650179");
        assert_eq!(json["lastUpdated"], "2026-09-23T12:42:40.722Z");

        let sub_usdc =
            FastTransferAllowance::new(U256::from(1_u64), iris_timestamp("2026-09-23T12:42:40Z"));
        let json = serde_json::to_value(sub_usdc).unwrap();

        assert_eq!(json["allowance"], "0.000001");
    }

    #[test]
    fn iris_timestamp_rejects_invalid_text_at_json_boundary() {
        let json = r#"{
            "allowance": "1",
            "lastUpdated": "not-a-timestamp"
        }"#;

        let result = serde_json::from_str::<FastTransferAllowance>(json);

        assert!(result.is_err());
    }

    #[test]
    fn iris_timestamp_serde_round_trip_uses_rfc3339_string() {
        let timestamp = iris_timestamp("2026-09-23T08:42:40.722-04:00");

        let json = serde_json::to_string(&timestamp).unwrap();
        let decoded: IrisTimestamp = serde_json::from_str(&json).unwrap();

        assert_eq!(json, r#""2026-09-23T12:42:40.722Z""#);
        assert_eq!(decoded, timestamp);
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
    fn fee_calculation_uses_burn_token_atomic_units() {
        let amount = U256::from(10_500_000u64);
        let fee = FeeBps::from_hundredths(100);

        assert_eq!(fee.apply_to_amount(amount).unwrap(), U256::from(1050u64));
        assert_eq!(
            fee.apply_to_amount_with_buffer_percent(amount, 20).unwrap(),
            U256::from(1260u64)
        );
    }

    #[test]
    fn fee_calculation_rounds_up_to_avoid_underquoting() {
        let amount = U256::from(1u64);
        let fee = FeeBps::from_hundredths(100);

        assert_eq!(fee.apply_to_amount(amount).unwrap(), U256::from(1u64));
    }

    #[test]
    fn fee_calculation_handles_zero_and_large_values() {
        let large_burn_token_amount = U256::from(1_000_000_000_000u64);
        let fractional_fee = FeeBps::from_hundredths(130);
        let zero_fee = FeeBps::from_hundredths(0);

        assert_eq!(
            fractional_fee.apply_to_amount(U256::ZERO).unwrap(),
            U256::ZERO
        );
        assert_eq!(
            zero_fee
                .apply_to_amount_with_buffer_percent(large_burn_token_amount, 20)
                .unwrap(),
            U256::ZERO
        );
        assert_eq!(
            fractional_fee
                .apply_to_amount(large_burn_token_amount)
                .unwrap(),
            U256::from(130_000_000u64)
        );
        assert_eq!(
            fractional_fee
                .apply_to_amount_with_buffer_percent(large_burn_token_amount, 20)
                .unwrap(),
            U256::from(156_000_000u64)
        );
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
