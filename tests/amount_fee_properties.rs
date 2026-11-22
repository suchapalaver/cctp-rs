// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Property checks for money-critical amount parsing and fee arithmetic.

use alloy_primitives::{U256, U512};
use cctp_rs::{
    AmountError, CctpError, FeeBps, ForwardingFee, ForwardingFeeLevel, ForwardingTransferFee,
    UsdcAmount,
};
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence, RngSeed};

const FEE_DENOMINATOR: u64 = 1_000_000;
const USDC_SCALE: u128 = 1_000_000;

fn property_config() -> ProptestConfig {
    ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(0xC1CC_7A14_2026_0001),
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
        ..ProptestConfig::default()
    }
}

fn u256_strategy() -> impl Strategy<Value = U256> {
    prop_oneof![
        Just(U256::ZERO),
        Just(U256::MAX),
        any::<[u64; 4]>().prop_map(U256::from_limbs),
    ]
}

fn widen(value: U256) -> U512 {
    U512::from_limbs_slice(&value.into_limbs())
}

fn narrow(value: U512) -> Option<U256> {
    U256::checked_from_limbs_slice(value.as_limbs())
}

fn ceil_div(numerator: U512, denominator: U512) -> U512 {
    if numerator == U512::ZERO {
        U512::ZERO
    } else {
        ((numerator - U512::from(1)) / denominator) + U512::from(1)
    }
}

fn oracle_fee(amount: U256, fee_hundredths: u32) -> U512 {
    ceil_div(
        widen(amount) * U512::from(fee_hundredths),
        U512::from(FEE_DENOMINATOR),
    )
}

fn fee_level(index: u8) -> ForwardingFeeLevel {
    match index {
        0 => ForwardingFeeLevel::Low,
        1 => ForwardingFeeLevel::Medium,
        _ => ForwardingFeeLevel::High,
    }
}

fn invalid_decimal_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        (prop_oneof![Just('+'), Just('-')], "[0-9]{1,20}")
            .prop_map(|(sign, digits)| format!("{sign}{digits}")),
        ("[0-9]{0,12}", "[0-9]{0,12}", "[0-9]{0,12}")
            .prop_map(|(left, middle, right)| format!("{left}.{middle}.{right}")),
        ("[0-9]{0,12}", "[a-zA-Z]", "[0-9]{0,12}")
            .prop_map(|(left, letter, right)| format!("{left}{letter}{right}")),
        Just(String::new()),
        Just(".".to_string()),
    ]
}

