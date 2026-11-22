// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::AttestationFailureKind;
use alloy_primitives::{hex::FromHex, Bytes};
use serde::{Deserialize, Deserializer, Serialize};

/// The bytes of the attestation.
pub type AttestationBytes = Vec<u8>;

// ============================================================================
// V2 Attestation Response Types
// ============================================================================

/// Raw response from the CCTP v2 attestation API.
///
/// The v2 API uses a different endpoint format (`/v2/messages/{domain}?transactionHash={tx}`)
/// and returns a wrapper containing an array of messages, since a single transaction
/// can emit multiple `MessageSent` events.
///
/// This remains public for source compatibility and direct Iris inspection, but it is a
/// wire DTO rather than a validated operational state. The bridge normalizes every message
/// before making polling or minting decisions. A future major release may expose that
/// validated state publicly and narrow direct construction of this raw type.
///
/// # Example Response
///
/// ```json
/// {
///   "messages": [
///     {
///       "status": "complete",
///       "message": "0x...",
///       "attestation": "0x..."
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V2AttestationResponse {
    /// Array of messages from the transaction
    pub messages: Vec<V2Message>,
}

/// Raw message in a v2 attestation response.
///
/// Each message contains the attestation status, the original message bytes,
/// and the signed attestation (when complete). Its public fields intentionally preserve
/// malformed wire combinations so callers can inspect exactly what Iris returned; use the
/// bridge polling APIs for normalized behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V2Message {
    /// Status of the attestation
    pub status: AttestationStatus,

    /// The original message bytes from the `MessageSent` event
    #[serde(default, deserialize_with = "deserialize_optional_bytes_or_pending")]
    pub message: Option<Bytes>,

    /// The signed attestation bytes (null/PENDING until complete)
    #[serde(default, deserialize_with = "deserialize_optional_bytes_or_pending")]
    pub attestation: Option<Bytes>,
}

// ============================================================================
// V1 Attestation Response Types
// ============================================================================

/// Raw response from the v1 attestation service.
///
/// It contains the status of the attestation and optionally the attestation data itself.
/// The attestation data is a hex-encoded string (with or without "0x" prefix) that is
/// automatically deserialized into bytes.
///
/// **API Quirk**: Circle's Iris API sometimes returns the string `"PENDING"` for the
/// attestation field instead of `null` when the attestation is not yet ready. This
/// deserializer handles that case gracefully by treating "PENDING" as `None`.
///
/// This remains public for source compatibility and direct Iris inspection, but it is a
/// wire DTO rather than a validated operational state. The bridge normalizes it immediately
/// after deserialization. A future major release may expose the validated state publicly and
/// narrow direct construction of this raw type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttestationResponse {
    pub status: AttestationStatus,
    #[serde(default, deserialize_with = "deserialize_optional_bytes_or_pending")]
    pub attestation: Option<Bytes>,
}

/// Custom deserializer that handles Circle API quirk where attestation field
/// may be the string "PENDING" instead of null
///
/// Handles the following cases:
/// - Valid hex string (with or without "0x") → deserializes to `Some(Bytes)`
/// - "PENDING" or "pending" → returns `None`
/// - null or missing field → returns `None`
/// - Empty string → returns `Some(Bytes::new())` so normalization can reject it explicitly
/// - Invalid hex → returns error
fn deserialize_optional_bytes_or_pending<'de, D>(deserializer: D) -> Result<Option<Bytes>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;

    match opt {
        None => Ok(None),
        Some(s) if s.is_empty() => Ok(Some(Bytes::new())),
        Some(s) if s.eq_ignore_ascii_case("pending") => Ok(None),
        Some(s) => {
            let bytes = Bytes::from_hex(s).map_err(serde::de::Error::custom)?;
            Ok(Some(bytes))
        }
    }
}

/// Represents the status of the attestation.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttestationStatus {
    Complete,
    Pending,
    PendingConfirmations,
    Failed,
}

/// Validated operational state derived from an untrusted Iris wire DTO.
///
/// This type is crate-private while the public raw DTOs remain source-compatible. Keeping the
/// invalid combinations in one `Malformed` variant prevents bridge selectors from independently
/// rediscovering status/payload rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttestationState<T> {
    Ready(T),
    Pending,
    Failed,
    Malformed(AttestationFailureKind),
}

/// Non-empty canonical v2 message and attestation bytes ready for minting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct V2Attestation {
    pub(crate) message: Vec<u8>,
    pub(crate) attestation: AttestationBytes,
}

