// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 transfer mode selection.
//!
//! Circle's v2 `depositForBurn` family exposes two orthogonal choices: the
//! finality threshold (fast vs. standard) and whether the burn carries hook
//! data for a post-mint action on the destination chain. This module encodes
//! the valid combinations as a single enum so the mode is explicit at
//! the API boundary instead of being inferred from independent flags.
//!
//! Both `depositForBurn` and `depositForBurnWithHook` accept `maxFee` and
//! `minFinalityThreshold` on-chain, so pairing hooks with fast finality is
//! supported by the protocol — see Circle's CCTP v2 reference at
//! <https://developers.circle.com/cctp/evm-smart-contracts>.
//!
//! [`TransferMode::Standard`] is the default and sends a zero `maxFee`.
//! Deployments with Circle's Standard Transfer fee switch use the explicit
//! [`TransferMode::StandardWithFee`] variants instead.

use alloy_primitives::{Bytes, U256};

use crate::protocol::FinalityThreshold;

/// Selects which CCTP v2 burn variant the bridge sends.
///
/// Replaces the legacy independent `fast_transfer` / `hook_data` / `max_fee`
/// fields. The enum captures the exact wire shape of the valid
/// configurations and makes the relationship between fee, finality, and hook
/// data unambiguous at the type level.
///
/// # Examples
///
/// ```rust
/// use cctp_rs::{FinalityThreshold, TransferMode};
/// use alloy_primitives::{Bytes, U256};
///
/// let standard = TransferMode::Standard;
/// assert_eq!(standard.finality_threshold(), FinalityThreshold::Standard);
/// assert!(standard.hook_data().is_none());
///
/// let fast = TransferMode::Fast { max_fee: U256::from(500) };
/// assert_eq!(fast.finality_threshold(), FinalityThreshold::Fast);
/// assert_eq!(fast.max_fee(), U256::from(500));
///
/// let fast_with_hook = TransferMode::FastWithHook {
///     max_fee: U256::from(500),
///     hook_data: Bytes::from(vec![0xde, 0xad]),
/// };
/// assert!(fast_with_hook.is_fast());
/// assert_eq!(fast_with_hook.hook_data().map(|b| b.len()), Some(2));
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransferMode {
    /// Plain burn at finalized finality (threshold 2000), no hooks.
    ///
    /// Settlement matches v1 timing (13–19 min). No fast-transfer fee.
    #[default]
    Standard,

    /// Plain burn at finalized finality with a Standard Transfer fee cap.
    ///
    /// This mode is accepted only on source deployments that expose Circle's
    /// Standard Transfer fee switch. Query the current minimum with
    /// `CctpV2Bridge::get_min_standard_transfer_fee_amount` before selecting
    /// the cap.
    StandardWithFee {
        /// Maximum Standard Transfer fee, in burn-token atomic units.
        max_fee: U256,
    },

    /// Fast burn at confirmed finality (threshold 1000), no hooks.
    ///
    /// `max_fee` caps the fast-transfer fee Circle's relayers may deduct
    /// from the minted amount. A fee below the chain's minimum will leave
    /// the burn pending until enough finality accrues for the standard
    /// path.
    Fast {
        /// Maximum fast-transfer fee, in burn-token atomic units.
        max_fee: U256,
    },

    /// Burn carrying hook data at finalized finality.
    ///
    /// The hook data is opaque to CCTP and runs on the destination chain
    /// after the mint.
    StandardWithHook {
        /// Hook payload forwarded to the destination chain.
        hook_data: Bytes,
    },

    /// Burn carrying hook data at finalized finality with a Standard Transfer
    /// fee cap.
    StandardWithFeeAndHook {
        /// Maximum Standard Transfer fee, in burn-token atomic units.
        max_fee: U256,
        /// Hook payload forwarded to the destination chain.
        hook_data: Bytes,
    },

    /// Burn carrying hook data at confirmed (fast) finality.
    ///
    /// Combines a fast-finality attestation with a post-mint hook. Both
    /// `maxFee` and `hookData` are passed through to
    /// `depositForBurnWithHook` on-chain.
    FastWithHook {
        /// Maximum fast-transfer fee, in burn-token atomic units.
        max_fee: U256,
        /// Hook payload forwarded to the destination chain.
        hook_data: Bytes,
    },
}

impl TransferMode {
    /// Returns the finality threshold this mode requests from Circle.
    #[inline]
    #[must_use]
    pub const fn finality_threshold(&self) -> FinalityThreshold {
        match self {
            Self::Standard
            | Self::StandardWithFee { .. }
            | Self::StandardWithHook { .. }
            | Self::StandardWithFeeAndHook { .. } => FinalityThreshold::Standard,
            Self::Fast { .. } | Self::FastWithHook { .. } => FinalityThreshold::Fast,
        }
    }

