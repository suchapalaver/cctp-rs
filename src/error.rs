// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use crate::primitives::CctpTransferAsset;
use alloy_chains::NamedChain;
use alloy_json_rpc::RpcError;
use alloy_primitives::{TxHash, U256};
use alloy_transport::TransportErrorKind;
use std::fmt;
use thiserror::Error;

/// Error returned when a restricted CCTP destination caller is the zero word.
///
/// CCTP reserves the all-zero `destinationCaller` for permissionless relay, so
/// it cannot express a restricted caller.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("restricted destination caller must be a non-zero bytes32 value")]
pub struct InvalidDestinationCaller;

/// Known revert reason patterns that indicate a message was already processed.
/// These are matched case-insensitively against error messages.
const ALREADY_RELAYED_PATTERNS: &[&str] = &[
    "nonce already used",
    "already received",
    "already processed",
    "message already received",
    "nonce used",
];

/// Categorical reasons an attestation poll can fail.
///
/// Carried by [`CctpError::AttestationFailed`] so callers can react
/// to specific failure modes without substring-matching on a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttestationFailureKind {
    /// The Iris API returned `AttestationStatus::Failed`.
    ApiReportedFailed,
    /// Status was `Complete` but the `attestation` field was null.
    AttestationMissing,
    /// Status was `Complete` but the `message` field was null (v2 only).
    MessageMissing,
    /// The `attestation` field was present but decoded to zero bytes.
    AttestationEmpty,
    /// The `message` field was present but decoded to zero bytes (v2 only).
    MessageEmpty,
    /// A non-complete response unexpectedly carried attestation bytes.
    UnexpectedAttestation,
    /// A failed response unexpectedly carried message bytes (v2 only).
    UnexpectedMessage,
}

impl fmt::Display for AttestationFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::ApiReportedFailed => "Iris API reported failed status",
            Self::AttestationMissing => "attestation field missing in complete response",
            Self::AttestationEmpty => "attestation field is empty",
            Self::MessageMissing => "message field missing in complete response",
            Self::MessageEmpty => "message field is empty",
            Self::UnexpectedAttestation => "attestation field present in a non-complete response",
            Self::UnexpectedMessage => "message field present in a failed response",
        };
        f.write_str(msg)
    }
}

/// Categorical reasons SDK configuration validation can fail.
///
/// Carried by [`CctpError::InvalidConfig`] so callers can react to known
/// invalid states without parsing human-readable error messages.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// A polling loop was configured to make no attempts.
    #[error("PollingConfig.max_attempts must be greater than 0")]
    PollingMaxAttemptsZero,
    /// A polling loop was configured to retry without sleeping.
    #[error("PollingConfig.poll_interval_secs must be greater than 0")]
    PollingIntervalZero,
    /// A provider was configured to time out immediately.
    #[error("ProviderConfig.timeout must be greater than zero")]
    ProviderTimeoutZero,
    /// A provider throttle was configured to permit no requests.
    #[error("ProviderConfig.rate_limit_rps must be greater than zero when set")]
    ProviderRateLimitZero,
    /// A CCTP route used the same chain as both endpoints.
    #[error("source and destination chains must differ (both were {chain:?})")]
    RouteSameChain { chain: NamedChain },
    /// A CCTP route crossed between mainnet and testnet environments.
    #[error(
        "source and destination chains must both be mainnet or both be testnet (got {source_chain:?} -> {destination_chain:?})"
    )]
    RouteEnvironmentMismatch {
        source_chain: NamedChain,
        destination_chain: NamedChain,
    },
}

