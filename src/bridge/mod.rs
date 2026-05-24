// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! Core CCTP bridge implementation
//!
//! This module provides the primary types and functionality for bridging USDC across
//! chains using Circle's Cross-Chain Transfer Protocol (CCTP).

mod bridge_trait;
mod cctp;
mod config;
pub mod multicall;
mod v2;

pub use bridge_trait::CctpBridge;
pub use cctp::Cctp;
pub use config::PollingConfig;
pub use multicall::{batch_token_state, TokenState};
pub use v2::{CctpV2, MintResult};

use crate::error::{AttestationFailureKind, CctpError};
use crate::spans;
use tracing::error;

pub(super) fn attestation_data_missing() -> CctpError {
    spans::record_error_with_context(
        "AttestationDataMissing",
        "Attestation status is complete but attestation field is null",
        Some("This indicates an unexpected API response format"),
    );
    error!(event = "attestation_data_missing");
    CctpError::AttestationFailed(AttestationFailureKind::AttestationMissing)
}

pub(super) fn attestation_api_reported_failed() -> CctpError {
    spans::record_error_with_context(
        "AttestationFailed",
        "Circle API returned failed status for attestation",
        Some("The message may be invalid or the source transaction may have failed"),
    );
    error!(event = "attestation_failed");
    CctpError::AttestationFailed(AttestationFailureKind::ApiReportedFailed)
}
