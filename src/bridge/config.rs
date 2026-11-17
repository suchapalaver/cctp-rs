// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use std::num::{NonZeroU32, NonZeroU64};

use alloy_chains::NamedChain;
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::error::{ConfigError, Result};

/// Circle Iris API environment URLs
///
/// See <https://developers.circle.com/stablecoins/cctp-apis>
///
pub const IRIS_API: &str = "https://iris-api.circle.com";
pub const IRIS_API_SANDBOX: &str = "https://iris-api-sandbox.circle.com";

/// Returns the Iris API base URL for the given chain.
///
/// Selects [`IRIS_API_SANDBOX`] when `chain` is a testnet and [`IRIS_API`]
/// otherwise.
pub fn iris_api_url(chain: NamedChain) -> Url {
    if chain.is_testnet() {
        Url::parse(IRIS_API_SANDBOX).unwrap()
    } else {
        Url::parse(IRIS_API).unwrap()
    }
}

/// CCTP v1 attestation API path
pub const ATTESTATION_PATH_V1: &str = "/v1/attestations/";

/// CCTP v2 messages API path
///
/// V2 uses a different endpoint format than v1:
/// - V1: `/v1/attestations/{messageHash}`
/// - V2: `/v2/messages/{sourceDomain}?transactionHash={txHash}`
pub const MESSAGES_PATH_V2: &str = "/v2/messages/";

/// CCTP v2 transfer fees API path prefix
///
/// V2 route fees are exposed as:
/// `/v2/burn/{asset}/fees/{sourceDomainId}/{destDomainId}`
pub const TRANSFER_FEES_PATH_V2_PREFIX: &str = "/v2/burn/";

/// CCTP v2 Fast Transfer allowance API path.
///
/// The allowance is a global USDC capacity pool per Iris environment:
/// `/v2/fastBurn/USDC/allowance`
pub const FAST_TRANSFER_ALLOWANCE_PATH_V2: &str = "/v2/fastBurn/USDC/allowance";

/// Configuration for attestation polling behavior.
///
/// Controls how the bridge polls Circle's Iris API for attestation availability.
/// Use [`Self::try_new`] or the fallible builder methods to customize, or use
/// preset configurations for common scenarios. Attempt counts and polling
/// intervals are nonzero by construction, including after deserialization.
///
/// # Examples
///
/// ```rust
/// use cctp_rs::PollingConfig;
///
/// // Use defaults (30 attempts, 60 second intervals)
/// let config = PollingConfig::default();
///
/// // Customize polling behavior
/// let config = PollingConfig::default()
///     .try_with_max_attempts(20).unwrap()
///     .try_with_poll_interval_secs(30).unwrap();
///
/// // Use preset for fast transfers (30 attempts, 5 second intervals)
/// let config = PollingConfig::fast_transfer();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PollingConfig {
    /// Maximum number of polling attempts before giving up.
    max_attempts: NonZeroU32,
    /// Seconds to wait between polling attempts.
    poll_interval_secs: NonZeroU64,
}

#[derive(Deserialize)]
struct PollingConfigWire {
    max_attempts: u32,
    poll_interval_secs: u64,
}

impl<'de> Deserialize<'de> for PollingConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PollingConfigWire::deserialize(deserializer)?;
        Self::try_new(wire.max_attempts, wire.poll_interval_secs).map_err(serde::de::Error::custom)
    }
}

impl Default for PollingConfig {
    /// Creates a default polling configuration suitable for standard CCTP v1 transfers.
    ///
    /// - `max_attempts`: 30
    /// - `poll_interval_secs`: 60 (1 minute)
    ///
    /// This results in a maximum wait time of ~30 minutes, which accommodates
    /// the typical 13-19 minute attestation time for v1 transfers.
    fn default() -> Self {
        Self::try_new(30, 60).expect("default polling values are nonzero")
    }
}