impl AttestationResponse {
    pub(crate) fn into_state(self) -> AttestationState<AttestationBytes> {
        match self.status {
            AttestationStatus::Complete => match self.attestation {
                None => AttestationState::Malformed(AttestationFailureKind::AttestationMissing),
                Some(bytes) if bytes.is_empty() => {
                    AttestationState::Malformed(AttestationFailureKind::AttestationEmpty)
                }
                Some(bytes) => AttestationState::Ready(bytes.to_vec()),
            },
            AttestationStatus::Pending | AttestationStatus::PendingConfirmations => {
                normalize_non_complete_attestation(self.attestation, AttestationState::Pending)
            }
            AttestationStatus::Failed => {
                normalize_non_complete_attestation(self.attestation, AttestationState::Failed)
            }
        }
    }
}

impl V2AttestationResponse {
    pub(crate) fn into_states(self) -> Vec<AttestationState<V2Attestation>> {
        self.messages
            .into_iter()
            .map(V2Message::into_state)
            .collect()
    }
}

impl V2Message {
    pub(crate) fn into_state(self) -> AttestationState<V2Attestation> {
        match self.status {
            AttestationStatus::Complete => normalize_complete_v2(self.message, self.attestation),
            AttestationStatus::Pending | AttestationStatus::PendingConfirmations => {
                normalize_pending_v2(self.message, self.attestation)
            }
            AttestationStatus::Failed => normalize_failed_v2(self.message, self.attestation),
        }
    }
}

fn normalize_non_complete_attestation<T>(
    attestation: Option<Bytes>,
    valid_state: AttestationState<T>,
) -> AttestationState<T> {
    match attestation {
        None => valid_state,
        Some(bytes) if bytes.is_empty() => {
            AttestationState::Malformed(AttestationFailureKind::AttestationEmpty)
        }
        Some(_) => AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation),
    }
}

fn normalize_complete_v2(
    message: Option<Bytes>,
    attestation: Option<Bytes>,
) -> AttestationState<V2Attestation> {
    let attestation = match attestation {
        None => {
            return AttestationState::Malformed(AttestationFailureKind::AttestationMissing);
        }
        Some(bytes) if bytes.is_empty() => {
            return AttestationState::Malformed(AttestationFailureKind::AttestationEmpty);
        }
        Some(bytes) => bytes,
    };
    let message = match message {
        None => return AttestationState::Malformed(AttestationFailureKind::MessageMissing),
        Some(bytes) if bytes.is_empty() => {
            return AttestationState::Malformed(AttestationFailureKind::MessageEmpty);
        }
        Some(bytes) => bytes,
    };

    AttestationState::Ready(V2Attestation {
        message: message.to_vec(),
        attestation: attestation.to_vec(),
    })
}

fn normalize_pending_v2(
    message: Option<Bytes>,
    attestation: Option<Bytes>,
) -> AttestationState<V2Attestation> {
    match attestation {
        Some(bytes) if bytes.is_empty() => {
            return AttestationState::Malformed(AttestationFailureKind::AttestationEmpty);
        }
        Some(_) => {
            return AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation);
        }
        None => {}
    }
    if message.as_ref().is_some_and(|bytes| bytes.is_empty()) {
        return AttestationState::Malformed(AttestationFailureKind::MessageEmpty);
    }

    AttestationState::Pending
}

