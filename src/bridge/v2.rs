// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

use crate::error::{AttestationFailureKind, CctpError, Result};
use crate::protocol::{AttestationBytes, FinalityThreshold, TransferFee};
use crate::{
    spans, AttestationStatus, CctpV2 as CctpV2Trait, CctpV2Route, DomainId, V2AttestationResponse,
    V2Message,
};
use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{hex, Address, Bytes, FixedBytes, TxHash, U256};
use alloy_provider::Provider;
use alloy_sol_types::SolEvent;
use async_trait::async_trait;
use bon::{bon, Builder};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, Instrument};
use url::Url;

use super::transfer_mode::TransferMode;

/// Result of attempting to mint on the destination chain
///
/// CCTP v2 is permissionless - anyone can relay a message once Circle's attestation
/// is available. Third-party relayers actively monitor for burns and may complete
/// transfers before your application does. This enum represents both outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "tx_hash", rename_all = "snake_case")]
pub enum MintResult {
    /// We successfully minted (includes the transaction hash)
    Minted(TxHash),
    /// Already relayed by a third party (transfer was successful)
    AlreadyRelayed,
}

use super::bridge_trait::CctpBridge;
use super::config::{iris_api_url, PollingConfig, MESSAGES_PATH_V2, TRANSFER_FEES_PATH_V2};
use crate::contracts::erc20::Erc20Contract;
use crate::contracts::message_transmitter::MessageTransmitter::MessageSent;
use crate::contracts::v2::{MessageTransmitterV2Contract, TokenMessengerV2Contract};

/// CCTP v2 bridge implementation
///
/// This struct provides the core functionality for bridging USDC across chains
/// using Circle's Cross-Chain Transfer Protocol v2 with support for Fast Transfer,
/// programmable hooks, and expanded network coverage.
///
/// # V2 Features
///
/// - **Fast Transfer**: Sub-30 second settlement times (vs 13-19 minutes in v1)
/// - **Programmable Hooks**: Execute custom logic post-transfer (swaps, lending, etc.)
/// - **Expanded Networks**: 11 v2-capable EVM chain families supported
///   (the 7 v1 chain families plus Linea, Sonic, Sei, HyperEVM) with their
///   testnets. The protocol parser recognizes all 21 announced CCTP v2
///   domain IDs (including non-EVM domains such as Solana and Starknet
///   Testnet) but bridging requires `NamedChain::supports_cctp_v2()`.
/// - **Unified Addresses**: Same contract addresses across all chains in each environment
///
/// # Examples
///
/// ```rust,no_run
/// # use cctp_rs::{CctpV2Bridge, CctpV2Route, TransferMode};
/// # use alloy_chains::NamedChain;
/// # use alloy_provider::ProviderBuilder;
/// # use alloy_primitives::{Address, U256, Bytes};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Route-first construction keeps source/destination chains tied to
/// // `CctpV2Route` validation.
/// let provider = ProviderBuilder::new().connect("http://localhost:8545").await?;
/// let route = CctpV2Route::new(NamedChain::Mainnet, NamedChain::Linea)?;
/// let bridge = CctpV2Bridge::from_route(route)
///     .source_provider(provider.clone())
///     .destination_provider(provider)
///     .recipient("0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?)
///     .build();
///
/// // Fast transfer with hooks
/// let provider2 = ProviderBuilder::new().connect("http://localhost:8545").await?;
/// let fast_bridge = CctpV2Bridge::builder()
///     .source_chain(NamedChain::Mainnet)
///     .destination_chain(NamedChain::Linea)
///     .source_provider(provider2.clone())
///     .destination_provider(provider2)
///     .recipient("0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?)
///     .transfer_mode(TransferMode::FastWithHook {
///         max_fee: U256::from(100),
///         hook_data: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
///     })
///     .build();
/// # Ok(())
/// # }
/// ```
///
/// # Transfer mode selection
///
/// The four valid CCTP v2 configurations are expressed via [`TransferMode`]:
///
/// | Mode | Finality | Hook | On-chain call |
/// |---|---|---|---|
/// | [`Standard`](TransferMode::Standard) | 2000 (finalized) | none | `depositForBurn` |
/// | [`Fast`](TransferMode::Fast) | 1000 (confirmed) | none | `depositForBurn` |
/// | [`StandardWithHook`](TransferMode::StandardWithHook) | 2000 | yes | `depositForBurnWithHook` |
/// | [`FastWithHook`](TransferMode::FastWithHook) | 1000 | yes | `depositForBurnWithHook` |
///
/// Earlier versions exposed `fast_transfer`, `hook_data`, and `max_fee` as
/// independent builder fields. That shape allowed contradictory configs
/// (e.g. fast + hook) that the bridge silently resolved to standard
/// finality while still reporting fast — see [issue
/// 218](https://github.com/semiotic-ai/cctp-rs/issues/218). The enum
/// removes the precedence question by construction.
#[derive(Builder, Clone, Debug)]
pub struct CctpV2<P: Provider<Ethereum> + Clone> {
    source_provider: P,
    destination_provider: P,
    source_chain: NamedChain,
    destination_chain: NamedChain,
    recipient: Address,

    /// Selects which v2 burn variant the bridge sends. Defaults to
    /// [`TransferMode::Standard`].
    #[builder(default)]
    transfer_mode: TransferMode,

    /// Override the Iris API base URL. Primarily intended for pointing the
    /// bridge at a local mock server (e.g. `wiremock`) in tests, or at a
    /// custom Iris-compatible endpoint. When `None`, the URL is selected
    /// from the source chain's mainnet/testnet status.
    api_url_override: Option<Url>,
}

#[bon]
impl<P: Provider<Ethereum> + Clone> CctpV2<P> {
    /// Creates a CCTP v2 bridge builder from a validated route.
    ///
    /// This is the route-first alternative to [`CctpV2::builder`]. The
    /// source and destination chains are populated from [`CctpV2Route`], so
    /// route validation remains centralized in the route constructor while
    /// callers can still configure providers, recipient, transfer mode, and
    /// an optional Iris API URL override.
    #[builder(finish_fn = build)]
    pub fn from_route(
        #[builder(start_fn)] route: CctpV2Route,
        source_provider: P,
        destination_provider: P,
        recipient: Address,
        #[builder(default)] transfer_mode: TransferMode,
        api_url_override: Option<Url>,
    ) -> Self {
        Self {
            source_provider,
            destination_provider,
            source_chain: route.source_chain(),
            destination_chain: route.destination_chain(),
            recipient,
            transfer_mode,
            api_url_override,
        }
    }
}

impl<P: Provider<Ethereum> + Clone> CctpV2<P> {
    /// Returns the CCTP v2 API URL for the current environment
    pub fn api_url(&self) -> Url {
        if let Some(url) = &self.api_url_override {
            return url.clone();
        }
        iris_api_url(self.source_chain)
    }

    /// Returns the source chain
    pub fn source_chain(&self) -> &NamedChain {
        &self.source_chain
    }

    /// Returns the destination chain
    pub fn destination_chain(&self) -> &NamedChain {
        &self.destination_chain
    }

    /// Returns the destination domain id
    pub fn destination_domain_id(&self) -> Result<DomainId> {
        self.destination_chain.cctp_v2_domain_id()
    }

    /// Returns the source provider
    pub fn source_provider(&self) -> &P {
        &self.source_provider
    }

    /// Returns the destination provider
    pub fn destination_provider(&self) -> &P {
        &self.destination_provider
    }

    /// Returns the CCTP v2 token messenger contract address
    pub fn token_messenger_v2_contract(&self) -> Result<Address> {
        self.source_chain.token_messenger_v2_address()
    }

    /// Returns the CCTP v2 message transmitter contract address
    pub fn message_transmitter_v2_contract(&self) -> Result<Address> {
        self.destination_chain.message_transmitter_v2_address()
    }

    /// Returns the recipient address
    pub fn recipient(&self) -> &Address {
        &self.recipient
    }

    /// Returns the configured transfer mode.
    pub fn transfer_mode(&self) -> &TransferMode {
        &self.transfer_mode
    }

    /// Returns whether fast transfer is enabled (derived from
    /// [`Self::transfer_mode`]).
    pub fn is_fast_transfer(&self) -> bool {
        self.transfer_mode.is_fast()
    }

    /// Returns the hook data if the configured mode carries any.
    pub fn hook_data(&self) -> Option<&Bytes> {
        self.transfer_mode.hook_data()
    }

    /// Returns the fast-transfer fee cap, or `None` when the mode is not a
    /// fast variant. Standard modes implicitly use a zero `maxFee` on-chain.
    pub fn max_fee(&self) -> Option<U256> {
        if self.transfer_mode.is_fast() {
            Some(self.transfer_mode.max_fee())
        } else {
            None
        }
    }

    /// Returns the finality threshold for the configured mode.
    ///
    /// Always agrees with the `min_finality_threshold` the bridge will send
    /// on-chain — both are derived from the same [`TransferMode`].
    pub fn finality_threshold(&self) -> FinalityThreshold {
        self.transfer_mode.finality_threshold()
    }

    /// Constructs the Iris API v2 URL for live route fee lookup.
    ///
    /// The fee API is route-aware:
    /// `/v2/burn/USDC/fees/{sourceDomainId}/{destDomainId}`.
    ///
    /// # Errors
    ///
    /// Returns `CctpError::UnsupportedChain` when either configured chain has no
    /// CCTP v2 domain, or `CctpError::InvalidUrl` if URL construction fails.
    pub fn create_transfer_fees_url(&self) -> Result<Url> {
        let (source_domain, destination_domain) = self.transfer_fee_domain_ids()?;
        Ok(self.api_url().join(&format!(
            "{TRANSFER_FEES_PATH_V2}{source_domain}/{destination_domain}"
        ))?)
    }

    /// Fetches all live transfer fee entries for this bridge's source and
    /// destination domain route.
    ///
    /// This is the authoritative SDK path for current CCTP v2 route fees.
    /// The chain-level [`CctpV2::fast_transfer_fee_bps`](crate::CctpV2::fast_transfer_fee_bps)
    /// helper reports static metadata only and currently returns
    /// [`FastTransferFee::Unknown`](crate::FastTransferFee::Unknown) for every
    /// supported chain.
    ///
    /// Circle returns fees in basis points. Use [`Self::get_fast_transfer_fee`]
    /// or [`Self::calculate_fast_transfer_max_fee`] when preparing the `maxFee`
    /// argument for a fast transfer.
    ///
    /// # Errors
    ///
    /// Returns an error when the route is unsupported, the Iris request fails,
    /// or the JSON response cannot be decoded.
    pub async fn get_transfer_fees(&self) -> Result<Vec<TransferFee>> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(CctpError::Network)?;
        let url = self.create_transfer_fees_url()?;