impl PollingConfig {
    /// Creates a polling configuration optimized for CCTP v2 fast transfers.
    ///
    /// - `max_attempts`: 30
    /// - `poll_interval_secs`: 5
    ///
    /// Fast transfers typically complete in under 30 seconds, so this configuration
    /// polls more frequently with shorter intervals.
    pub fn fast_transfer() -> Self {
        Self::try_new(30, 5).expect("fast-transfer polling values are nonzero")
    }

    /// Creates a polling configuration after checking that both fields are
    /// usable for polling.
    ///
    /// Rejects `max_attempts = 0` (which would time out immediately) and
    /// `poll_interval_secs = 0` (which would spin a tight retry loop).
    ///
    /// # Errors
    ///
    /// Returns [`crate::CctpError::InvalidConfig`] when either input is zero.
    ///
    /// # Example
    ///
    /// ```rust
    /// use cctp_rs::PollingConfig;
    ///
    /// let config = PollingConfig::try_new(20, 30).unwrap();
    /// assert_eq!(config.max_attempts(), 20);
    /// assert_eq!(config.poll_interval_secs(), 30);
    ///
    /// assert!(PollingConfig::try_new(0, 30).is_err());
    /// assert!(PollingConfig::try_new(20, 0).is_err());
    /// ```
    pub fn try_new(max_attempts: u32, poll_interval_secs: u64) -> Result<Self> {
        let max_attempts =
            NonZeroU32::new(max_attempts).ok_or(ConfigError::PollingMaxAttemptsZero)?;
        let poll_interval_secs =
            NonZeroU64::new(poll_interval_secs).ok_or(ConfigError::PollingIntervalZero)?;

        Ok(Self {
            max_attempts,
            poll_interval_secs,
        })
    }