    /// Returns `true` when the mode requests fast (confirmed) finality.
    #[inline]
    #[must_use]
    pub const fn is_fast(&self) -> bool {
        match self {
            Self::Fast { .. } | Self::FastWithHook { .. } => true,
            Self::Standard
            | Self::StandardWithFee { .. }
            | Self::StandardWithHook { .. }
            | Self::StandardWithFeeAndHook { .. } => false,
        }
    }

    /// Returns `true` when the mode carries hook data.
    #[inline]
    #[must_use]
    pub const fn has_hook(&self) -> bool {
        match self {
            Self::StandardWithHook { .. }
            | Self::StandardWithFeeAndHook { .. }
            | Self::FastWithHook { .. } => true,
            Self::Standard | Self::StandardWithFee { .. } | Self::Fast { .. } => false,
        }
    }

    /// Returns `true` when a Standard Transfer explicitly opts into the
    /// on-chain fee switch.
    #[inline]
    #[must_use]
    pub const fn uses_standard_transfer_fee_switch(&self) -> bool {
        matches!(
            self,
            Self::StandardWithFee { .. } | Self::StandardWithFeeAndHook { .. }
        )
    }

    /// Returns `true` when the mode carries an explicit `maxFee` cap.
    #[inline]
    #[must_use]
    pub const fn has_explicit_fee_cap(&self) -> bool {
        self.is_fast() || self.uses_standard_transfer_fee_switch()
    }

    /// Returns the transfer fee cap sent on-chain.
    ///
    /// Defaults to `U256::ZERO` for legacy standard modes. Explicit
    /// fee-switch standard modes and fast modes return their configured cap.
    #[inline]
    #[must_use]
    pub fn max_fee(&self) -> U256 {
        match self {
            Self::StandardWithFee { max_fee }
            | Self::StandardWithFeeAndHook { max_fee, .. }
            | Self::Fast { max_fee }
            | Self::FastWithHook { max_fee, .. } => *max_fee,
            Self::Standard | Self::StandardWithHook { .. } => U256::ZERO,
        }
    }

    /// Returns the hook payload, if any.
    #[inline]
    #[must_use]
    pub const fn hook_data(&self) -> Option<&Bytes> {
        match self {
            Self::StandardWithHook { hook_data }
            | Self::StandardWithFeeAndHook { hook_data, .. }
            | Self::FastWithHook { hook_data, .. } => Some(hook_data),
            Self::Standard | Self::StandardWithFee { .. } | Self::Fast { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_defaults() {
        let mode = TransferMode::default();
        assert_eq!(mode, TransferMode::Standard);
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Standard);
        assert!(!mode.is_fast());
        assert!(!mode.has_hook());
        assert_eq!(mode.max_fee(), U256::ZERO);
        assert!(mode.hook_data().is_none());
    }

    #[test]
    fn fast_carries_fee() {
        let mode = TransferMode::Fast {
            max_fee: U256::from(1234),
        };
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Fast);
        assert!(mode.is_fast());
        assert!(!mode.has_hook());
        assert_eq!(mode.max_fee(), U256::from(1234));
        assert!(mode.hook_data().is_none());
    }

    #[test]
    fn standard_fee_switch_carries_fee_without_fast_finality() {
        let mode = TransferMode::StandardWithFee {
            max_fee: U256::from(42),
        };
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Standard);
        assert!(!mode.is_fast());
        assert!(mode.uses_standard_transfer_fee_switch());
        assert!(mode.has_explicit_fee_cap());
        assert_eq!(mode.max_fee(), U256::from(42));
        assert!(mode.hook_data().is_none());
    }

    #[test]
    fn standard_with_hook_keeps_standard_finality() {
        let hook = Bytes::from(vec![1, 2, 3]);
        let mode = TransferMode::StandardWithHook {
            hook_data: hook.clone(),
        };
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Standard);
        assert!(!mode.is_fast());
        assert!(mode.has_hook());
        assert_eq!(mode.max_fee(), U256::ZERO);
        assert_eq!(mode.hook_data(), Some(&hook));
    }

    #[test]
    fn fast_with_hook_combines_both() {
        let hook = Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]);
        let mode = TransferMode::FastWithHook {
            max_fee: U256::from(500),
            hook_data: hook.clone(),
        };
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Fast);
        assert!(mode.is_fast());
        assert!(mode.has_hook());
        assert_eq!(mode.max_fee(), U256::from(500));
        assert_eq!(mode.hook_data(), Some(&hook));
    }

    #[test]
    fn standard_fee_switch_with_hook_carries_both() {
        let hook = Bytes::from(vec![0xca, 0xfe]);
        let mode = TransferMode::StandardWithFeeAndHook {
            max_fee: U256::from(42),
            hook_data: hook.clone(),
        };
        assert_eq!(mode.finality_threshold(), FinalityThreshold::Standard);
        assert!(!mode.is_fast());
        assert!(mode.has_hook());
        assert!(mode.uses_standard_transfer_fee_switch());
        assert_eq!(mode.max_fee(), U256::from(42));
        assert_eq!(mode.hook_data(), Some(&hook));
    }
}