        info!(
            url = %url,
            source_chain = %self.source_chain,
            destination_chain = %self.destination_chain,
            event = "transfer_fee_request_started"
        );

        let response = self.fetch_transfer_fees_response(&client, &url).await?;
        response.error_for_status_ref()?;
        let response_text = response.text().await?;
        let fees: Vec<TransferFee> = serde_json::from_str(&response_text)?;

        info!(
            fee_count = fees.len(),
            event = "transfer_fee_request_complete"
        );

        Ok(fees)
    }

    /// Fetches the live fee entry for the requested finality threshold.
    ///
    /// Returns `Ok(None)` when Iris returns no entry for the threshold.
    pub async fn get_transfer_fee(
        &self,
        finality_threshold: FinalityThreshold,
    ) -> Result<Option<TransferFee>> {
        let fees = self.get_transfer_fees().await?;
        Ok(fees
            .into_iter()
            .find(|fee| fee.finality_threshold == finality_threshold.as_u32()))
    }

    /// Fetches the live Fast Transfer fee for this route, if Iris returns one.
    pub async fn get_fast_transfer_fee(&self) -> Result<Option<TransferFee>> {
        self.get_transfer_fee(FinalityThreshold::Fast).await
    }

    /// Fetches the live Standard Transfer fee for this route, if Iris returns one.
    pub async fn get_standard_transfer_fee(&self) -> Result<Option<TransferFee>> {
        self.get_transfer_fee(FinalityThreshold::Standard).await
    }

    /// Fetches the live fast-transfer fee and calculates a buffered `maxFee`.
    ///
    /// `amount` is denominated in USDC atomic units. `buffer_percent = 20`
    /// returns a cap 20% above the current protocol fee.
    /// Use this immediately before constructing a fast-transfer
    /// [`TransferMode`] so the on-chain `maxFee` reflects the current route fee
    /// plus your selected buffer.
    ///
    /// # Errors
    ///
    /// Returns [`CctpError::TransferFeeUnavailable`] when Iris does not return a
    /// Fast Transfer fee for the configured route.
    pub async fn calculate_fast_transfer_max_fee(
        &self,
        amount: U256,
        buffer_percent: u32,
    ) -> Result<U256> {
        let Some(fee) = self.get_fast_transfer_fee().await? else {
            let (source_domain, destination_domain) = self.transfer_fee_domain_ids()?;
            return Err(CctpError::TransferFeeUnavailable {
                source_domain,
                destination_domain,
                finality_threshold: FinalityThreshold::Fast.as_u32(),
            });
        };

        Ok(fee.max_fee_with_buffer_percent(amount, buffer_percent))
    }

    /// Gets the `MessageSent` event data from a CCTP v2 bridge transaction
    ///
    /// **⚠️ WARNING**: For v2 transfers, the message extracted from transaction logs contains
    /// zeros in the nonce field (bytes 12-44). Circle's attestation service fills in the actual
    /// nonce before signing. If you need to mint tokens, use [`Self::get_attestation`] instead,
    /// which returns both the canonical message (with correct nonce) and attestation from Circle's API.
    ///
    /// This function is useful for:
    /// - Computing the message hash for tracking/monitoring purposes
    /// - Debugging and inspecting the message structure
    ///
    /// For actual token minting, use [`Self::get_attestation`] to get the correct message.
    ///
    /// # Arguments
    ///
    /// * `tx_hash`: The hash of the transaction to get the `MessageSent` event for
    ///
    /// # Returns
    ///
    /// Returns the message bytes (with zeros for nonce) and its hash
    pub async fn get_message_sent_event(
        &self,
        tx_hash: TxHash,
    ) -> Result<(Vec<u8>, FixedBytes<32>)> {
        let span =
            spans::get_message_sent_event(tx_hash, &self.source_chain, &self.destination_chain);

        async move {
            let tx_receipt = match self.source_provider.get_transaction_receipt(tx_hash).await {
                Ok(receipt) => receipt,
                Err(e) => {
                    spans::record_error_with_context(
                        "ReceiptRetrievalFailed",
                        &format!("Failed to get transaction receipt: {e}"),
                        Some("RPC call to get_transaction_receipt failed"),
                    );
                    error!(
                        error = %e,
                        event = "transaction_receipt_retrieval_failed"
                    );
                    return Err(e.into());
                }
            };

            if let Some(tx_receipt) = tx_receipt {
                // Calculate the event topic by hashing the event signature
                let message_sent_topic = alloy_primitives::keccak256(b"MessageSent(bytes)");

                let message_sent_log = tx_receipt
                    .inner
                    .logs()
                    .iter()
                    .find(|log| {
                        log.topics()
                            .first()
                            .is_some_and(|topic| topic.as_slice() == message_sent_topic)
                    })
                    .ok_or_else(|| {
                        spans::record_error_with_context(
                            "MessageSentEventNotFound",
                            "MessageSent event not found in transaction logs",
                            Some(&format!(
                                "Transaction contained {} logs but none matched MessageSent signature",
                                tx_receipt.inner.logs().len()
                            )),
                        );
                        error!(
                            available_logs = tx_receipt.inner.logs().len(),
                            event = "message_sent_event_not_found"
                        );
                        CctpError::MessageSentEventMissing { tx_hash }
                    })?;

                // Decode the log data using the generated event bindings
                let decoded = MessageSent::abi_decode_data(&message_sent_log.data().data)?;

                let message_sent_event = decoded.0.to_vec();
                let message_hash = alloy_primitives::keccak256(&message_sent_event);

                info!(
                    message_hash = %hex::encode(message_hash),
                    message_length_bytes = message_sent_event.len(),
                    version = "v2",
                    fast_transfer = self.transfer_mode.is_fast(),
                    has_hooks = self.transfer_mode.has_hook(),
                    event = "message_sent_event_extracted"
                );

                Ok((message_sent_event, message_hash))
            } else {
                spans::record_error_with_context(
                    "TransactionNotFound",
                    "Transaction receipt not found",
                    Some("The transaction may not have been mined yet or the RPC node doesn't have it"),
                );
                error!(event = "transaction_not_found");
                Err(CctpError::TransactionNotFound { tx_hash })
            }
        }
        .instrument(span)
        .await
    }

    /// Gets the attestation and canonical message for a transaction from Circle's Iris API (v2)
    ///
    /// This method polls the Iris API until the attestation is ready or times out.
    /// Unlike CCTP v1 which uses message hashes, v2 uses the transaction hash directly.
    /// The source domain is automatically derived from the bridge's configured source chain.
    ///
    /// **Important**: For v2 transfers, the `MessageSent` event log contains a "template" message
    /// with zeros in the nonce field. Circle's attestation service fills in the actual nonce
    /// before signing. This method returns the canonical message from Circle's API with the
    /// correct nonce, which you MUST use for minting.
    ///
    /// # Arguments
    ///
    /// * `tx_hash` - The hash of the burn transaction on the source chain
    /// * `polling_config` - Configuration for polling behavior (attempts, intervals).
    ///   Use `PollingConfig::fast_transfer()` for fast transfers or `PollingConfig::default()` for standard.
    ///
    /// # Returns
    ///
    /// A tuple of `(message_bytes, attestation_bytes)` where:
    /// - `message_bytes`: The canonical message from Circle's API (with nonce filled in)
    /// - `attestation_bytes`: The signed attestation to submit to the destination chain
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The attestation request fails
    /// - Circle's API returns a failed status
    /// - The maximum number of attempts is reached (timeout)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use cctp_rs::PollingConfig;
    ///
    /// // Get attestation for a burn transaction with fast transfer polling
    /// let (message, attestation) = bridge.get_attestation(
    ///     burn_tx_hash,
    ///     PollingConfig::fast_transfer(),
    /// ).await?;
    ///
    /// // Or with custom retry settings
    /// let (message, attestation) = bridge.get_attestation(
    ///     burn_tx_hash,
    ///     PollingConfig::default()
    ///         .with_max_attempts(60)
    ///         .with_poll_interval_secs(10),
    /// ).await?;
    ///
    /// // Use the returned message (NOT from get_message_sent_event) for minting
    /// let mint_tx = bridge.mint(message, attestation, recipient).await?;
    /// ```
    pub async fn get_attestation(
        &self,
        tx_hash: TxHash,
        polling_config: PollingConfig,
    ) -> Result<(Vec<u8>, AttestationBytes)> {
        polling_config.validate()?;
        let max_attempts = polling_config.max_attempts;
        let poll_interval = polling_config.poll_interval_secs;
        let total_timeout_secs = polling_config.total_timeout_secs();

        let span = spans::get_v2_attestation_with_retry(
            tx_hash,
            &self.source_chain,
            &self.destination_chain,
            max_attempts,
            poll_interval,
        );

        async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(CctpError::Network)?;
            let url = self.create_url(tx_hash)?;

            info!(
                url = %url,
                tx_hash = %tx_hash,
                version = "v2",
                fast_transfer = self.transfer_mode.is_fast(),
                finality_threshold = %self.finality_threshold(),
                event = "attestation_polling_started"
            );

            for attempt in 1..=max_attempts {
                let attempt_span = spans::get_attestation(&url, attempt);
                let attempt_result: Result<Option<(Vec<u8>, AttestationBytes)>> = async {
                    let response = match self.fetch_attestation_response(&client, &url).await {
                        Ok(r) => r,
                        Err(e) => {
                            spans::record_error_with_context(
                                "HttpRequestFailed",
                                &format!("Failed to fetch attestation: {e}"),
                                Some(&format!("Attempt {attempt}/{max_attempts}")),
                            );
                            error!(
                                error = %e,
                                attempt = attempt,
                                event = "attestation_http_request_failed"
                            );
                            return Err(e);
                        }
                    };

                    let status_code = response.status().as_u16();
                    let process_span = spans::process_attestation_response(status_code, attempt);

                    async {
                        // Handle rate limiting
                        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                            let secs = 5 * 60;
                            debug!(sleep_secs = secs, event = "rate_limit_exceeded");
                            sleep(Duration::from_secs(secs)).await;
                            return Ok(None);
                        }

                        // Handle 404 status - treat as pending since the attestation likely doesn't exist yet
                        if response.status() == reqwest::StatusCode::NOT_FOUND {
                            debug!(event = "attestation_not_found");
                            sleep(Duration::from_secs(poll_interval)).await;
                            return Ok(None);
                        }

                        // Ensure the response status is successful before trying to parse JSON
                        response.error_for_status_ref()?;

                        // Get response body as text first for better error logging
                        let response_text = response.text().await?;

                        // Parse v2 response format (array of messages)
                        let v2_response: V2AttestationResponse =
                            match serde_json::from_str(&response_text) {
                                Ok(response) => response,
                                Err(e) => {
                                    error!(
                                        error = %e,
                                        response_body = %response_text,
                                        tx_hash = %tx_hash,
                                        attempt = attempt,
                                        event = "attestation_decode_failed"
                                    );
                                    sleep(Duration::from_secs(poll_interval)).await;
                                    return Ok(None);
                                }
                            };

                        // V2 can return several messages for one transaction. Pick
                        // the right action by scanning the whole array rather than
                        // trusting the first entry — the first could be pending or
                        // failed while a sibling carries usable attestation data.
                        let message_count = v2_response.messages.len();
                        match select_v2_attestation(&v2_response.messages) {
                            V2AttestationOutcome::Ready {
                                message: message_bytes,
                                attestation: attestation_bytes,
                            } => {
                                info!(
                                    message_length_bytes = message_bytes.len(),
                                    attestation_length_bytes = attestation_bytes.len(),
                                    message_count = message_count,
                                    version = "v2",
                                    fast_transfer = self.transfer_mode.is_fast(),
                                    event = "attestation_complete"
                                );
                                Ok(Some((message_bytes, attestation_bytes)))
                            }
                            V2AttestationOutcome::Failed => {
                                Err(super::attestation_api_reported_failed())
                            }
                            V2AttestationOutcome::AttestationMissing => {
                                Err(super::attestation_data_missing())
                            }
                            V2AttestationOutcome::MessageMissing => {
                                spans::record_error_with_context(
                                    "MessageDataMissing",
                                    "Attestation status is complete but message field is null",
                                    Some("This indicates an unexpected API response format"),
                                );
                                error!(event = "message_data_missing");
                                Err(CctpError::AttestationFailed(
                                    AttestationFailureKind::MessageMissing,
                                ))
                            }
                            V2AttestationOutcome::Pending => {
                                debug!(
                                    message_count = message_count,
                                    event = "attestation_pending"
                                );
                                sleep(Duration::from_secs(poll_interval)).await;
                                Ok(None)
                            }
                            V2AttestationOutcome::Empty => {
                                debug!(event = "no_messages_in_response");
                                sleep(Duration::from_secs(poll_interval)).await;
                                Ok(None)
                            }
                        }
                    }
                    .instrument(process_span)
                    .await
                }
                .instrument(attempt_span)
                .await;

                match attempt_result {
                    Ok(Some(pair)) => return Ok(pair),
                    Ok(None) => continue,
                    Err(e) => return Err(e),
                }
            }

            spans::record_error_with_context(
                "AttestationTimeout",
                &format!("Attestation polling timed out after {max_attempts} attempts"),
                Some(&format!("Total duration: {total_timeout_secs} seconds")),
            );
            error!(
                total_duration_secs = total_timeout_secs,
                event = "attestation_timeout"
            );
            Err(CctpError::AttestationTimeout)
        }
        .instrument(span)
        .await
    }

    /// Initiate a USDC burn on the source chain
    ///
    /// This creates and sends the depositForBurn transaction which locks USDC on the source
    /// chain and emits a `MessageSent` event.
    ///
    /// # Arguments
    ///
    /// * `amount` - Amount of USDC to transfer (in atomic units, e.g., 1 USDC = `1_000_000`)
    /// * `from` - Address that will send the transaction (must have USDC balance and gas)
    /// * `token_address` - USDC token contract address on source chain
    ///
    /// # Returns
    ///
    /// The transaction hash of the burn transaction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::{Address, U256};
    /// # async fn example<P>(bridge: CctpV2Bridge<P>) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let amount = U256::from(1_000_000); // 1 USDC
    /// let from_address = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse()?;
    ///
    /// let tx_hash = bridge.burn(amount, from_address, usdc).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn burn(
        &self,
        amount: U256,
        from: Address,
        token_address: Address,
    ) -> Result<TxHash> {
        let token_messenger_address = self.token_messenger_v2_contract()?;
        let destination_domain = self.destination_domain_id()?;

        let token_messenger =
            TokenMessengerV2Contract::new(token_messenger_address, self.source_provider.clone());

        // Wire values come from the same `TransferMode` helpers the accessors
        // expose, so `finality_threshold()` / `max_fee()` and the on-chain
        // `minFinalityThreshold` / `maxFee` cannot drift apart — the structural
        // defense behind the issue #218 fix.
        let max_fee = self.transfer_mode.max_fee();
        let min_finality_threshold = self.transfer_mode.finality_threshold().as_u32();

        // Match the enum variants directly: `TransferMode` is `#[non_exhaustive]`
        // for external callers, but exhaustiveness still applies in-crate, so a
        // new variant forces a compile error here until the wire shape is
        // decided.
        let tx_request = match &self.transfer_mode {
            TransferMode::Standard => token_messenger.deposit_for_burn_transaction(
                from,
                self.recipient,
                destination_domain,
                token_address,
                amount,
                min_finality_threshold,
            ),
            TransferMode::Fast { .. } => token_messenger.deposit_for_burn_fast_transaction(
                from,
                self.recipient,
                destination_domain,
                token_address,
                amount,
                max_fee,
                min_finality_threshold,
            ),
            TransferMode::StandardWithHook { hook_data }
            | TransferMode::FastWithHook { hook_data, .. } => token_messenger
                .deposit_for_burn_with_hooks_transaction(
                    from,
                    self.recipient,
                    destination_domain,
                    token_address,
                    amount,
                    max_fee,
                    min_finality_threshold,
                    hook_data.clone(),
                ),
        };

        info!(
            from = %from,
            amount = %amount,
            token_address = %token_address,
            destination_domain = %destination_domain,
            fast_transfer = self.transfer_mode.is_fast(),
            has_hooks = self.transfer_mode.has_hook(),
            finality_threshold = %self.transfer_mode.finality_threshold(),
            version = "v2",
            event = "burn_transaction_initiated"
        );

        let pending_tx = self.source_provider.send_transaction(tx_request).await?;
        let tx_hash = *pending_tx.tx_hash();

        info!(
            tx_hash = %tx_hash,
            version = "v2",
            event = "burn_transaction_sent"
        );

        Ok(tx_hash)
    }

    /// Complete a transfer by minting USDC on the destination chain
    ///
    /// This submits the receiveMessage transaction with the attestation to mint USDC
    /// on the destination chain.
    ///
    /// # Arguments
    ///
    /// * `message_bytes` - The message bytes from the `MessageSent` event
    /// * `attestation` - Circle's attestation signature for the message
    /// * `from` - Address that will submit the transaction (needs gas on destination chain)
    ///
    /// # Returns
    ///
    /// The transaction hash of the mint transaction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::Address;
    /// # async fn example<P>(
    /// #     bridge: CctpV2Bridge<P>,
    /// #     message: Vec<u8>,
    /// #     attestation: Vec<u8>
    /// # ) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let from_address = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    /// let tx_hash = bridge.mint(message, attestation, from_address).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn mint(
        &self,
        message_bytes: Vec<u8>,
        attestation: AttestationBytes,
        from: Address,
    ) -> Result<TxHash> {
        let message_transmitter_address = self.message_transmitter_v2_contract()?;

        let message_transmitter = MessageTransmitterV2Contract::new(
            message_transmitter_address,
            self.destination_provider.clone(),
        );

        let tx_request = message_transmitter.receive_message_transaction(
            Bytes::from(message_bytes.clone()),
            Bytes::from(attestation.clone()),
            from,
        );

        info!(
            from = %from,
            message_len = message_bytes.len(),
            attestation_len = attestation.len(),
            version = "v2",
            event = "mint_transaction_initiated"
        );

        let pending_tx = self
            .destination_provider
            .send_transaction(tx_request)
            .await?;
        let tx_hash = *pending_tx.tx_hash();

        info!(
            tx_hash = %tx_hash,
            version = "v2",
            event = "mint_transaction_sent"
        );

        Ok(tx_hash)
    }

    /// Check if a message has already been received on the destination chain
    ///
    /// This queries the on-chain `usedNonces` mapping to determine if the message
    /// was already processed (by us or a third-party relayer). Use this to check
    /// transfer status without attempting to mint.
    ///
    /// # Arguments
    ///
    /// * `message` - The canonical message bytes (from [`Self::get_attestation`])
    ///
    /// # Returns
    ///
    /// * `true` if the message has been processed (funds already minted)
    /// * `false` if the message is still pending
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (message, _attestation) = bridge.get_attestation(
    ///     burn_tx,
    ///     PollingConfig::fast_transfer(),
    /// ).await?;
    /// if bridge.is_message_received(&message).await? {
    ///     println!("Transfer already complete!");
    /// }
    /// ```
    pub async fn is_message_received(&self, message: &[u8]) -> Result<bool> {
        let message_transmitter_address = self.message_transmitter_v2_contract()?;
        let message_transmitter = MessageTransmitterV2Contract::new(
            message_transmitter_address,
            self.destination_provider.clone(),
        );

        let message_hash: [u8; 32] = alloy_primitives::keccak256(message).into();

        debug!(
            message_hash = %hex::encode(message_hash),
            version = "v2",
            event = "checking_message_received_status"
        );

        Ok(message_transmitter
            .is_message_received(message_hash)
            .await?)
    }

    /// Wait until a message has been received on the destination chain
    ///
    /// Polls the destination chain until the message is processed, regardless
    /// of who relayed it (your application or a third-party relayer). Use this
    /// when you don't need to self-relay and just want to know when the transfer
    /// is complete.
    ///
    /// # Arguments
    ///
    /// * `message` - The canonical message bytes (from [`Self::get_attestation`])
    /// * `max_attempts` - Maximum polling attempts (default: 60). `Some(0)` is
    ///   rejected with [`CctpError::InvalidConfig`].
    /// * `poll_interval` - Seconds between polls (default: based on transfer
    ///   type and chain). `Some(0)` is rejected with [`CctpError::InvalidConfig`].
    ///
    /// # Returns
    ///
    /// * `Ok(())` when the message has been received
    /// * `Err(CctpError::ReceiveTimeout)` if max attempts exceeded.
    ///   This is distinct from [`CctpError::AttestationTimeout`] — by
    ///   the time this method runs, attestation has already succeeded;
    ///   what timed out is observing the destination-chain receipt.
    /// * `Err(CctpError::InvalidConfig)` if either input is `Some(0)`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let burn_tx = bridge.burn(amount, from, usdc).await?;
    /// let (message, _attestation) = bridge.get_attestation(
    ///     burn_tx,
    ///     PollingConfig::fast_transfer(),
    /// ).await?;
    ///
    /// // Wait for completion (relayer or self)
    /// bridge.wait_for_receive(&message, None, None).await?;
    /// println!("Transfer complete!");
    /// ```
    pub async fn wait_for_receive(
        &self,
        message: &[u8],
        max_attempts: Option<u32>,
        poll_interval: Option<u64>,
    ) -> Result<()> {
        if matches!(max_attempts, Some(0)) {
            return Err(CctpError::InvalidConfig(
                "wait_for_receive max_attempts must be greater than 0".to_string(),
            ));
        }
        if matches!(poll_interval, Some(0)) {
            return Err(CctpError::InvalidConfig(
                "wait_for_receive poll_interval must be greater than 0".to_string(),
            ));
        }

        let max_attempts = max_attempts.unwrap_or(60);
        let poll_interval = poll_interval.unwrap_or_else(|| {
            if self.transfer_mode.is_fast() {
                self.destination_chain
                    .fast_transfer_confirmation_time_seconds()
                    .unwrap_or(5)
            } else {
                self.destination_chain
                    .standard_transfer_confirmation_time_seconds()
                    .unwrap_or(60)
            }
        });

        let message_hash: FixedBytes<32> = alloy_primitives::keccak256(message);
        let span = spans::wait_for_receive(
            &message_hash,
            &self.source_chain,
            &self.destination_chain,
            max_attempts,
            poll_interval,
        );

        async move {
            info!(
                max_attempts = max_attempts,
                poll_interval_secs = poll_interval,
                fast_transfer = self.transfer_mode.is_fast(),
                version = "v2",
                event = "wait_for_receive_started"
            );

            for attempt in 1..=max_attempts {
                if self.is_message_received(message).await? {
                    info!(
                        attempt = attempt,
                        version = "v2",
                        event = "message_received_confirmed"
                    );
                    return Ok(());
                }

                debug!(
                    attempt = attempt,
                    max_attempts = max_attempts,
                    version = "v2",
                    event = "message_not_yet_received"
                );

                sleep(Duration::from_secs(poll_interval)).await;
            }

            spans::record_error_with_context(
                "ReceiveTimeout",
                &format!(
                    "wait_for_receive polling timed out after {max_attempts} attempts"
                ),
                Some(&format!(
                    "Poll interval: {poll_interval} seconds; destination chain never reported receipt"
                )),
            );
            error!(
                max_attempts = max_attempts,
                poll_interval_secs = poll_interval,
                version = "v2",
                error_type = "ReceiveTimeout",
                event = "wait_for_receive_timeout"
            );

            Err(CctpError::ReceiveTimeout)
        }
        .instrument(span)
        .await
    }

    /// Attempt to mint, gracefully handling if already relayed
    ///
    /// This is the recommended method for production use. It uses a **conservative**
    /// strategy: always checks [`Self::is_message_received`] before attempting to mint.
    /// This avoids wasted gas on failed transactions when relayers are active.
    ///
    /// # Arguments
    ///
    /// * `message_bytes` - The canonical message (from [`Self::get_attestation`])
    /// * `attestation` - Circle's attestation signature
    /// * `from` - Address to submit the transaction from
    ///
    /// # Returns
    ///
    /// * `Ok(MintResult::Minted(tx_hash))` if we successfully minted
    /// * `Ok(MintResult::AlreadyRelayed)` if a relayer completed the transfer
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let burn_tx = bridge.burn(amount, from, usdc).await?;
    /// let (message, attestation) = bridge.get_attestation(
    ///     burn_tx,
    ///     PollingConfig::fast_transfer(),
    /// ).await?;
    ///
    /// match bridge.mint_if_needed(message, attestation, from).await? {
    ///     MintResult::Minted(tx) => println!("We minted: {tx}"),
    ///     MintResult::AlreadyRelayed => println!("Relayer completed it!"),
    /// }
    /// ```
    pub async fn mint_if_needed(
        &self,
        message_bytes: Vec<u8>,
        attestation: AttestationBytes,
        from: Address,
    ) -> Result<MintResult> {
        // Conservative approach: always check first to avoid wasted gas
        if self.is_message_received(&message_bytes).await? {
            info!(version = "v2", event = "mint_skipped_already_relayed");
            return Ok(MintResult::AlreadyRelayed);
        }

        // Attempt to mint
        match self.mint(message_bytes.clone(), attestation, from).await {
            Ok(tx_hash) => {
                info!(
                    tx_hash = %tx_hash,
                    version = "v2",
                    event = "mint_if_needed_successful"
                );
                Ok(MintResult::Minted(tx_hash))
            }
            Err(e) => {
                // Race condition: relayer may have completed between our check and mint
                // Use typed error detection instead of string matching
                if e.is_already_relayed() {
                    info!(
                        original_error = %e,
                        version = "v2",
                        event = "mint_raced_by_relayer"
                    );
                    Ok(MintResult::AlreadyRelayed)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Get the current ERC20 allowance for the `TokenMessenger` contract
    ///
    /// Use this to check if approval is needed before calling `burn`.
    ///
    /// # Arguments
    ///
    /// * `token_address` - The ERC20 token contract address (e.g., USDC)
    /// * `owner` - The address that owns the tokens
    ///
    /// # Returns
    ///
    /// The current allowance amount
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::{Address, U256};
    /// # async fn example<P>(bridge: CctpV2Bridge<P>) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse()?;
    /// let owner = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    ///
    /// let allowance = bridge.get_allowance(usdc, owner).await?;
    /// if allowance < U256::from(1_000_000) {
    ///     // Need to approve first
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_allowance(&self, token_address: Address, owner: Address) -> Result<U256> {
        let spender = self.token_messenger_v2_contract()?;
        let erc20 = Erc20Contract::new(token_address, self.source_provider.clone());

        Ok(erc20.allowance(owner, spender).await?)
    }

    /// Approve the `TokenMessenger` contract to spend tokens
    ///
    /// This must be called before `burn` if the `TokenMessenger` doesn't have
    /// sufficient allowance to transfer the desired amount.
    ///
    /// # Arguments
    ///
    /// * `token_address` - The ERC20 token contract address (e.g., USDC)
    /// * `owner` - The address that owns the tokens and will sign the transaction
    /// * `amount` - The amount to approve
    ///
    /// # Returns
    ///
    /// The transaction hash of the approval transaction
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::{Address, U256};
    /// # async fn example<P>(bridge: CctpV2Bridge<P>) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse()?;
    /// let owner = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    /// let amount = U256::from(1_000_000); // 1 USDC
    ///
    /// // Check allowance first
    /// let allowance = bridge.get_allowance(usdc, owner).await?;
    /// if allowance < amount {
    ///     let tx_hash = bridge.approve(usdc, owner, amount).await?;
    ///     println!("Approved: {}", tx_hash);
    /// }
    ///
    /// // Now burn is safe to call
    /// let burn_tx = bridge.burn(amount, owner, usdc).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn approve(
        &self,
        token_address: Address,
        owner: Address,
        amount: U256,
    ) -> Result<TxHash> {
        let spender = self.token_messenger_v2_contract()?;
        let erc20 = Erc20Contract::new(token_address, self.source_provider.clone());

        let tx_request = erc20.approve_transaction(owner, spender, amount);

        info!(
            owner = %owner,
            spender = %spender,
            amount = %amount,
            token_address = %token_address,
            version = "v2",
            event = "approval_transaction_initiated"
        );

        let pending_tx = self.source_provider.send_transaction(tx_request).await?;
        let tx_hash = *pending_tx.tx_hash();

        info!(
            tx_hash = %tx_hash,
            version = "v2",
            event = "approval_transaction_sent"
        );

        Ok(tx_hash)
    }

    /// Check if approval is needed and approve if necessary
    ///
    /// This is a convenience method that combines `get_allowance` and `approve`.
    /// It only sends an approval transaction if the current allowance is less than
    /// the requested amount.
    ///
    /// # Arguments
    ///
    /// * `token_address` - The ERC20 token contract address (e.g., USDC)
    /// * `owner` - The address that owns the tokens
    /// * `amount` - The amount that needs to be approved
    ///
    /// # Returns
    ///
    /// `Some(tx_hash)` if an approval was sent, `None` if approval was already sufficient
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::{Address, U256};
    /// # async fn example<P>(bridge: CctpV2Bridge<P>) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse()?;
    /// let owner = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    /// let amount = U256::from(1_000_000);
    ///
    /// // Approve if needed, then burn
    /// if let Some(approval_tx) = bridge.ensure_approval(usdc, owner, amount).await? {
    ///     println!("Approval sent: {}", approval_tx);
    /// }
    /// let burn_tx = bridge.burn(amount, owner, usdc).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn ensure_approval(
        &self,
        token_address: Address,
        owner: Address,
        amount: U256,
    ) -> Result<Option<TxHash>> {
        let current_allowance = self.get_allowance(token_address, owner).await?;

        if current_allowance >= amount {
            info!(
                owner = %owner,
                current_allowance = %current_allowance,
                required_amount = %amount,
                token_address = %token_address,
                version = "v2",
                event = "approval_not_needed"
            );
            return Ok(None);
        }

        info!(
            owner = %owner,
            current_allowance = %current_allowance,
            required_amount = %amount,
            token_address = %token_address,
            version = "v2",
            event = "approval_needed"
        );

        let tx_hash = self.approve(token_address, owner, amount).await?;
        Ok(Some(tx_hash))
    }

    /// Execute a full cross-chain transfer: burn + wait for attestation + mint
    ///
    /// This is a convenience method that orchestrates the complete transfer flow:
    /// 1. Burns USDC on source chain
    /// 2. Extracts `MessageSent` event from burn transaction
    /// 3. Polls Circle's Iris API for attestation
    /// 4. Mints USDC on destination chain
    ///
    /// # Arguments
    ///
    /// * `amount` - Amount of USDC to transfer (in atomic units)
    /// * `from` - Address initiating the transfer (needs USDC + gas on source, gas on destination)
    /// * `token_address` - USDC token contract address on source chain
    ///
    /// # Returns
    ///
    /// Tuple of (`burn_tx_hash`, `mint_tx_hash`)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::{Address, U256};
    /// # async fn example<P>(bridge: CctpV2Bridge<P>) -> Result<(), Box<dyn std::error::Error>>
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let amount = U256::from(1_000_000); // 1 USDC
    /// let from_address = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d".parse()?;
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse()?;
    ///
    /// let (burn_tx, mint_tx) = bridge.transfer(amount, from_address, usdc).await?;
    /// println!("Transfer complete! Burn: {}, Mint: {}", burn_tx, mint_tx);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn transfer(
        &self,
        amount: U256,
        from: Address,
        token_address: Address,
    ) -> Result<(TxHash, TxHash)> {
        info!(
            amount = %amount,
            from = %from,
            token_address = %token_address,
            source_chain = ?self.source_chain,
            destination_chain = ?self.destination_chain,
            fast_transfer = self.transfer_mode.is_fast(),
            has_hooks = self.transfer_mode.has_hook(),
            version = "v2",
            event = "full_transfer_initiated"
        );

        // Step 1: Burn tokens on source chain
        let burn_tx_hash = self.burn(amount, from, token_address).await?;

        info!(
            burn_tx_hash = %burn_tx_hash,
            event = "waiting_for_message_sent_event"
        );

        // Step 2: Poll for attestation and get canonical message from Circle's API
        // Note: The MessageSent event log contains zeros in the nonce field.
        // Circle fills in the actual nonce before signing, so we must use the message
        // returned by get_attestation (from Circle's API), not from the event log.
        let polling_config = if self.transfer_mode.is_fast() {
            PollingConfig::fast_transfer()
        } else {
            PollingConfig::default()
        };
        let (message_bytes, attestation) =
            self.get_attestation(burn_tx_hash, polling_config).await?;

        info!(
            burn_tx_hash = %burn_tx_hash,
            message_len = message_bytes.len(),
            attestation_len = attestation.len(),
            event = "attestation_received"
        );

        // Step 3: Mint tokens on destination chain
        let mint_tx_hash = self.mint(message_bytes, attestation, from).await?;

        info!(
            burn_tx_hash = %burn_tx_hash,
            mint_tx_hash = %mint_tx_hash,
            version = "v2",
            event = "full_transfer_completed"
        );

        Ok((burn_tx_hash, mint_tx_hash))
    }

    /// Constructs the Iris API v2 URL for attestation polling
    ///
    /// The v2 API uses a different endpoint format than v1:
    /// - V1: `/v1/attestations/{messageHash}`
    /// - V2: `/v2/messages/{sourceDomain}?transactionHash={txHash}`
    ///
    /// # Arguments
    ///
    /// * `tx_hash` - The transaction hash of the burn transaction on the source chain
    ///
    /// # Returns
    ///
    /// The v2 messages endpoint URL with source domain and transaction hash
    ///
    /// # Errors
    ///
    /// Returns `CctpError::InvalidUrl` if URL construction fails, or
    /// `CctpError::UnsupportedChain` if the source chain doesn't have a v2 domain ID.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// # use cctp_rs::CctpV2Bridge;
    /// # use alloy_primitives::TxHash;
    /// # fn example<P>(bridge: &CctpV2Bridge<P>)
    /// # where P: alloy_provider::Provider<alloy_network::Ethereum> + Clone
    /// # {
    /// let tx_hash: TxHash = "0x123...".parse().unwrap();
    /// let url = bridge.create_url(tx_hash).unwrap();
    /// // URL format: https://iris-api.circle.com/v2/messages/0?transactionHash=0x123...
    /// assert!(url.as_str().contains("/v2/messages/"));
    /// assert!(url.as_str().contains("transactionHash="));
    /// # }
    /// ```
    ///
    /// See <https://developers.circle.com/cctp/transfer-usdc-on-testnet-from-ethereum-to-avalanche>
    pub fn create_url(&self, tx_hash: TxHash) -> Result<Url> {
        let source_domain = self.source_chain.cctp_v2_domain_id()?.as_u32();
        Ok(self.api_url().join(&format!(
            "{MESSAGES_PATH_V2}{source_domain}?transactionHash={tx_hash}"
        ))?)
    }

    fn transfer_fee_domain_ids(&self) -> Result<(u32, u32)> {
        Ok((
            self.source_chain.cctp_v2_domain_id()?.as_u32(),
            self.destination_chain.cctp_v2_domain_id()?.as_u32(),
        ))
    }

    /// Fetches the attestation response from the CCTP v2 API
    ///
    /// # Arguments
    ///
    /// * `client`: The HTTP client to use
    /// * `url`: The URL to get the attestation from
    ///
    async fn fetch_attestation_response(&self, client: &Client, url: &Url) -> Result<Response> {
        client
            .get(url.as_str())
            .send()
            .await
            .map_err(CctpError::Network)
    }

    /// Fetches the transfer fee response from the CCTP v2 API
    async fn fetch_transfer_fees_response(&self, client: &Client, url: &Url) -> Result<Response> {
        client
            .get(url.as_str())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(CctpError::Network)
    }
}

// Implement CctpBridge trait for v2 CctpV2 struct
#[async_trait]
impl<P: Provider<Ethereum> + Clone> CctpBridge for CctpV2<P> {
    fn source_chain(&self) -> NamedChain {
        self.source_chain
    }

    fn destination_chain(&self) -> NamedChain {
        self.destination_chain
    }

    fn recipient(&self) -> Address {
        self.recipient
    }

    async fn get_message_sent_event(&self, tx_hash: TxHash) -> Result<(Vec<u8>, FixedBytes<32>)> {
        self.get_message_sent_event(tx_hash).await
    }

    fn supports_fast_transfer(&self) -> bool {
        self.transfer_mode.is_fast()
    }

    fn supports_hooks(&self) -> bool {
        self.transfer_mode.has_hook()
    }

    fn finality_threshold(&self) -> Option<FinalityThreshold> {
        Some(self.finality_threshold())
    }
}

/// The action a single poll of the v2 attestation API should take, derived
/// from scanning every message Circle returned for the transaction.
///
/// A v2 query is keyed by transaction hash, and one transaction can emit
/// multiple `MessageSent` events, so the response array may hold a mix of
/// pending, complete, and failed entries. Selecting on the array as a whole
/// keeps a usable attestation from being missed because it sat behind a
/// pending or failed sibling.
#[derive(Debug, PartialEq, Eq)]
enum V2AttestationOutcome {
    /// A complete message carrying both message and attestation bytes.
    Ready {
        message: Vec<u8>,
        attestation: AttestationBytes,
    },
    /// No usable message yet, but at least one is still pending — keep polling.
    Pending,
    /// A `Complete` message was found but its attestation field was null.
    AttestationMissing,
    /// A `Complete` message was found but its message field was null.
    MessageMissing,
    /// Every message reported failure and none carried usable data.
    Failed,
    /// The response held no messages — keep polling.
    Empty,
}

/// Chooses what to do with a v2 attestation response's message array.
///
/// Precedence, highest first:
/// 1. Any `Complete` message with both fields present succeeds, even if a
///    sibling has failed.
/// 2. A still-pending message means keep polling — a failed sibling does not
///    end the transaction while another message may yet complete.
/// 3. A `Complete` message missing its data is surfaced as a malformed
///    response (attestation checked before message, matching the prior path).
/// 4. With nothing pending or usable, an all-failed response fails.
/// 5. An empty array keeps polling.
fn select_v2_attestation(messages: &[V2Message]) -> V2AttestationOutcome {
    if let Some((message, attestation)) = messages.iter().find_map(|message| {
        if message.status != AttestationStatus::Complete {
            return None;
        }
        Some((
            message.message.as_ref()?.to_vec(),
            message.attestation.as_ref()?.to_vec(),
        ))
    }) {
        return V2AttestationOutcome::Ready {
            message,
            attestation,
        };
    }

    if messages.iter().any(|message| {
        matches!(
            message.status,
            AttestationStatus::Pending | AttestationStatus::PendingConfirmations
        )
    }) {
        return V2AttestationOutcome::Pending;
    }

    if let Some(message) = messages
        .iter()
        .find(|message| message.status == AttestationStatus::Complete)
    {
        return if message.attestation.is_none() {
            V2AttestationOutcome::AttestationMissing
        } else {
            V2AttestationOutcome::MessageMissing
        };
    }

    if messages
        .iter()
        .any(|message| message.status == AttestationStatus::Failed)
    {
        return V2AttestationOutcome::Failed;
    }

    V2AttestationOutcome::Empty
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use alloy_primitives::{Address, FixedBytes};
    use alloy_provider::ProviderBuilder;
    use rstest::rstest;

    #[rstest]
    #[case(NamedChain::Mainnet, NamedChain::Linea)]
    #[case(NamedChain::Arbitrum, NamedChain::Sonic)]
    #[case(NamedChain::Base, NamedChain::Sei)]
    #[case(NamedChain::Sepolia, NamedChain::BaseSepolia)]
    fn test_v2_cross_chain_compatibility(
        #[case] source: NamedChain,
        #[case] destination: NamedChain,
    ) {
        // Test that chains support v2
        assert!(source.supports_cctp_v2());
        assert!(destination.supports_cctp_v2());

        // Test that we can get domain IDs for supported chains
        assert!(source.cctp_v2_domain_id().is_ok());
        assert!(destination.cctp_v2_domain_id().is_ok());
        assert!(source.token_messenger_v2_address().is_ok());
        assert!(destination.message_transmitter_v2_address().is_ok());
    }

    #[test]
    fn test_v2_unsupported_chain_error() {
        let result = NamedChain::Moonbeam.token_messenger_v2_address();
        assert!(result.is_err());
    }

    #[test]
    fn test_v2_messages_url_format_mainnet() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // V2 uses transaction hash, not message hash
        let test_tx_hash: TxHash = FixedBytes::from([0x12; 32]);
        let url = bridge.create_url(test_tx_hash).unwrap();
        // Format: /v2/messages/{domain}?transactionHash={txHash}
        // Ethereum mainnet domain = 0
        insta::assert_snapshot!(url.as_str(), @"https://iris-api.circle.com/v2/messages/0?transactionHash=0x1212121212121212121212121212121212121212121212121212121212121212");
    }

    #[test]
    fn test_v2_messages_url_format_sepolia() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Sepolia)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // V2 uses transaction hash, not message hash
        let test_tx_hash: TxHash = FixedBytes::from([0x12; 32]);
        let url = bridge.create_url(test_tx_hash).unwrap();
        // Format: /v2/messages/{domain}?transactionHash={txHash}
        // Sepolia domain = 0 (same as mainnet Ethereum)
        insta::assert_snapshot!(url.as_str(), @"https://iris-api-sandbox.circle.com/v2/messages/0?transactionHash=0x1212121212121212121212121212121212121212121212121212121212121212");
    }

    #[test]
    fn test_v2_transfer_fees_url_format_mainnet() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        let url = bridge.create_transfer_fees_url().unwrap();
        insta::assert_snapshot!(url.as_str(), @"https://iris-api.circle.com/v2/burn/USDC/fees/0/11");
    }

    #[test]
    fn test_v2_transfer_fees_url_format_testnet() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Sepolia)
            .destination_chain(NamedChain::BaseSepolia)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        let url = bridge.create_transfer_fees_url().unwrap();
        insta::assert_snapshot!(url.as_str(), @"https://iris-api-sandbox.circle.com/v2/burn/USDC/fees/0/6");
    }

    #[test]
    fn test_v2_fast_transfer_flag() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Standard transfer (default)
        let standard = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();

        assert!(!standard.is_fast_transfer());
        assert_eq!(standard.finality_threshold(), FinalityThreshold::Standard);
        assert!(!standard.supports_fast_transfer());

        // Fast transfer
        let fast = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::ZERO,
            })
            .build();

        assert!(fast.is_fast_transfer());
        assert_eq!(fast.finality_threshold(), FinalityThreshold::Fast);
        assert!(fast.supports_fast_transfer());
    }

    #[test]
    fn test_v2_hooks_support() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Without hooks
        let no_hooks = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();

        assert!(!no_hooks.supports_hooks());
        assert!(no_hooks.hook_data().is_none());

        // With hooks
        let hook_data = Bytes::from(vec![1, 2, 3, 4]);
        let with_hooks = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::StandardWithHook {
                hook_data: hook_data.clone(),
            })
            .build();

        assert!(with_hooks.supports_hooks());
        assert_eq!(with_hooks.hook_data(), Some(&hook_data));
    }

    #[test]
    fn test_v2_max_fee() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let max_fee = U256::from(1000);

        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast { max_fee })
            .build();

        assert_eq!(bridge.max_fee(), Some(max_fee));
    }

    #[test]
    fn test_v2_unified_addresses() {
        // All v2 mainnet chains should have the same addresses
        let linea_tm = NamedChain::Linea.token_messenger_v2_address().unwrap();
        let sonic_tm = NamedChain::Sonic.token_messenger_v2_address().unwrap();
        let mainnet_tm = NamedChain::Mainnet.token_messenger_v2_address().unwrap();

        assert_eq!(linea_tm, sonic_tm);
        assert_eq!(linea_tm, mainnet_tm);

        let linea_mt = NamedChain::Linea.message_transmitter_v2_address().unwrap();
        let sonic_mt = NamedChain::Sonic.message_transmitter_v2_address().unwrap();
        let mainnet_mt = NamedChain::Mainnet
            .message_transmitter_v2_address()
            .unwrap();

        assert_eq!(linea_mt, sonic_mt);
        assert_eq!(linea_mt, mainnet_mt);
    }

    #[test]
    fn test_v2_builder_pattern() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Build with all options
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::FastWithHook {
                max_fee: U256::from(500),
                hook_data: Bytes::from(vec![1, 2, 3]),
            })
            .build();

        assert_eq!(bridge.source_chain(), &NamedChain::Mainnet);
        assert_eq!(bridge.destination_chain(), &NamedChain::Linea);
        assert_eq!(bridge.recipient(), &Address::ZERO);
        assert!(bridge.is_fast_transfer());
        assert_eq!(bridge.max_fee(), Some(U256::from(500)));
        assert!(bridge.hook_data().is_some());
    }

    #[test]
    fn test_v2_from_route_builder_populates_chains() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let route = CctpV2Route::new(NamedChain::Mainnet, NamedChain::Linea).unwrap();

        let bridge = CctpV2::from_route(route)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::from(500),
            })
            .build();

        assert_eq!(bridge.source_chain(), &NamedChain::Mainnet);
        assert_eq!(bridge.destination_chain(), &NamedChain::Linea);
        assert_eq!(bridge.recipient(), &Address::ZERO);
        assert!(bridge.is_fast_transfer());
        assert_eq!(bridge.max_fee(), Some(U256::from(500)));
    }

    // Integration tests for transfer flow logic

    #[test]
    fn test_v2_contract_method_selection_standard() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Standard transfer should use basic depositForBurn
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // Verify configuration for standard transfer
        assert!(!bridge.is_fast_transfer());
        assert!(bridge.hook_data().is_none());
        assert_eq!(bridge.finality_threshold(), FinalityThreshold::Standard);
        assert_eq!(bridge.finality_threshold().as_u32(), 2000);
    }

    #[test]
    fn test_v2_contract_method_selection_fast() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Fast transfer should use depositForBurnFast
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::from(1000),
            })
            .build();

        // Verify configuration for fast transfer
        assert!(bridge.is_fast_transfer());
        assert!(bridge.hook_data().is_none());
        assert_eq!(bridge.finality_threshold(), FinalityThreshold::Fast);
        assert_eq!(bridge.finality_threshold().as_u32(), 1000);
        assert_eq!(bridge.max_fee(), Some(U256::from(1000)));
    }

    #[test]
    fn test_v2_contract_method_selection_hooks() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let hook_data = Bytes::from(vec![1, 2, 3, 4]);

        // With hooks should use depositForBurnWithHook
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::StandardWithHook {
                hook_data: hook_data.clone(),
            })
            .build();

        // Verify configuration for hooks transfer
        assert!(!bridge.is_fast_transfer());
        assert_eq!(bridge.hook_data(), Some(&hook_data));
        assert_eq!(bridge.finality_threshold(), FinalityThreshold::Standard);
    }

    #[test]
    fn test_v2_fast_with_hook_uses_fast_finality_and_fee() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let hook_data = Bytes::from(vec![1, 2, 3, 4]);

        // FastWithHook combines fast finality with hook data and a fee cap.
        // Previously the bridge silently fell back to standard finality with
        // zero fee while still reporting fast — see issue #218.
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::FastWithHook {
                max_fee: U256::from(1000),
                hook_data: hook_data.clone(),
            })
            .build();

        assert!(bridge.is_fast_transfer());
        assert_eq!(bridge.hook_data(), Some(&hook_data));
        assert_eq!(bridge.finality_threshold(), FinalityThreshold::Fast);
        assert_eq!(bridge.max_fee(), Some(U256::from(1000)));
        assert!(matches!(
            bridge.transfer_mode(),
            TransferMode::FastWithHook { .. }
        ));
    }

    #[test]
    fn test_v2_fast_with_hook_calldata_carries_fast_finality_and_max_fee() {
        use crate::contracts::v2::TokenMessengerV2;
        use alloy_sol_types::SolCall;

        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let hook_data = Bytes::from(vec![0xde, 0xad]);
        let max_fee = U256::from(1000);

        // Issue #218 was an accessor-vs-wire divergence: the bridge reported fast
        // finality while the on-chain call carried standard finality and zero fee.
        // Pin the fix to the wire by decoding the calldata the bridge would send
        // and asserting `minFinalityThreshold` and `maxFee` directly.
        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::FastWithHook {
                max_fee,
                hook_data: hook_data.clone(),
            })
            .build();

        let from = Address::repeat_byte(0xaa);
        let token_address = Address::repeat_byte(0xbb);
        let amount = U256::from(1_000_000_u64);
        let destination_domain = bridge.destination_domain_id().unwrap();
        let token_messenger_address = bridge.source_chain().token_messenger_v2_address().unwrap();
        let token_messenger = TokenMessengerV2Contract::new(token_messenger_address, provider);

        // Pull wire values from `bridge.transfer_mode()` — the same source `burn()`
        // uses (`src/bridge/v2.rs:591-592`). A future refactor that drops either
        // accessor must break this test before it can ship a wrong wire shape.
        let mode = bridge.transfer_mode();
        let mode_max_fee = mode.max_fee();
        let mode_finality_threshold = mode.finality_threshold().as_u32();
        let mode_hook_data = mode
            .hook_data()
            .expect("FastWithHook carries hook data")
            .clone();

        let tx_request = token_messenger.deposit_for_burn_with_hooks_transaction(
            from,
            *bridge.recipient(),
            destination_domain,
            token_address,
            amount,
            mode_max_fee,
            mode_finality_threshold,
            mode_hook_data,
        );

        let calldata = tx_request
            .input
            .input()
            .expect("transaction request carries calldata");

        let decoded = TokenMessengerV2::depositForBurnWithHookCall::abi_decode(calldata)
            .expect("calldata decodes as depositForBurnWithHook");

        assert_eq!(decoded.minFinalityThreshold, 1000);
        assert_eq!(decoded.maxFee, max_fee);
        assert_eq!(decoded.amount, amount);
        assert_eq!(decoded.destinationDomain, destination_domain.as_u32());
        assert_eq!(decoded.hookData, hook_data);
    }

    #[rstest]
    #[case(NamedChain::Mainnet, NamedChain::Linea)]
    #[case(NamedChain::Arbitrum, NamedChain::Sonic)]
    #[case(NamedChain::Base, NamedChain::Sei)]
    #[case(NamedChain::Sepolia, NamedChain::BaseSepolia)]
    fn test_v2_fast_transfer_chain_support(
        #[case] source: NamedChain,
        #[case] destination: NamedChain,
    ) {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // All v2 chains support fast transfer
        let bridge = CctpV2::builder()
            .source_chain(source)
            .destination_chain(destination)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::ZERO,
            })
            .build();

        assert!(bridge.supports_fast_transfer());
        assert_eq!(bridge.finality_threshold(), FinalityThreshold::Fast);
    }

    #[test]
    fn test_v2_domain_id_resolution() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // Verify domain IDs are correctly resolved
        let source_domain = bridge.source_chain().cctp_v2_domain_id().unwrap();
        let dest_domain = bridge.destination_domain_id().unwrap();

        assert_eq!(source_domain, DomainId::Ethereum);
        assert_eq!(dest_domain, DomainId::Linea);
        assert_eq!(source_domain.as_u32(), 0);
        assert_eq!(dest_domain.as_u32(), 11);
    }

    #[test]
    fn test_v2_contract_address_resolution() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // Verify contract addresses are correctly resolved
        let token_messenger = bridge.token_messenger_v2_contract().unwrap();
        let message_transmitter = bridge.message_transmitter_v2_contract().unwrap();

        // Mainnet v2 addresses (unified across all v2 chains)
        assert_eq!(
            token_messenger,
            "0x28b5a0e9C621a5BadaA536219b3a228C8168cf5d"
                .parse::<Address>()
                .unwrap()
        );
        assert_eq!(
            message_transmitter,
            "0x81D40F21F12A8F0E3252Bccb954D722d4c464B64"
                .parse::<Address>()
                .unwrap()
        );
    }

    #[test]
    fn test_v2_api_url_construction() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Mainnet should use production API
        let mainnet_bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();

        let test_tx_hash: TxHash = FixedBytes::from([0xab; 32]);
        let mainnet_url = mainnet_bridge.create_url(test_tx_hash).unwrap();
        assert!(mainnet_url.as_str().contains("iris-api.circle.com"));
        assert!(mainnet_url.as_str().contains("/v2/messages/"));
        assert!(mainnet_url.as_str().contains("transactionHash="));

        // Testnet should use sandbox API
        let testnet_bridge = CctpV2::builder()
            .source_chain(NamedChain::Sepolia)
            .destination_chain(NamedChain::BaseSepolia)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        let testnet_url = testnet_bridge.create_url(test_tx_hash).unwrap();
        assert!(testnet_url.as_str().contains("iris-api-sandbox.circle.com"));
        assert!(testnet_url.as_str().contains("/v2/messages/"));
        assert!(testnet_url.as_str().contains("transactionHash="));
    }

    #[test]
    fn test_v2_finality_threshold_mapping() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Standard transfer
        let standard = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();

        assert_eq!(standard.finality_threshold(), FinalityThreshold::Standard);
        assert_eq!(standard.finality_threshold().as_u32(), 2000);
        assert!(standard.finality_threshold().is_standard());
        assert!(!standard.finality_threshold().is_fast());

        // Fast transfer
        let fast = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::ZERO,
            })
            .build();

        assert_eq!(fast.finality_threshold(), FinalityThreshold::Fast);
        assert_eq!(fast.finality_threshold().as_u32(), 1000);
        assert!(!fast.finality_threshold().is_standard());
        assert!(fast.finality_threshold().is_fast());
    }

    #[rstest]
    #[case(NamedChain::Mainnet, NamedChain::Linea)]
    #[case(NamedChain::Arbitrum, NamedChain::Sonic)]
    #[case(NamedChain::Base, NamedChain::Sei)]
    #[case(NamedChain::Optimism, NamedChain::Polygon)]
    fn test_v2_cross_chain_integration(
        #[case] source: NamedChain,
        #[case] destination: NamedChain,
    ) {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Verify we can create a bridge for any valid v2 chain pair
        let bridge = CctpV2::builder()
            .source_chain(source)
            .destination_chain(destination)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .build();

        // All should resolve successfully
        assert!(bridge.source_chain().cctp_v2_domain_id().is_ok());
        assert!(bridge.destination_domain_id().is_ok());
        assert!(bridge.token_messenger_v2_contract().is_ok());
        assert!(bridge.message_transmitter_v2_contract().is_ok());

        // All mainnet chains should have the same v2 contract addresses
        if !source.is_testnet() && !destination.is_testnet() {
            let token_messenger = bridge.token_messenger_v2_contract().unwrap();
            let message_transmitter = bridge.message_transmitter_v2_contract().unwrap();

            assert_eq!(
                token_messenger,
                "0x28b5a0e9C621a5BadaA536219b3a228C8168cf5d"
                    .parse::<Address>()
                    .unwrap()
            );
            assert_eq!(
                message_transmitter,
                "0x81D40F21F12A8F0E3252Bccb954D722d4c464B64"
                    .parse::<Address>()
                    .unwrap()
            );
        }
    }

    #[test]
    fn test_v2_error_handling_unsupported_chain() {
        // Try to get v2 addresses for a chain that doesn't support v2
        let result = NamedChain::Moonbeam.token_messenger_v2_address();
        assert!(result.is_err());

        let result = NamedChain::Moonbeam.message_transmitter_v2_address();
        assert!(result.is_err());

        let result = NamedChain::Moonbeam.cctp_v2_domain_id();
        assert!(result.is_err());
    }

    #[test]
    fn test_v2_recipient_address_validation() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let recipient = "0x742d35Cc6634C0532925a3b844Bc9e7595f8fA0d"
            .parse::<Address>()
            .unwrap();

        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(recipient)
            .build();

        assert_eq!(bridge.recipient(), &recipient);
    }

    #[test]
    fn test_v2_max_fee_defaults() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());

        // Fast with zero max_fee — still reports Some(ZERO) because the mode is fast.
        // Standard modes report None.
        let zero_fee = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::ZERO,
            })
            .build();

        assert_eq!(zero_fee.max_fee(), Some(U256::ZERO));

        // Standard modes have no fee
        let standard = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider.clone())
            .recipient(Address::ZERO)
            .build();
        assert_eq!(standard.max_fee(), None);

        // With max_fee specified
        let with_fee = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::Fast {
                max_fee: U256::from(500),
            })
            .build();

        assert_eq!(with_fee.max_fee(), Some(U256::from(500)));
    }

    #[test]
    fn test_v2_hooks_data_validation() {
        let provider =
            ProviderBuilder::new().connect_http("http://localhost:8545".parse().unwrap());
        let hook_data = Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]);

        let bridge = CctpV2::builder()
            .source_chain(NamedChain::Mainnet)
            .destination_chain(NamedChain::Linea)
            .source_provider(provider.clone())
            .destination_provider(provider)
            .recipient(Address::ZERO)
            .transfer_mode(TransferMode::StandardWithHook {
                hook_data: hook_data.clone(),
            })
            .build();

        assert_eq!(bridge.hook_data(), Some(&hook_data));
        assert_eq!(bridge.hook_data().unwrap().len(), 4);
        assert_eq!(bridge.hook_data().unwrap()[0], 0xde);
    }

    mod select_v2_attestation {
        //! Unit coverage for the multi-message selection in
        //! [`super::super::select_v2_attestation`] (issue #213).
        //!
        //! V2 polling used to read `messages.first()` and act on that one
        //! entry's status, so a complete attestation sitting behind a
        //! pending or failed sibling was either ignored or treated as a
        //! whole-transaction failure. These tests pin the array-wide
        //! precedence: a usable complete message wins over anything, a
        //! pending sibling keeps polling alive, and only an all-failed
        //! response fails.
        use super::super::{select_v2_attestation, V2AttestationOutcome};
        use super::*;

        fn message(
            status: AttestationStatus,
            message: Option<&[u8]>,
            attestation: Option<&[u8]>,
        ) -> V2Message {
            V2Message {
                status,
                message: message.map(Bytes::copy_from_slice),
                attestation: attestation.map(Bytes::copy_from_slice),
            }
        }

        fn complete(msg: &[u8], attestation: &[u8]) -> V2Message {
            message(AttestationStatus::Complete, Some(msg), Some(attestation))
        }

        #[test]
        fn empty_array_keeps_polling() {
            assert_eq!(select_v2_attestation(&[]), V2AttestationOutcome::Empty);
        }

        #[test]
        fn single_complete_message_is_ready() {
            let messages = vec![complete(&[0xde, 0xad], &[0xbe, 0xef])];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Ready {
                    message: vec![0xde, 0xad],
                    attestation: vec![0xbe, 0xef],
                }
            );
        }

        #[test]
        fn complete_behind_pending_is_selected() {
            // The defect: the first entry is pending, the usable attestation
            // sits second. The old `messages.first()` path would have slept
            // and kept polling forever even though data was already present.
            let messages = vec![
                message(AttestationStatus::Pending, None, None),
                complete(&[0x01, 0x02], &[0x03, 0x04]),
            ];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Ready {
                    message: vec![0x01, 0x02],
                    attestation: vec![0x03, 0x04],
                }
            );
        }

        #[test]
        fn complete_behind_failed_is_selected() {
            // The defect's other half: a failed first entry would have failed
            // the whole transaction even though a sibling completed.
            let messages = vec![
                message(AttestationStatus::Failed, None, None),
                complete(&[0xaa], &[0xbb]),
            ];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Ready {
                    message: vec![0xaa],
                    attestation: vec![0xbb],
                }
            );
        }

        #[test]
        fn first_complete_message_wins_when_several_are_ready() {
            let messages = vec![complete(&[0x11], &[0x22]), complete(&[0x33], &[0x44])];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Ready {
                    message: vec![0x11],
                    attestation: vec![0x22],
                }
            );
        }

        #[test]
        fn pending_sibling_outranks_failed_when_none_complete() {
            // A failed message must not end the transaction while a sibling
            // is still pending and might yet complete on a later poll.
            let messages = vec![
                message(AttestationStatus::Failed, None, None),
                message(AttestationStatus::PendingConfirmations, None, None),
            ];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Pending
            );
        }

        #[test]
        fn all_failed_fails() {
            let messages = vec![
                message(AttestationStatus::Failed, None, None),
                message(AttestationStatus::Failed, None, None),
            ];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Failed
            );
        }

        #[test]
        fn complete_missing_attestation_reports_attestation_missing() {
            // No pending sibling, so the malformed complete is surfaced rather
            // than masked — attestation is checked before message.
            let messages = vec![message(
                AttestationStatus::Complete,
                Some(&[0xde, 0xad]),
                None,
            )];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::AttestationMissing
            );
        }

        #[test]
        fn complete_missing_message_reports_message_missing() {
            let messages = vec![message(
                AttestationStatus::Complete,
                None,
                Some(&[0xbe, 0xef]),
            )];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::MessageMissing
            );
        }

        #[test]
        fn pending_outranks_malformed_complete() {
            // A complete-but-empty entry alongside a pending one keeps polling:
            // the pending message may resolve into a usable attestation.
            let messages = vec![
                message(AttestationStatus::Complete, None, None),
                message(AttestationStatus::Pending, None, None),
            ];
            assert_eq!(
                select_v2_attestation(&messages),
                V2AttestationOutcome::Pending
            );
        }
    }

    mod burn_dispatch_wire {
        //! End-to-end coverage of `CctpV2::burn`'s dispatch arm.
        //!
        //! Issue #229: the existing fast-with-hook wire-shape test
        //! (`test_v2_fast_with_hook_calldata_carries_fast_finality_and_max_fee`)
        //! builds a `TokenMessengerV2Contract` directly and never exercises
        //! the `match &self.transfer_mode` dispatch inside `burn()` itself
        //! (`src/bridge/v2.rs:598-628`). That dispatch was the home of the
        //! issue #218 bug — a future refactor that rewires an arm to the
        //! wrong contract method could silently re-introduce the same
        //! defect with every accessor-level test still green.
        //!
        //! The tests below drive `bridge.burn(amount, from, token).await`
        //! against a capturing in-process JSON-RPC transport, then decode
        //! the captured `eth_sendTransaction` calldata with the same
        //! `sol!`-generated call types the contract uses. Every
        //! `TransferMode` variant is covered, so any rewiring that lands
        //! the wrong call on the wire — `depositForBurn` vs
        //! `depositForBurnWithHook`, fast vs standard finality threshold,
        //! zero vs caller-supplied `max_fee` — has to fail a test here.
        use super::*;
        use alloy_json_rpc::{RequestPacket, Response, ResponsePacket, ResponsePayload};
        use alloy_provider::ProviderBuilder;
        use alloy_rpc_client::RpcClient;
        use alloy_rpc_types::TransactionRequest;
        use alloy_sol_types::SolCall;
        use rstest::rstest;
        use std::sync::{Arc, Mutex};
        use std::task::{Context, Poll};
        use tower::Service;

        use crate::contracts::v2::TokenMessengerV2;

        /// A tower service that records every JSON-RPC request and answers
        /// each with a fixed transaction hash. `burn()` only issues a
        /// single `eth_sendTransaction` once recommended fillers are
        /// disabled, so the captured queue is exactly what hit the wire.
        #[derive(Clone, Default)]
        struct CapturingTransport {
            requests: Arc<Mutex<Vec<alloy_json_rpc::SerializedRequest>>>,
        }

        impl Service<RequestPacket> for CapturingTransport {
            type Response = ResponsePacket;
            type Error = alloy_transport::TransportError;
            type Future = alloy_transport::TransportFut<'static>;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: RequestPacket) -> Self::Future {
                let requests = self.requests.clone();
                Box::pin(async move {
                    let RequestPacket::Single(req) = req else {
                        panic!("burn() does not issue batch requests");
                    };
                    let id = req.id().clone();
                    requests.lock().expect("captured-request mutex").push(req);
                    // Echo back any well-formed tx hash; `burn()` only
                    // reads it via `pending_tx.tx_hash()` and returns it.
                    let hash_json =
                        "\"0x0101010101010101010101010101010101010101010101010101010101010101\"";
                    let raw = serde_json::value::RawValue::from_string(hash_json.to_string())
                        .expect("static tx-hash JSON parses");
                    Ok(ResponsePacket::Single(Response {
                        id,
                        payload: ResponsePayload::Success(raw),
                    }))
                })
            }
        }

        fn build_bridge(
            transfer_mode: TransferMode,
        ) -> (
            CctpV2<alloy_provider::RootProvider<Ethereum>>,
            Arc<Mutex<Vec<alloy_json_rpc::SerializedRequest>>>,
        ) {
            let transport = CapturingTransport::default();
            let captured = transport.requests.clone();
            // `disable_recommended_fillers` keeps the chainId / nonce /
            // gas RPCs from firing, so the only call we observe is the
            // `eth_sendTransaction` that carries the burn calldata —
            // exactly what this test wants to pin.
            let provider = ProviderBuilder::new()
                .disable_recommended_fillers()
                .connect_client(RpcClient::new(transport, true));
            let bridge = CctpV2::builder()
                .source_chain(NamedChain::Mainnet)
                .destination_chain(NamedChain::Linea)
                .source_provider(provider.clone())
                .destination_provider(provider)
                .recipient(Address::ZERO)
                .transfer_mode(transfer_mode)
                .build();
            (bridge, captured)
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum ExpectedCall {
            DepositForBurn,
            DepositForBurnWithHook,
        }

        #[rstest]
        #[case::standard(
            TransferMode::Standard,
            ExpectedCall::DepositForBurn,
            2000_u32,
            U256::ZERO,
            None
        )]
        #[case::fast(
            TransferMode::Fast { max_fee: U256::from(1500_u64) },
            ExpectedCall::DepositForBurn,
            1000_u32,
            U256::from(1500_u64),
            None,
        )]
        #[case::standard_with_hook(
            TransferMode::StandardWithHook { hook_data: Bytes::from_static(&[0xab, 0xcd]) },
            ExpectedCall::DepositForBurnWithHook,
            2000_u32,
            U256::ZERO,
            Some(Bytes::from_static(&[0xab, 0xcd])),
        )]
        #[case::fast_with_hook(
            TransferMode::FastWithHook {
                max_fee: U256::from(2500_u64),
                hook_data: Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef]),
            },
            ExpectedCall::DepositForBurnWithHook,
            1000_u32,
            U256::from(2500_u64),
            Some(Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef])),
        )]
        #[tokio::test]
        async fn burn_emits_expected_calldata(
            #[case] transfer_mode: TransferMode,
            #[case] expected_call: ExpectedCall,
            #[case] expected_finality_threshold: u32,
            #[case] expected_max_fee: U256,
            #[case] expected_hook_data: Option<Bytes>,
        ) {
            let (bridge, captured) = build_bridge(transfer_mode);
            let from = Address::repeat_byte(0xaa);
            let token = Address::repeat_byte(0xbb);
            let amount = U256::from(1_000_000_u64);
            let destination_domain = bridge.destination_domain_id().unwrap().as_u32();
            let recipient = *bridge.recipient();

            bridge
                .burn(amount, from, token)
                .await
                .expect("burn submits one eth_sendTransaction to the capturing transport");

            let requests = captured.lock().expect("captured-request mutex");
            assert_eq!(
                requests.len(),
                1,
                "burn() should issue exactly one RPC call when fillers are disabled"
            );
            assert_eq!(requests[0].method(), "eth_sendTransaction");

            let params = requests[0]
                .params()
                .expect("eth_sendTransaction request carries params");
            let (tx_request,): (TransactionRequest,) = serde_json::from_str(params.get())
                .expect("params decode as a single TransactionRequest");

            assert_eq!(
                tx_request.from,
                Some(from),
                "calldata must originate from the address the bridge was asked to burn for"
            );
            let calldata = tx_request
                .input
                .input()
                .expect("transaction request carries calldata");

            // `destinationCaller` is hard-coded to `Address::ZERO` so that any
            // attester can submit `receiveMessage` on the destination chain
            // (`src/contracts/v2/token_messenger_v2.rs:77,236`). A future
            // refactor that wires a real caller into the helpers would silently
            // lock burns to a single relayer — pinning the zero value here
            // makes that drift fail this test instead of stranding funds.
            let expected_destination_caller = Address::ZERO.into_word();

            match expected_call {
                ExpectedCall::DepositForBurn => {
                    let decoded = TokenMessengerV2::depositForBurnCall::abi_decode(calldata)
                        .expect("calldata decodes as depositForBurn");
                    assert_eq!(decoded.amount, amount);
                    assert_eq!(decoded.destinationDomain, destination_domain);
                    assert_eq!(decoded.mintRecipient, recipient.into_word());
                    assert_eq!(decoded.burnToken, token);
                    assert_eq!(decoded.destinationCaller, expected_destination_caller);
                    assert_eq!(decoded.maxFee, expected_max_fee);
                    assert_eq!(decoded.minFinalityThreshold, expected_finality_threshold);
                    assert!(
                        expected_hook_data.is_none(),
                        "no-hook variants must not carry hook data on the wire"
                    );
                }
                ExpectedCall::DepositForBurnWithHook => {
                    let decoded =
                        TokenMessengerV2::depositForBurnWithHookCall::abi_decode(calldata)
                            .expect("calldata decodes as depositForBurnWithHook");
                    assert_eq!(decoded.amount, amount);
                    assert_eq!(decoded.destinationDomain, destination_domain);
                    assert_eq!(decoded.mintRecipient, recipient.into_word());
                    assert_eq!(decoded.burnToken, token);
                    assert_eq!(decoded.destinationCaller, expected_destination_caller);
                    assert_eq!(decoded.maxFee, expected_max_fee);
                    assert_eq!(decoded.minFinalityThreshold, expected_finality_threshold);
                    assert_eq!(
                        decoded.hookData,
                        expected_hook_data.expect("hook variants carry hook data")
                    );
                }
            }
        }
    }

    mod wait_for_receive_timeout {
        //! Regression coverage for #217: when polling exhausts the
        //! attempt budget, `wait_for_receive` must return the receive-
        //! specific timeout variant. `AttestationTimeout` would be
        //! misleading — attestation is already complete by the time the
        //! caller is waiting on the destination receipt.
        use super::*;
        use alloy_json_rpc::{RequestPacket, Response, ResponsePacket, ResponsePayload};
        use alloy_provider::ProviderBuilder;
        use alloy_rpc_client::RpcClient;
        use std::sync::{Arc, Mutex};
        use std::task::{Context, Poll};
        use tower::Service;

        /// Tower service that responds to every `eth_call` with an ABI-
        /// encoded `false`, simulating a destination chain that never
        /// observes the message as received.
        #[derive(Clone, Default)]
        struct AlwaysFalseEthCallTransport {
            call_count: Arc<Mutex<u32>>,
        }

        impl Service<RequestPacket> for AlwaysFalseEthCallTransport {
            type Response = ResponsePacket;
            type Error = alloy_transport::TransportError;
            type Future = alloy_transport::TransportFut<'static>;

            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: RequestPacket) -> Self::Future {
                let call_count = self.call_count.clone();
                Box::pin(async move {
                    let RequestPacket::Single(req) = req else {
                        panic!("wait_for_receive does not issue batch requests");
                    };
                    let id = req.id().clone();
                    assert_eq!(
                        req.method(),
                        "eth_call",
                        "wait_for_receive's polling loop only issues eth_call",
                    );
                    *call_count.lock().expect("call-count mutex") += 1;
                    // `is_message_received` calls `usedNonces(bytes32) returns
                    // (uint256)` and interprets a non-zero return as "received".
                    // 32 zero bytes decodes to `U256::ZERO`, which the SDK
                    // maps to "not received" — exactly the state we want to
                    // pin so the polling loop runs to exhaustion.
                    let unused_nonce_json =
                        "\"0x0000000000000000000000000000000000000000000000000000000000000000\"";
                    let raw =
                        serde_json::value::RawValue::from_string(unused_nonce_json.to_string())
                            .expect("static unused-nonce JSON parses");
                    Ok(ResponsePacket::Single(Response {
                        id,
                        payload: ResponsePayload::Success(raw),
                    }))
                })
            }
        }

        #[tokio::test(flavor = "current_thread", start_paused = true)]
        async fn returns_receive_timeout_when_polling_exhausts() {
            let transport = AlwaysFalseEthCallTransport::default();
            let call_count = transport.call_count.clone();
            let provider = ProviderBuilder::new()
                .disable_recommended_fillers()
                .connect_client(RpcClient::new(transport, true));
            let bridge = CctpV2::builder()
                .source_chain(NamedChain::Mainnet)
                .destination_chain(NamedChain::Linea)
                .source_provider(provider.clone())
                .destination_provider(provider)
                .recipient(Address::ZERO)
                .build();

            let err = bridge
                .wait_for_receive(b"any message bytes", Some(3), Some(1))
                .await
                .expect_err("polling against a destination that never reports receipt must error");

            assert!(
                matches!(err, CctpError::ReceiveTimeout),
                "wait_for_receive must return ReceiveTimeout, not AttestationTimeout: {err:?}",
            );
            assert_eq!(
                *call_count.lock().expect("call-count mutex"),
                3,
                "polling must exhaust the configured attempt budget before timing out",
            );
        }
    }
}