    /// Returns the maximum number of polling attempts before giving up.
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts.get()
    }

    /// Returns the number of seconds between polling attempts.
    pub const fn poll_interval_secs(&self) -> u64 {
        self.poll_interval_secs.get()
    }

    /// Sets the maximum number of polling attempts after validating it.
    ///
    /// # Arguments
    ///
    /// * `attempts` - Maximum number of times to poll the attestation API
    ///
    /// # Example
    ///
    /// ```rust
    /// use cctp_rs::PollingConfig;
    ///
    /// let config = PollingConfig::default().try_with_max_attempts(60).unwrap();
    /// assert_eq!(config.max_attempts(), 60);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`crate::CctpError::InvalidConfig`] when `attempts` is zero.
    pub fn try_with_max_attempts(self, attempts: u32) -> Result<Self> {
        Self::try_new(attempts, self.poll_interval_secs())
    }

    /// Sets the interval between polling attempts in seconds after validating it.
    ///
    /// # Arguments
    ///
    /// * `secs` - Seconds to wait between each polling attempt
    ///
    /// # Example
    ///
    /// ```rust
    /// use cctp_rs::PollingConfig;
    ///
    /// let config = PollingConfig::default()
    ///     .try_with_poll_interval_secs(30)
    ///     .unwrap();
    /// assert_eq!(config.poll_interval_secs(), 30);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`crate::CctpError::InvalidConfig`] when `secs` is zero.
    pub fn try_with_poll_interval_secs(self, secs: u64) -> Result<Self> {
        Self::try_new(self.max_attempts(), secs)
    }

    /// Returns the total maximum wait time in seconds.
    ///
    /// Computed as `max_attempts * poll_interval_secs` with saturating
    /// arithmetic, so pathological large values clamp to [`u64::MAX`] instead
    /// of overflowing.
    ///
    /// # Example
    ///
    /// ```rust
    /// use cctp_rs::PollingConfig;
    ///
    /// let config = PollingConfig::default();
    /// assert_eq!(config.total_timeout_secs(), 30 * 60); // 30 minutes
    /// ```
    pub fn total_timeout_secs(&self) -> u64 {
        u64::from(self.max_attempts()).saturating_mul(self.poll_interval_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CctpError;

    #[test]
    fn test_default_config() {
        let config = PollingConfig::default();
        assert_eq!(config.max_attempts(), 30);
        assert_eq!(config.poll_interval_secs(), 60);
        assert_eq!(config.total_timeout_secs(), 1800); // 30 minutes
    }

    #[test]
    fn test_fast_transfer_config() {
        let config = PollingConfig::fast_transfer();
        assert_eq!(config.max_attempts(), 30);
        assert_eq!(config.poll_interval_secs(), 5);
        assert_eq!(config.total_timeout_secs(), 150); // 2.5 minutes
    }

    #[test]
    fn test_builder_methods() {
        let config = PollingConfig::default()
            .try_with_max_attempts(20)
            .unwrap()
            .try_with_poll_interval_secs(30)
            .unwrap();
        assert_eq!(config.max_attempts(), 20);
        assert_eq!(config.poll_interval_secs(), 30);
        assert_eq!(config.total_timeout_secs(), 600); // 10 minutes
    }

    #[test]
    fn test_config_is_copy() {
        let config = PollingConfig::default();
        let copied = config;
        assert_eq!(config, copied);
    }

    #[test]
    fn test_builder_rejects_zero_max_attempts() {
        let err = PollingConfig::default()
            .try_with_max_attempts(0)
            .unwrap_err();
        assert!(
            matches!(
                err,
                CctpError::InvalidConfig(ConfigError::PollingMaxAttemptsZero)
            ),
            "expected typed zero-attempt error, got {err:?}"
        );
    }

    #[test]
    fn test_builder_rejects_zero_poll_interval() {
        let err = PollingConfig::default()
            .try_with_poll_interval_secs(0)
            .unwrap_err();
        assert!(
            matches!(
                err,
                CctpError::InvalidConfig(ConfigError::PollingIntervalZero)
            ),
            "expected typed zero-interval error, got {err:?}"
        );
    }

    #[test]
    fn test_try_new_accepts_positive_values() {
        let config = PollingConfig::try_new(20, 30).unwrap();
        assert_eq!(config.max_attempts(), 20);
        assert_eq!(config.poll_interval_secs(), 30);
    }

    #[test]
    fn test_try_new_rejects_zero_inputs() {
        assert!(matches!(
            PollingConfig::try_new(0, 30),
            Err(CctpError::InvalidConfig(
                ConfigError::PollingMaxAttemptsZero
            ))
        ));
        assert!(matches!(
            PollingConfig::try_new(20, 0),
            Err(CctpError::InvalidConfig(ConfigError::PollingIntervalZero))
        ));
        assert!(matches!(
            PollingConfig::try_new(0, 0),
            Err(CctpError::InvalidConfig(
                ConfigError::PollingMaxAttemptsZero
            ))
        ));
    }

    #[test]
    fn test_total_timeout_saturates_on_overflow() {
        let config = PollingConfig::try_new(u32::MAX, u64::MAX).unwrap();
        // u32::MAX * u64::MAX would overflow; saturating arithmetic clamps it.
        assert_eq!(config.total_timeout_secs(), u64::MAX);
    }

    #[test]
    fn test_total_timeout_with_large_in_range_values() {
        // 1_000_000 attempts * 3600 secs = 3.6e9, fits comfortably in u64.
        let config = PollingConfig::try_new(1_000_000, 3600).unwrap();
        assert_eq!(config.total_timeout_secs(), 3_600_000_000);
    }

    #[test]
    fn test_serde_round_trip_preserves_wire_shape() {
        let config = PollingConfig::try_new(20, 30).unwrap();
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(json, r#"{"max_attempts":20,"poll_interval_secs":30}"#);
        assert_eq!(
            serde_json::from_str::<PollingConfig>(&json).unwrap(),
            config
        );
    }

    #[test]
    fn test_deserialization_rejects_zero_fields() {
        let zero_attempts =
            serde_json::from_str::<PollingConfig>(r#"{"max_attempts":0,"poll_interval_secs":30}"#)
                .unwrap_err();
        assert!(zero_attempts.to_string().contains("max_attempts"));

        let zero_interval =
            serde_json::from_str::<PollingConfig>(r#"{"max_attempts":20,"poll_interval_secs":0}"#)
                .unwrap_err();
        assert!(zero_interval.to_string().contains("poll_interval_secs"));
    }
}
