// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

use alloy_chains::NamedChain;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{CctpError, Result};

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

/// Configuration for attestation polling behavior.
///
/// Controls how the bridge polls Circle's Iris API for attestation availability.
/// Use the builder methods to customize, or use preset configurations for common scenarios.
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
///     .with_max_attempts(20)
///     .with_poll_interval_secs(30);
///
/// // Use preset for fast transfers (30 attempts, 5 second intervals)
/// let config = PollingConfig::fast_transfer();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollingConfig {
    /// Maximum number of polling attempts before giving up.
    pub max_attempts: u32,
    /// Seconds to wait between polling attempts.
    pub poll_interval_secs: u64,
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
        Self {
            max_attempts: 30,
            poll_interval_secs: 60,
        }
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
        Self {
            max_attempts: 30,
            poll_interval_secs: 5,
        }
    }

    /// Creates a polling configuration after checking that both fields are
    /// usable for polling.
    ///
    /// Rejects `max_attempts = 0` (which would time out immediately) and
    /// `poll_interval_secs = 0` (which would spin a tight retry loop).
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::InvalidConfig`] when either input is zero.
    ///
    /// # Example
    ///
    /// ```rust
    /// use cctp_rs::PollingConfig;
    ///
    /// let config = PollingConfig::try_new(20, 30).unwrap();
    /// assert_eq!(config.max_attempts, 20);
    /// assert_eq!(config.poll_interval_secs, 30);
    ///
    /// assert!(PollingConfig::try_new(0, 30).is_err());
    /// assert!(PollingConfig::try_new(20, 0).is_err());
    /// ```
    pub fn try_new(max_attempts: u32, poll_interval_secs: u64) -> Result<Self> {
        let config = Self {
            max_attempts,
            poll_interval_secs,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration for use in polling loops.
    ///
    /// Builder methods like [`Self::with_max_attempts`] and
    /// [`Self::with_poll_interval_secs`] do not check their inputs, so callers
    /// that accept the values at runtime should call this before using the
    /// config to poll Circle's Iris API.
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::InvalidConfig`] when `max_attempts` is zero
    /// (the polling loop would exit before its first request) or
    /// `poll_interval_secs` is zero (the loop would never sleep between
    /// retries).
    pub fn validate(&self) -> Result<()> {
        if self.max_attempts == 0 {
            return Err(CctpError::InvalidConfig(
                "PollingConfig.max_attempts must be greater than 0".to_string(),
            ));
        }
        if self.poll_interval_secs == 0 {
            return Err(CctpError::InvalidConfig(
                "PollingConfig.poll_interval_secs must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    /// Sets the maximum number of polling attempts.
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
    /// let config = PollingConfig::default().with_max_attempts(60);
    /// assert_eq!(config.max_attempts, 60);
    /// ```
    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Sets the interval between polling attempts in seconds.
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
    /// let config = PollingConfig::default().with_poll_interval_secs(30);
    /// assert_eq!(config.poll_interval_secs, 30);
    /// ```
    pub fn with_poll_interval_secs(mut self, secs: u64) -> Self {
        self.poll_interval_secs = secs;
        self
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
        u64::from(self.max_attempts).saturating_mul(self.poll_interval_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PollingConfig::default();
        assert_eq!(config.max_attempts, 30);
        assert_eq!(config.poll_interval_secs, 60);
        assert_eq!(config.total_timeout_secs(), 1800); // 30 minutes
    }

    #[test]
    fn test_fast_transfer_config() {
        let config = PollingConfig::fast_transfer();
        assert_eq!(config.max_attempts, 30);
        assert_eq!(config.poll_interval_secs, 5);
        assert_eq!(config.total_timeout_secs(), 150); // 2.5 minutes
    }

    #[test]
    fn test_builder_methods() {
        let config = PollingConfig::default()
            .with_max_attempts(20)
            .with_poll_interval_secs(30);
        assert_eq!(config.max_attempts, 20);
        assert_eq!(config.poll_interval_secs, 30);
        assert_eq!(config.total_timeout_secs(), 600); // 10 minutes
    }

    #[test]
    fn test_config_is_copy() {
        let config = PollingConfig::default();
        let copied = config;
        assert_eq!(config, copied);
    }

    #[test]
    fn test_validate_accepts_defaults() {
        assert!(PollingConfig::default().validate().is_ok());
        assert!(PollingConfig::fast_transfer().validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_zero_max_attempts() {
        let config = PollingConfig::default().with_max_attempts(0);
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, CctpError::InvalidConfig(ref msg) if msg.contains("max_attempts")),
            "expected InvalidConfig mentioning max_attempts, got {err:?}"
        );
    }

    #[test]
    fn test_validate_rejects_zero_poll_interval() {
        let config = PollingConfig::default().with_poll_interval_secs(0);
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, CctpError::InvalidConfig(ref msg) if msg.contains("poll_interval_secs")),
            "expected InvalidConfig mentioning poll_interval_secs, got {err:?}"
        );
    }

    #[test]
    fn test_try_new_accepts_positive_values() {
        let config = PollingConfig::try_new(20, 30).unwrap();
        assert_eq!(config.max_attempts, 20);
        assert_eq!(config.poll_interval_secs, 30);
    }

    #[test]
    fn test_try_new_rejects_zero_inputs() {
        assert!(PollingConfig::try_new(0, 30).is_err());
        assert!(PollingConfig::try_new(20, 0).is_err());
        assert!(PollingConfig::try_new(0, 0).is_err());
    }

    #[test]
    fn test_total_timeout_saturates_on_overflow() {
        let config = PollingConfig {
            max_attempts: u32::MAX,
            poll_interval_secs: u64::MAX,
        };
        // u32::MAX * u64::MAX would overflow; saturating arithmetic clamps it.
        assert_eq!(config.total_timeout_secs(), u64::MAX);
    }

    #[test]
    fn test_total_timeout_with_large_in_range_values() {
        // 1_000_000 attempts * 3600 secs = 3.6e9, fits comfortably in u64.
        let config = PollingConfig {
            max_attempts: 1_000_000,
            poll_interval_secs: 3600,
        };
        assert_eq!(config.total_timeout_secs(), 3_600_000_000);
    }
}
