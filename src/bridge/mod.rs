// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! Core CCTP bridge implementation
//!
//! This module provides the primary types and functionality for bridging modeled
//! CCTP assets across chains using Circle's Cross-Chain Transfer Protocol
//! (CCTP).

mod bridge_trait;
mod cctp;
mod config;
pub mod multicall;
mod transfer_mode;
mod v2;

pub use bridge_trait::CctpBridge;
pub use cctp::Cctp;
pub use config::PollingConfig;
pub use multicall::{batch_token_state, TokenState};
pub use transfer_mode::TransferMode;
pub use v2::{CctpV2, MintResult};

use crate::error::{AttestationFailureKind, CctpError};
use crate::spans;
use tracing::error;

pub(super) fn attestation_api_reported_failed() -> CctpError {
    spans::record_error_with_context(
        "AttestationFailed",
        "Circle API returned failed status for attestation",
        Some("The message may be invalid or the source transaction may have failed"),
    );
    error!(event = "attestation_failed");
    CctpError::AttestationFailed(AttestationFailureKind::ApiReportedFailed)
}

pub(super) fn malformed_attestation_response(kind: AttestationFailureKind) -> CctpError {
    let (error_type, message, context, event) = match kind {
        AttestationFailureKind::AttestationMissing => (
            "AttestationDataMissing",
            "Attestation status is complete but attestation field is null",
            "This indicates an unexpected API response format",
            "attestation_data_missing",
        ),
        AttestationFailureKind::MessageMissing => (
            "MessageDataMissing",
            "Attestation status is complete but message field is null",
            "This indicates an unexpected API response format",
            "message_data_missing",
        ),
        _ => (
            "MalformedAttestationResponse",
            "Iris returned an invalid attestation status/payload combination",
            "The response is not safe to consume",
            "malformed_attestation_response",
        ),
    };
    spans::record_error_with_context(error_type, message, Some(context));
    error!(reason = %kind, event = event);
    CctpError::AttestationFailed(kind)
}