proptest! {
    #![proptest_config(property_config())]

    #[test]
    fn formatted_usdc_round_trip_preserves_u256_atomic_amounts(
        atomic in u256_strategy().prop_filter("USDC amounts must be nonzero", |amount| *amount != U256::ZERO),
    ) {
        let formatted = UsdcAmount::from_atomic(atomic).to_string();
        let reparsed = UsdcAmount::parse_decimal(&formatted).expect("formatted amount must parse");

        prop_assert_eq!(reparsed.atomic(), atomic);
    }

    #[test]
    fn valid_usdc_grammar_maps_exactly_to_atomic_units(
        whole in 0u64..=1_000_000_000_000,
        fractional_digits in 0u8..=6,
        raw_fraction in any::<u32>(),
        omit_zero_whole in any::<bool>(),
        trailing_dot in any::<bool>(),
    ) {
        let fractional_scale = 10u128.pow(u32::from(fractional_digits));
        let fraction = u128::from(raw_fraction) % fractional_scale;
        let atomic = u128::from(whole) * USDC_SCALE
            + fraction * 10u128.pow(u32::from(6 - fractional_digits));
        prop_assume!(atomic != 0);

        let input = if fractional_digits == 0 {
            if trailing_dot {
                format!("{whole}.")
            } else {
                whole.to_string()
            }
        } else if whole == 0 && omit_zero_whole {
            format!(".{fraction:0width$}", width = usize::from(fractional_digits))
        } else {
            format!("{whole}.{fraction:0width$}", width = usize::from(fractional_digits))
        };

        let parsed = UsdcAmount::parse_decimal(&input).expect("generated grammar is valid");
        prop_assert_eq!(parsed.atomic(), U256::from(atomic));
    }

    #[test]
    fn malformed_usdc_grammar_has_a_typed_error(input in invalid_decimal_strategy()) {
        prop_assert!(matches!(
            UsdcAmount::parse_decimal(&input),
            Err(CctpError::InvalidAmount(AmountError::InvalidDecimal))
        ));
    }

    #[test]
    fn excessive_usdc_precision_reports_the_exact_count(
        whole in "[0-9]{0,20}",
        fraction in "[0-9]{7,24}",
    ) {
        let input = format!("{whole}.{fraction}");
        let has_expected_error = matches!(
            UsdcAmount::parse_decimal(&input),
            Err(CctpError::InvalidAmount(AmountError::TooManyDecimalPlaces { actual }))
                if actual == fraction.len()
        );
        prop_assert!(has_expected_error);
    }

    #[test]
    fn zero_usdc_spellings_have_a_typed_error(
        whole_zeros in 1usize..40,
        fractional_zeros in 0usize..=6,
        trailing_dot in any::<bool>(),
    ) {
        let whole = "0".repeat(whole_zeros);
        let input = if fractional_zeros == 0 {
            if trailing_dot { format!(" {whole}. ") } else { format!(" {whole} ") }
        } else {
            format!(" {whole}.{} ", "0".repeat(fractional_zeros))
        };

        prop_assert!(matches!(
            UsdcAmount::parse_decimal(&input),
            Err(CctpError::InvalidAmount(AmountError::Zero))
        ));
    }

    #[test]
    fn fee_result_matches_wide_rational_or_typed_overflow(
        amount in u256_strategy(),
        fee_hundredths in any::<u32>(),
    ) {
        let fee = FeeBps::from_hundredths(fee_hundredths);
        let exact_numerator = widen(amount) * U512::from(fee_hundredths);
        let expected = oracle_fee(amount, fee_hundredths);

        match narrow(expected) {
            Some(expected) => {
                let actual = fee.apply_to_amount(amount).expect("wide result fits uint256");
                prop_assert_eq!(actual, expected);
                prop_assert!(widen(actual) * U512::from(FEE_DENOMINATOR) >= exact_numerator);
                if actual != U256::ZERO {
                    prop_assert!(
                        widen(actual - U256::from(1)) * U512::from(FEE_DENOMINATOR)
                            < exact_numerator
                    );
                }
            }
            None => prop_assert_eq!(
                fee.apply_to_amount(amount),
                Err(AmountError::FeeOverflow { amount, fee_hundredths }),
            ),
        }
    }

    #[test]
    fn fees_are_monotone_in_amount_and_rate(
        first_amount in any::<u128>(),
        second_amount in any::<u128>(),
        first_rate in any::<u32>(),
        second_rate in any::<u32>(),
    ) {
        let low_amount = U256::from(first_amount.min(second_amount));
        let high_amount = U256::from(first_amount.max(second_amount));
        let low_rate = first_rate.min(second_rate);
        let high_rate = first_rate.max(second_rate);

        let low = FeeBps::from_hundredths(low_rate)
            .apply_to_amount(low_amount)
            .expect("u128 amount times u32 rate fits uint256");
        let amount_increased = FeeBps::from_hundredths(low_rate)
            .apply_to_amount(high_amount)
            .expect("u128 amount times u32 rate fits uint256");
        let both_increased = FeeBps::from_hundredths(high_rate)
            .apply_to_amount(high_amount)
            .expect("u128 amount times u32 rate fits uint256");

        prop_assert!(low <= amount_increased);
        prop_assert!(amount_increased <= both_increased);
    }

    #[test]
    fn buffered_fee_matches_wide_oracle_and_never_decreases_fee(
        amount in u256_strategy(),
        fee_hundredths in any::<u32>(),
        buffer_percent in any::<u32>(),
    ) {
        let fee = FeeBps::from_hundredths(fee_hundredths);
        let base_wide = oracle_fee(amount, fee_hundredths);
        let actual = fee.apply_to_amount_with_buffer_percent(amount, buffer_percent);

        match narrow(base_wide) {
            None => prop_assert_eq!(
                actual,
                Err(AmountError::FeeOverflow { amount, fee_hundredths }),
            ),
            Some(base_fee) => {
                let expected_wide = ceil_div(
                    base_wide * U512::from(u64::from(buffer_percent) + 100),
                    U512::from(100),
                );
                match narrow(expected_wide) {
                    Some(expected) => {
                        let buffered = actual.expect("wide buffered result fits uint256");
                        prop_assert_eq!(buffered, expected);
                        prop_assert!(buffered >= base_fee);
                    }
                    None => prop_assert_eq!(
                        actual,
                        Err(AmountError::FeeBufferOverflow {
                            fee: base_fee,
                            buffer_percent,
                        }),
                    ),
                }
            }
        }
    }

    #[test]
    fn forwarding_fee_and_burn_composition_match_wide_oracle(
        recipient_amount in u256_strategy(),
        fee_hundredths in any::<u32>(),
        low in u256_strategy(),
        medium in u256_strategy(),
        high in u256_strategy(),
        level_index in 0u8..3,
    ) {
        let level = fee_level(level_index);
        let quote = ForwardingTransferFee::new(
            1_000,
            FeeBps::from_hundredths(fee_hundredths),
            ForwardingFee::new(low, medium, high),
        );
        let forwarding_fee = quote.forward_fee().at(level);
        let protocol_wide = oracle_fee(recipient_amount, fee_hundredths);
        let combined_wide = protocol_wide + widen(forwarding_fee);
        let actual_fee = quote.minimum_max_fee(recipient_amount, level);

        let expected_fee = match narrow(protocol_wide) {
            None => Err(AmountError::ForwardingProtocolFeeOverflow {
                recipient_amount,
                fee_hundredths,
            }),
            Some(protocol_fee) => match narrow(combined_wide) {
                Some(combined) => Ok(combined),
                None => Err(AmountError::ForwardingFeeOverflow {
                    protocol_fee,
                    forwarding_fee,
                }),
            },
        };
        prop_assert_eq!(actual_fee, expected_fee);

        let actual_plan = quote.minimum_burn_plan(recipient_amount, level);
        if recipient_amount == U256::ZERO {
            prop_assert_eq!(actual_plan, Err(AmountError::Zero));
        } else {
            match expected_fee {
                Err(error) => prop_assert_eq!(actual_plan, Err(error)),
                Ok(max_fee) => {
                    let burn_wide = widen(recipient_amount) + widen(max_fee);
                    match narrow(burn_wide) {
                        Some(expected) => {
                            let plan = actual_plan.expect("wide burn amount fits uint256");
                            prop_assert_eq!(plan.max_fee(), max_fee);
                            prop_assert_eq!(plan.burn_amount(), expected);
                        }
                        None => prop_assert_eq!(
                            actual_plan,
                            Err(AmountError::ForwardingBurnAmountOverflow {
                                recipient_amount,
                                max_fee,
                            }),
                        ),
                    }
                }
            }
        }
    }
}