fn normalize_failed_v2(
    message: Option<Bytes>,
    attestation: Option<Bytes>,
) -> AttestationState<V2Attestation> {
    match attestation {
        Some(bytes) if bytes.is_empty() => {
            return AttestationState::Malformed(AttestationFailureKind::AttestationEmpty);
        }
        Some(_) => {
            return AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation);
        }
        None => {}
    }
    match message {
        Some(bytes) if bytes.is_empty() => {
            AttestationState::Malformed(AttestationFailureKind::MessageEmpty)
        }
        Some(_) => AttestationState::Malformed(AttestationFailureKind::UnexpectedMessage),
        None => AttestationState::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence, RngSeed};

    #[derive(Clone, Debug)]
    enum PayloadCase {
        Missing,
        Empty,
        NonEmpty(Vec<u8>),
    }

    impl PayloadCase {
        fn as_bytes(&self) -> Option<Bytes> {
            match self {
                Self::Missing => None,
                Self::Empty => Some(Bytes::new()),
                Self::NonEmpty(bytes) => Some(Bytes::copy_from_slice(bytes)),
            }
        }
    }

    fn property_config() -> ProptestConfig {
        ProptestConfig {
            cases: 512,
            rng_seed: RngSeed::Fixed(0xA77E_57A7_10A1_0001),
            failure_persistence: Some(Box::new(FileFailurePersistence::WithSource("regressions"))),
            ..ProptestConfig::default()
        }
    }

    fn status_strategy() -> impl Strategy<Value = AttestationStatus> {
        prop_oneof![
            Just(AttestationStatus::Complete),
            Just(AttestationStatus::Pending),
            Just(AttestationStatus::PendingConfirmations),
            Just(AttestationStatus::Failed),
        ]
    }

    fn payload_strategy() -> impl Strategy<Value = PayloadCase> {
        prop_oneof![
            Just(PayloadCase::Missing),
            Just(PayloadCase::Empty),
            prop::collection::vec(any::<u8>(), 1..65).prop_map(PayloadCase::NonEmpty),
        ]
    }

    fn expected_v1_state(
        status: &AttestationStatus,
        attestation: &PayloadCase,
    ) -> AttestationState<AttestationBytes> {
        match attestation {
            PayloadCase::Missing => match status {
                AttestationStatus::Complete => {
                    AttestationState::Malformed(AttestationFailureKind::AttestationMissing)
                }
                AttestationStatus::Pending | AttestationStatus::PendingConfirmations => {
                    AttestationState::Pending
                }
                AttestationStatus::Failed => AttestationState::Failed,
            },
            PayloadCase::Empty => {
                AttestationState::Malformed(AttestationFailureKind::AttestationEmpty)
            }
            PayloadCase::NonEmpty(bytes) => match status {
                AttestationStatus::Complete => AttestationState::Ready(bytes.clone()),
                AttestationStatus::Pending
                | AttestationStatus::PendingConfirmations
                | AttestationStatus::Failed => {
                    AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation)
                }
            },
        }
    }

    fn expected_v2_state(
        status: &AttestationStatus,
        message: &PayloadCase,
        attestation: &PayloadCase,
    ) -> AttestationState<V2Attestation> {
        match attestation {
            PayloadCase::Empty => {
                return AttestationState::Malformed(AttestationFailureKind::AttestationEmpty);
            }
            PayloadCase::NonEmpty(_) if !matches!(status, AttestationStatus::Complete) => {
                return AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation);
            }
            PayloadCase::Missing if matches!(status, AttestationStatus::Complete) => {
                return AttestationState::Malformed(AttestationFailureKind::AttestationMissing);
            }
            PayloadCase::Missing | PayloadCase::NonEmpty(_) => {}
        }

        match status {
            AttestationStatus::Complete => match message {
                PayloadCase::Missing => {
                    AttestationState::Malformed(AttestationFailureKind::MessageMissing)
                }
                PayloadCase::Empty => {
                    AttestationState::Malformed(AttestationFailureKind::MessageEmpty)
                }
                PayloadCase::NonEmpty(message) => {
                    let PayloadCase::NonEmpty(attestation) = attestation else {
                        unreachable!("complete attestation was validated above");
                    };
                    AttestationState::Ready(V2Attestation {
                        message: message.clone(),
                        attestation: attestation.clone(),
                    })
                }
            },
            AttestationStatus::Pending | AttestationStatus::PendingConfirmations => match message {
                PayloadCase::Empty => {
                    AttestationState::Malformed(AttestationFailureKind::MessageEmpty)
                }
                PayloadCase::Missing | PayloadCase::NonEmpty(_) => AttestationState::Pending,
            },
            AttestationStatus::Failed => match message {
                PayloadCase::Missing => AttestationState::Failed,
                PayloadCase::Empty => {
                    AttestationState::Malformed(AttestationFailureKind::MessageEmpty)
                }
                PayloadCase::NonEmpty(_) => {
                    AttestationState::Malformed(AttestationFailureKind::UnexpectedMessage)
                }
            },
        }
    }

    proptest! {
        #![proptest_config(property_config())]

        #[test]
        fn v1_normalization_matches_the_status_payload_partition(
            status in status_strategy(),
            attestation in payload_strategy(),
        ) {
            let expected = expected_v1_state(&status, &attestation);
            let actual = AttestationResponse {
                status,
                attestation: attestation.as_bytes(),
            }
            .into_state();

            prop_assert_eq!(actual, expected);
        }

        #[test]
        fn v2_normalization_matches_the_status_payload_partition(
            status in status_strategy(),
            message in payload_strategy(),
            attestation in payload_strategy(),
        ) {
            let expected = expected_v2_state(&status, &message, &attestation);
            let actual = V2Message {
                status,
                message: message.as_bytes(),
                attestation: attestation.as_bytes(),
            }
            .into_state();

            prop_assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_deserialize_attestation_with_valid_hex() {
        let json = r#"{"status":"complete","attestation":"0x1234abcd"}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Complete);
        assert!(response.attestation.is_some());
        assert_eq!(
            response.attestation.unwrap().to_vec(),
            vec![0x12, 0x34, 0xab, 0xcd]
        );
    }

    #[test]
    fn test_deserialize_attestation_with_pending_string() {
        let json = r#"{"status":"pending","attestation":"PENDING"}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Pending);
        assert!(response.attestation.is_none());
    }

    #[test]
    fn test_deserialize_attestation_with_pending_lowercase() {
        let json = r#"{"status":"pending","attestation":"pending"}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Pending);
        assert!(response.attestation.is_none());
    }

    #[test]
    fn test_deserialize_attestation_with_null() {
        let json = r#"{"status":"pending","attestation":null}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Pending);
        assert!(response.attestation.is_none());
    }

    #[test]
    fn test_deserialize_attestation_missing_field() {
        let json = r#"{"status":"pending"}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Pending);
        assert!(response.attestation.is_none());
    }

    #[test]
    fn test_deserialize_attestation_with_empty_string() {
        let json = r#"{"status":"pending","attestation":""}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Pending);
        assert_eq!(response.attestation, Some(Bytes::new()));
    }

    #[test]
    fn test_deserialize_attestation_with_hex_no_prefix() {
        let json = r#"{"status":"complete","attestation":"deadbeef"}"#;
        let response: AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, AttestationStatus::Complete);
        assert!(response.attestation.is_some());
        assert_eq!(
            response.attestation.unwrap().to_vec(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn test_deserialize_attestation_with_invalid_hex_fails() {
        let json = r#"{"status":"complete","attestation":"not_valid_hex"}"#;
        let result = serde_json::from_str::<AttestationResponse>(json);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_all_status_variants() {
        let complete = r#"{"status":"complete"}"#;
        let pending = r#"{"status":"pending"}"#;
        let pending_confirmations = r#"{"status":"pending_confirmations"}"#;
        let failed = r#"{"status":"failed"}"#;

        assert_eq!(
            serde_json::from_str::<AttestationResponse>(complete)
                .unwrap()
                .status,
            AttestationStatus::Complete
        );
        assert_eq!(
            serde_json::from_str::<AttestationResponse>(pending)
                .unwrap()
                .status,
            AttestationStatus::Pending
        );
        assert_eq!(
            serde_json::from_str::<AttestationResponse>(pending_confirmations)
                .unwrap()
                .status,
            AttestationStatus::PendingConfirmations
        );
        assert_eq!(
            serde_json::from_str::<AttestationResponse>(failed)
                .unwrap()
                .status,
            AttestationStatus::Failed
        );
    }

    // ========================================================================
    // V2 Response Tests
    // ========================================================================

    #[test]
    fn test_v2_deserialize_complete_response() {
        let json = r#"{
            "messages": [
                {
                    "status": "complete",
                    "message": "0xdeadbeef",
                    "attestation": "0x1234abcd"
                }
            ]
        }"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].status, AttestationStatus::Complete);
        assert!(response.messages[0].message.is_some());
        assert!(response.messages[0].attestation.is_some());
        assert_eq!(
            response.messages[0].attestation.as_ref().unwrap().to_vec(),
            vec![0x12, 0x34, 0xab, 0xcd]
        );
        assert_eq!(
            response.messages[0].message.as_ref().unwrap().to_vec(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn test_v2_deserialize_pending_response() {
        let json = r#"{
            "messages": [
                {
                    "status": "pending",
                    "message": null,
                    "attestation": null
                }
            ]
        }"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].status, AttestationStatus::Pending);
        assert!(response.messages[0].message.is_none());
        assert!(response.messages[0].attestation.is_none());
    }

    #[test]
    fn test_v2_deserialize_pending_with_string() {
        let json = r#"{
            "messages": [
                {
                    "status": "pending",
                    "message": "PENDING",
                    "attestation": "PENDING"
                }
            ]
        }"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].status, AttestationStatus::Pending);
        assert!(response.messages[0].message.is_none());
        assert!(response.messages[0].attestation.is_none());
    }

    #[test]
    fn test_v2_deserialize_multiple_messages() {
        let json = r#"{
            "messages": [
                {
                    "status": "complete",
                    "message": "0xaa",
                    "attestation": "0xbb"
                },
                {
                    "status": "complete",
                    "message": "0xcc",
                    "attestation": "0xdd"
                }
            ]
        }"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 2);
        assert_eq!(response.messages[0].status, AttestationStatus::Complete);
        assert_eq!(response.messages[1].status, AttestationStatus::Complete);
    }

    #[test]
    fn test_v2_deserialize_empty_messages() {
        let json = r#"{"messages": []}"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 0);
    }

    #[test]
    fn test_v2_deserialize_pending_confirmations() {
        let json = r#"{
            "messages": [
                {
                    "status": "pending_confirmations",
                    "message": "0xdeadbeef",
                    "attestation": null
                }
            ]
        }"#;
        let response: V2AttestationResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.messages.len(), 1);
        assert_eq!(
            response.messages[0].status,
            AttestationStatus::PendingConfirmations
        );
        assert!(response.messages[0].message.is_some());
        assert!(response.messages[0].attestation.is_none());
    }

    #[test]
    fn v1_normalization_distinguishes_ready_pending_failed_and_malformed() {
        let ready = AttestationResponse {
            status: AttestationStatus::Complete,
            attestation: Some(Bytes::from_static(&[0xaa])),
        };
        assert_eq!(ready.into_state(), AttestationState::Ready(vec![0xaa]));

        let pending = AttestationResponse {
            status: AttestationStatus::Pending,
            attestation: None,
        };
        assert_eq!(pending.into_state(), AttestationState::Pending);

        let failed = AttestationResponse {
            status: AttestationStatus::Failed,
            attestation: None,
        };
        assert_eq!(failed.into_state(), AttestationState::Failed);

        let malformed = AttestationResponse {
            status: AttestationStatus::PendingConfirmations,
            attestation: Some(Bytes::from_static(&[0xbb])),
        };
        assert_eq!(
            malformed.into_state(),
            AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation)
        );
    }

    #[test]
    fn v1_normalization_distinguishes_missing_and_empty_attestation() {
        let missing = AttestationResponse {
            status: AttestationStatus::Complete,
            attestation: None,
        };
        assert_eq!(
            missing.into_state(),
            AttestationState::Malformed(AttestationFailureKind::AttestationMissing)
        );

        let empty_json = r#"{"status":"complete","attestation":""}"#;
        let empty: AttestationResponse = serde_json::from_str(empty_json).unwrap();
        assert_eq!(
            empty.into_state(),
            AttestationState::Malformed(AttestationFailureKind::AttestationEmpty)
        );
    }

    #[test]
    fn v2_normalization_distinguishes_empty_payloads() {
        let empty_message_json = r#"{
            "messages": [{
                "status": "complete",
                "message": "",
                "attestation": "0xaa"
            }]
        }"#;
        let empty_message: V2AttestationResponse =
            serde_json::from_str(empty_message_json).unwrap();
        assert_eq!(
            empty_message.into_states(),
            vec![AttestationState::Malformed(
                AttestationFailureKind::MessageEmpty
            )]
        );

        let empty_attestation_json = r#"{
            "messages": [{
                "status": "complete",
                "message": "0xaa",
                "attestation": ""
            }]
        }"#;
        let empty_attestation: V2AttestationResponse =
            serde_json::from_str(empty_attestation_json).unwrap();
        assert_eq!(
            empty_attestation.into_states(),
            vec![AttestationState::Malformed(
                AttestationFailureKind::AttestationEmpty
            )]
        );
    }

    #[test]
    fn v2_normalization_rejects_payloads_for_non_complete_statuses() {
        let pending_with_attestation = V2Message {
            status: AttestationStatus::Pending,
            message: Some(Bytes::from_static(&[0xaa])),
            attestation: Some(Bytes::from_static(&[0xbb])),
        };
        assert_eq!(
            pending_with_attestation.into_state(),
            AttestationState::Malformed(AttestationFailureKind::UnexpectedAttestation)
        );

        let failed_with_message = V2Message {
            status: AttestationStatus::Failed,
            message: Some(Bytes::from_static(&[0xaa])),
            attestation: None,
        };
        assert_eq!(
            failed_with_message.into_state(),
            AttestationState::Malformed(AttestationFailureKind::UnexpectedMessage)
        );
    }
}