/// Categorical reasons SDK amount validation can fail.
///
/// Carried by [`CctpError::InvalidAmount`] so callers can distinguish invalid
/// user input from fee-relationship failures without parsing display text.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AmountError {
    /// Decimal input does not use the accepted unsigned decimal grammar.
    #[error("amount must be an unsigned decimal number")]
    InvalidDecimal,
    /// Decimal USDC input exceeds the token's six supported decimal places.
    #[error("USDC amounts support at most 6 decimal places, got {actual}")]
    TooManyDecimalPlaces { actual: usize },
    /// The amount cannot be represented in the SDK's atomic-unit range.
    #[error("amount is too large")]
    TooLarge,
    /// A transfer amount resolved to zero atomic units.
    #[error("amount must be greater than zero")]
    Zero,
    /// A Standard Transfer fee cap was not smaller than the burn amount.
    #[error("Standard Transfer max_fee ({max_fee}) must be less than the burn amount ({amount})")]
    StandardTransferFeeNotLessThanAmount { max_fee: U256, amount: U256 },
    /// A Forwarding Service fee cap was not smaller than the burn amount.
    #[error("Forwarding Service max_fee ({max_fee}) must be less than the burn amount ({amount})")]
    ForwardingFeeNotLessThanAmount { max_fee: U256, amount: U256 },
    /// The percentage protocol fee could not be represented in the on-chain
    /// `uint256` range.
    #[error(
        "Forwarding Service protocol fee exceeds uint256 for recipient amount {recipient_amount} at {fee_hundredths} hundredths of a basis point"
    )]
    ForwardingProtocolFeeOverflow {
        recipient_amount: U256,
        fee_hundredths: u32,
    },
    /// The protocol and Forwarding Service fees could not be added without
    /// exceeding the on-chain `uint256` range.
    #[error(
        "combined Forwarding Service fee exceeds uint256 (protocol fee {protocol_fee}, forwarding fee {forwarding_fee})"
    )]
    ForwardingFeeOverflow {
        protocol_fee: U256,
        forwarding_fee: U256,
    },
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CctpError {
    #[error("Unsupported chain: {0:?}")]
    UnsupportedChain(alloy_chains::NamedChain),

    #[error(
        "Fast Transfer is not available from source chain {source_chain:?}; use a standard transfer"
    )]
    FastTransferUnsupported { source_chain: NamedChain },

    #[error(
        "Standard Transfer fee switch is not available from source chain {source_chain:?}; use TransferMode::Standard with a zero maxFee"
    )]
    StandardTransferFeeSwitchUnsupported { source_chain: NamedChain },

    #[error(
        "Circle Forwarding Service is not available for destination chain {destination_chain:?}"
    )]
    ForwardingServiceUnsupported { destination_chain: NamedChain },

    #[error("Circle Forwarding Service is modeled for USDC only, not {asset}")]
    ForwardingServiceAssetUnsupported { asset: CctpTransferAsset },

    #[error(
        "Circle Forwarding Service modes cannot use CctpV2Bridge::transfer because Circle relays the destination mint; use burn or burn_asset and wait_for_receive"
    )]
    ForwardingServiceSelfRelayUnsupported,

    #[error("Unsupported CCTP asset {asset} on chain {chain:?}: {reason}")]
    UnsupportedAssetChain {
        asset: CctpTransferAsset,
        chain: NamedChain,
        reason: String,
    },

    #[error(
        "Unsupported CCTP asset {asset} for route {source_chain:?}->{destination_chain:?}: {reason}"
    )]
    UnsupportedAssetRoute {
        asset: CctpTransferAsset,
        source_chain: NamedChain,
        destination_chain: NamedChain,
        reason: String,
    },

    #[error("Message already relayed (transfer successful via third party): {original}")]
    AlreadyRelayed { original: String },

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// A typed contract-call error from `alloy_contract`. Preserves alloy's
    /// structured introspection so callers can use [`alloy_contract::Error::as_revert_data`]
    /// and [`alloy_contract::Error::as_decoded_interface_error`] for revert decoding.
    #[error(transparent)]
    Contract(#[from] alloy_contract::Error),

    #[error("Attestation failed: {0}")]
    AttestationFailed(AttestationFailureKind),

    /// The source-chain RPC returned no receipt for the given transaction
    /// hash. The transaction may not have been mined yet, may have been
    /// dropped, or the queried node may not have indexed it.
    #[error("Transaction not found: {tx_hash}")]
    TransactionNotFound { tx_hash: TxHash },

    /// The transaction receipt was found but did not contain a `MessageSent`
    /// log. This indicates the transaction did not emit the CCTP bridge
    /// event — typically because the call hit the wrong contract or
    /// reverted before reaching the burn step.
    #[error("MessageSent event not found in transaction logs: {tx_hash}")]
    MessageSentEventMissing { tx_hash: TxHash },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(#[from] ConfigError),

    #[error("Invalid amount: {0}")]
    InvalidAmount(#[from] AmountError),

    #[error(
        "Transfer fee unavailable for route {source_domain}->{destination_domain} at finality threshold {finality_threshold}"
    )]
    TransferFeeUnavailable {
        source_domain: u32,
        destination_domain: u32,
        finality_threshold: u32,
    },

    #[error(
        "Transfer fee endpoint unavailable for asset {asset} on route {source_domain}->{destination_domain}: {reason}"
    )]
    TransferFeeEndpointUnavailable {
        asset: CctpTransferAsset,
        source_domain: u32,
        destination_domain: u32,
        reason: String,
    },

    #[error("Timeout waiting for attestation")]
    AttestationTimeout,

    /// Polling for destination-chain receipt confirmation exhausted its
    /// attempt budget. The attestation may already be complete; what
    /// timed out is observing the message as received on the
    /// destination chain.
    #[error("Timeout waiting for destination-chain message receipt")]
    ReceiveTimeout,

    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("RPC error: {0}")]
    Rpc(#[from] alloy_json_rpc::RpcError<alloy_transport::TransportErrorKind>),

    #[error("ABI encoding/decoding error: {0}")]
    Abi(#[from] alloy_sol_types::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Hex conversion error: {0}")]
    Hex(#[from] alloy_primitives::hex::FromHexError),
}

impl CctpError {
    /// Checks if this error indicates that a CCTP message was already relayed.
    ///
    /// This is common in CCTP v2 where third-party relayers may complete transfers
    /// before your application. When this returns `true`, the transfer was successful
    /// (just not by us).
    ///
    /// Detects the explicit [`AlreadyRelayed`](Self::AlreadyRelayed) variant
    /// directly; for [`Rpc`](Self::Rpc) and the transport-error subset of
    /// [`Contract`](Self::Contract), inspects the RPC error payload's message
    /// and data fields against known revert phrases for robustness across
    /// different RPC providers.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// match bridge.mint(message, attestation, from).await {
    ///     Ok(tx_hash) => println!("Minted: {tx_hash}"),
    ///     Err(e) if e.is_already_relayed() => println!("Already relayed by third party"),
    ///     Err(e) => return Err(e),
    /// }
    /// ```
    pub fn is_already_relayed(&self) -> bool {
        match self {
            // Explicit AlreadyRelayed variant
            CctpError::AlreadyRelayed { .. } => true,

            // Check RPC errors for execution revert with known patterns
            CctpError::Rpc(rpc_error) => Self::rpc_error_is_already_relayed(rpc_error),

            // Only `TransportError` carries chain-level revert data — see
            // `alloy_contract::Error::as_revert_data`.
            CctpError::Contract(alloy_contract::Error::TransportError(rpc_error)) => {
                Self::rpc_error_is_already_relayed(rpc_error)
            }

            _ => false,
        }
    }

    /// Checks if an RPC error indicates the message was already relayed.
    fn rpc_error_is_already_relayed(error: &RpcError<TransportErrorKind>) -> bool {
        match error {
            // Check error response payload for revert reasons
            RpcError::ErrorResp(payload) => {
                Self::message_matches_already_relayed(&payload.message)
                    || payload
                        .data
                        .as_ref()
                        .is_some_and(|d| Self::message_matches_already_relayed(&d.to_string()))
            }
            // Local errors may contain the revert reason in their display
            RpcError::LocalUsageError(e) => Self::message_matches_already_relayed(&e.to_string()),
            // Transport or serialization errors don't indicate already relayed
            _ => false,
        }
    }

    /// Checks if a message string matches known "already relayed" patterns.
    fn message_matches_already_relayed(message: &str) -> bool {
        let lower = message.to_lowercase();
        ALREADY_RELAYED_PATTERNS
            .iter()
            .any(|pattern| lower.contains(pattern))
    }

    /// Checks if this is a timeout error.
    ///
    /// Useful for implementing retry logic for transient failures.
    pub fn is_timeout(&self) -> bool {
        if matches!(
            self,
            CctpError::AttestationTimeout | CctpError::ReceiveTimeout
        ) {
            return true;
        }

        // Check if network error indicates timeout
        if let CctpError::Network(e) = self {
            return e.is_timeout();
        }

        false
    }

    /// Checks if this is a rate limiting error.
    ///
    /// Useful for implementing backoff logic when hitting provider rate limits.
    pub fn is_rate_limited(&self) -> bool {
        match self {
            CctpError::Network(e) => e.status().is_some_and(|s| s.as_u16() == 429),
            CctpError::Rpc(RpcError::Transport(TransportErrorKind::HttpError(err))) => {
                err.status == 429
            }
            _ => false,
        }
    }

    /// Checks if this error is transient and the operation could be retried.
    ///
    /// Transient errors include timeouts, rate limiting, and temporary network issues.
    pub fn is_transient(&self) -> bool {
        self.is_timeout() || self.is_rate_limited() || self.is_network_error()
    }

    /// Checks if this is a network-level error.
    fn is_network_error(&self) -> bool {
        matches!(self, CctpError::Network(_))
            || matches!(
                self,
                CctpError::Rpc(RpcError::Transport(
                    TransportErrorKind::BackendGone | TransportErrorKind::HttpError(_)
                ))
            )
    }
}

pub type Result<T> = std::result::Result<T, CctpError>;

#[cfg(test)]
mod tests {
    use super::*;

    fn error_payload(message: &'static str) -> alloy_json_rpc::ErrorPayload {
        alloy_json_rpc::ErrorPayload {
            code: 3,
            message: message.into(),
            data: None,
        }
    }

    #[test]
    fn test_is_already_relayed_explicit_variant() {
        let err = CctpError::AlreadyRelayed {
            original: "test".to_string(),
        };
        assert!(err.is_already_relayed());
    }

    #[test]
    fn test_is_already_relayed_rpc_payload() {
        let check = |message: &'static str| {
            CctpError::Rpc(RpcError::ErrorResp(error_payload(message))).is_already_relayed()
        };

        assert!(check("nonce already used"));
        assert!(check("message already received"));
        assert!(check("Already Received"));
        assert!(check("NONCE ALREADY USED"));
        assert!(check("Nonce Already Used"));
        assert!(!check("some other error"));
    }

    #[test]
    fn test_is_timeout() {
        let err = CctpError::AttestationTimeout;
        assert!(err.is_timeout());

        let err = CctpError::ReceiveTimeout;
        assert!(err.is_timeout());

        let err = CctpError::InvalidConfig(ConfigError::PollingMaxAttemptsZero);
        assert!(!err.is_timeout());
    }

    #[test]
    fn test_receive_timeout_renders_distinct_message() {
        assert_eq!(
            CctpError::ReceiveTimeout.to_string(),
            "Timeout waiting for destination-chain message receipt",
        );
        assert_ne!(
            CctpError::ReceiveTimeout.to_string(),
            CctpError::AttestationTimeout.to_string(),
        );
    }

    #[test]
    fn test_transfer_fee_unavailable_renders_route_context() {
        let err = CctpError::TransferFeeUnavailable {
            source_domain: 0,
            destination_domain: 11,
            finality_threshold: 1000,
        };

        assert_eq!(
            err.to_string(),
            "Transfer fee unavailable for route 0->11 at finality threshold 1000",
        );
    }

    #[test]
    fn test_unrelated_errors_not_already_relayed() {
        assert!(!CctpError::AttestationTimeout.is_already_relayed());
        assert!(
            !CctpError::InvalidConfig(ConfigError::PollingMaxAttemptsZero).is_already_relayed()
        );
        assert!(!CctpError::NotImplemented("test".to_string()).is_already_relayed());
        assert!(!CctpError::TransactionNotFound {
            tx_hash: TxHash::ZERO,
        }
        .is_already_relayed());
        assert!(!CctpError::MessageSentEventMissing {
            tx_hash: TxHash::ZERO,
        }
        .is_already_relayed());
    }

    #[test]
    fn test_contract_variant_routes_through_is_already_relayed() {
        // `ContractNotDeployed` renders without any "already relayed" pattern.
        let err: CctpError = alloy_contract::Error::ContractNotDeployed.into();
        assert!(matches!(err, CctpError::Contract(_)));
        assert!(!err.is_already_relayed());
    }

    #[test]
    fn test_attestation_failure_kind_renders_prose() {
        let render = |kind: AttestationFailureKind| CctpError::AttestationFailed(kind).to_string();

        assert_eq!(
            render(AttestationFailureKind::ApiReportedFailed),
            "Attestation failed: Iris API reported failed status",
        );
        assert_eq!(
            render(AttestationFailureKind::AttestationMissing),
            "Attestation failed: attestation field missing in complete response",
        );
        assert_eq!(
            render(AttestationFailureKind::MessageMissing),
            "Attestation failed: message field missing in complete response",
        );
        assert_eq!(
            render(AttestationFailureKind::AttestationEmpty),
            "Attestation failed: attestation field is empty",
        );
        assert_eq!(
            render(AttestationFailureKind::MessageEmpty),
            "Attestation failed: message field is empty",
        );
        assert_eq!(
            render(AttestationFailureKind::UnexpectedAttestation),
            "Attestation failed: attestation field present in a non-complete response",
        );
        assert_eq!(
            render(AttestationFailureKind::UnexpectedMessage),
            "Attestation failed: message field present in a failed response",
        );
    }

    #[test]
    fn test_config_error_kinds_render_structured_context() {
        let cases = [
            (
                ConfigError::PollingMaxAttemptsZero,
                "PollingConfig.max_attempts must be greater than 0",
            ),
            (
                ConfigError::PollingIntervalZero,
                "PollingConfig.poll_interval_secs must be greater than 0",
            ),
            (
                ConfigError::ProviderTimeoutZero,
                "ProviderConfig.timeout must be greater than zero",
            ),
            (
                ConfigError::ProviderRateLimitZero,
                "ProviderConfig.rate_limit_rps must be greater than zero when set",
            ),
            (
                ConfigError::RouteSameChain {
                    chain: NamedChain::Mainnet,
                },
                "source and destination chains must differ (both were Mainnet)",
            ),
            (
                ConfigError::RouteEnvironmentMismatch {
                    source_chain: NamedChain::Mainnet,
                    destination_chain: NamedChain::BaseSepolia,
                },
                "source and destination chains must both be mainnet or both be testnet (got Mainnet -> BaseSepolia)",
            ),
        ];

        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
            assert_eq!(
                CctpError::from(kind).to_string(),
                format!("Invalid configuration: {expected}")
            );
        }
    }

    #[test]
    fn test_amount_error_kinds_render_structured_context() {
        let cases = [
            (
                AmountError::InvalidDecimal,
                "amount must be an unsigned decimal number".to_string(),
            ),
            (
                AmountError::TooManyDecimalPlaces { actual: 7 },
                "USDC amounts support at most 6 decimal places, got 7".to_string(),
            ),
            (AmountError::TooLarge, "amount is too large".to_string()),
            (
                AmountError::Zero,
                "amount must be greater than zero".to_string(),
            ),
            (
                AmountError::StandardTransferFeeNotLessThanAmount {
                    max_fee: U256::from(1_000_000_u64),
                    amount: U256::from(1_000_000_u64),
                },
                "Standard Transfer max_fee (1000000) must be less than the burn amount (1000000)"
                    .to_string(),
            ),
        ];

        for (kind, expected) in cases {
            assert_eq!(kind.to_string(), expected);
            assert_eq!(
                CctpError::from(kind).to_string(),
                format!("Invalid amount: {expected}")
            );
        }
    }

    #[test]
    fn test_invalid_url_preserves_typed_parse_error() {
        let parse_err = url::Url::parse("not a url").unwrap_err();
        let err: CctpError = parse_err.into();
        assert!(matches!(err, CctpError::InvalidUrl(_)));
    }

    #[test]
    fn test_contract_transport_error_inspects_rpc_payload() {
        let contract_err_with_message = |message: &'static str| -> CctpError {
            alloy_contract::Error::TransportError(alloy_transport::TransportError::ErrorResp(
                error_payload(message),
            ))
            .into()
        };

        assert!(
            contract_err_with_message("execution reverted: nonce already used")
                .is_already_relayed()
        );
        assert!(
            !contract_err_with_message("execution reverted: insufficient allowance")
                .is_already_relayed()
        );
    }
}
