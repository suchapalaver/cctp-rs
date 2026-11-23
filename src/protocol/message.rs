// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 message format types
//!
//! Circle's CCTP v2 introduces a structured message format with headers and
//! typed body formats for different message types (burn messages, etc.).
//!
//! Reference: <https://developers.circle.com/cctp/technical-guide>

use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use serde::{Serialize, Serializer};
use thiserror::Error;

use super::DomainId;
use crate::FinalityThreshold;

const CCTP_V2_MESSAGE_VERSION: u32 = 1;
const CCTP_V2_BURN_BODY_VERSION: u32 = 1;

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn address_word(address: Address) -> FixedBytes<32> {
    address.into_word()
}

fn push_word(bytes: &mut Vec<u8>, word: FixedBytes<32>) {
    bytes.extend_from_slice(word.as_slice());
}

/// Decodes a canonical EVM address word.
///
/// Returns `None` unless `bytes` is exactly 32 bytes long and the leading 12
/// bytes are zero (the CCTP `bytes32` padding convention for EVM addresses).
/// Rejecting non-canonical words preserves the `parse(raw).encode() == raw`
/// invariant — otherwise stray leading bytes would be silently truncated on
/// decode and reintroduced as zeros on re-encode.
fn decode_address_word(bytes: &[u8]) -> Option<Address> {
    if bytes.len() != 32 {
        return None;
    }
    if !is_canonical_evm_address_word(bytes) {
        return None;
    }
    Some(Address::from_slice(&bytes[12..32]))
}

fn is_canonical_evm_address_word(bytes: &[u8]) -> bool {
    bytes.len() == 32 && bytes[..12].iter().all(|byte| *byte == 0)
}

/// Error returned when parsing a canonical CCTP v2 message fails.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[cfg_attr(test, derive(strum::EnumCount))]
#[non_exhaustive]
pub enum ParseMessageError {
    /// The input is shorter than the 148-byte message header.
    #[error(
        "invalid CCTP v2 message: header requires at least {minimum} bytes, got {actual}",
        minimum = MessageHeader::SIZE
    )]
    HeaderTooShort { actual: usize },
    /// The bytes following the header are shorter than the 228-byte burn body.
    #[error(
        "invalid CCTP v2 message: burn message body requires at least {minimum} bytes, got {actual}",
        minimum = BurnMessageV2::MIN_SIZE
    )]
    BurnBodyTooShort { actual: usize },
    /// The message header uses a wire version this parser does not understand.
    #[error(
        "invalid CCTP v2 message: unsupported CCTP v2 message header version {version}; supported version is {supported}",
        supported = CCTP_V2_MESSAGE_VERSION
    )]
    UnsupportedHeaderVersion { version: u32 },
    /// The burn body uses a wire version this parser does not understand.
    #[error(
        "invalid CCTP v2 message: unsupported CCTP v2 burn message body version {version}; supported version is {supported}",
        supported = CCTP_V2_BURN_BODY_VERSION
    )]
    UnsupportedBurnBodyVersion { version: u32 },
    /// The header's source-domain word is not in the modeled CCTP domain table.
    #[error("invalid CCTP v2 message: unknown source domain ID {domain}")]
    UnknownSourceDomain { domain: u32 },
    /// The header's destination-domain word is not in the modeled CCTP domain table.
    #[error("invalid CCTP v2 message: unknown destination domain ID {domain}")]
    UnknownDestinationDomain { domain: u32 },
    /// An EVM-source burn-token word is not canonically left-padded.
    #[error(
        "invalid CCTP v2 message: burn_token word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes"
    )]
    NonCanonicalBurnToken,
    /// An EVM-destination mint-recipient word is not canonically left-padded.
    #[error(
        "invalid CCTP v2 message: mint_recipient word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes"
    )]
    NonCanonicalMintRecipient,
    /// An EVM-source message-sender word is not canonically left-padded.
    #[error(
        "invalid CCTP v2 message: message_sender word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes"
    )]
    NonCanonicalMessageSender,
}

/// CCTP v2 Message Header
///
/// The message header contains metadata about cross-chain messages,
/// including source/destination domains, finality requirements, and routing.
///
/// # Format
///
/// - version: uint32 (4 bytes)
/// - sourceDomain: uint32 (4 bytes)
/// - destinationDomain: uint32 (4 bytes)
/// - nonce: bytes32 (32 bytes) - unique identifier assigned by Circle
/// - sender: bytes32 (32 bytes) - message sender address
/// - recipient: bytes32 (32 bytes) - message recipient address
/// - destinationCaller: bytes32 (32 bytes) - authorized caller on destination
/// - minFinalityThreshold: uint32 (4 bytes) - minimum required finality
/// - finalityThresholdExecuted: uint32 (4 bytes) - actual finality level
///
/// Total fixed size: 4 + 4 + 4 + 32 + 32 + 32 + 32 + 4 + 4 = 148 bytes
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageHeader {
    /// Message format version
    version: u32,
    /// Source blockchain domain ID
    source_domain: DomainId,
    /// Destination blockchain domain ID
    destination_domain: DomainId,
    /// Unique message nonce assigned by Circle
    nonce: FixedBytes<32>,
    /// Address that sent the message (padded to 32 bytes)
    sender: FixedBytes<32>,
    /// Address that will receive the message (padded to 32 bytes)
    recipient: FixedBytes<32>,
    /// Address authorized to call receiveMessage on destination (0 = anyone)
    destination_caller: FixedBytes<32>,
    /// Minimum finality threshold required (1000 = Fast, 2000 = Standard)
    min_finality_threshold: u32,
    /// Actual finality threshold when message was attested
    finality_threshold_executed: u32,
}

impl MessageHeader {
    /// Size of the message header in bytes
    pub const SIZE: usize = 148;
    /// The only CCTP v2 message header version this parser understands.
    pub const SUPPORTED_VERSION: u32 = CCTP_V2_MESSAGE_VERSION;

    /// Creates a new header using the only supported wire version.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_domain: DomainId,
        destination_domain: DomainId,
        nonce: FixedBytes<32>,
        sender: FixedBytes<32>,
        recipient: FixedBytes<32>,
        destination_caller: FixedBytes<32>,
        min_finality_threshold: u32,
        finality_threshold_executed: u32,
    ) -> Self {
        Self {
            version: Self::SUPPORTED_VERSION,
            source_domain,
            destination_domain,
            nonce,
            sender,
            recipient,
            destination_caller,
            min_finality_threshold,
            finality_threshold_executed,
        }
    }

    /// Returns the validated message format version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the source blockchain domain.
    #[must_use]
    pub const fn source_domain(&self) -> DomainId {
        self.source_domain
    }

    /// Returns the destination blockchain domain.
    #[must_use]
    pub const fn destination_domain(&self) -> DomainId {
        self.destination_domain
    }

    /// Returns the canonical 32-byte nonce.
    #[must_use]
    pub const fn nonce(&self) -> FixedBytes<32> {
        self.nonce
    }

    /// Returns the canonical 32-byte sender word.
    #[must_use]
    pub const fn sender(&self) -> FixedBytes<32> {
        self.sender
    }

    /// Returns the canonical 32-byte recipient word.
    #[must_use]
    pub const fn recipient(&self) -> FixedBytes<32> {
        self.recipient
    }

    /// Returns the canonical 32-byte destination-caller word.
    #[must_use]
    pub const fn destination_caller(&self) -> FixedBytes<32> {
        self.destination_caller
    }

    /// Returns the requested minimum finality threshold as its wire value.
    #[must_use]
    pub const fn min_finality_threshold(&self) -> u32 {
        self.min_finality_threshold
    }

    /// Returns the executed finality threshold as its wire value.
    #[must_use]
    pub const fn finality_threshold_executed(&self) -> u32 {
        self.finality_threshold_executed
    }

    /// Encodes the message header to bytes
    ///
    /// The encoding follows Circle's v2 message format specification.
    pub fn encode(&self) -> Bytes {
        let mut bytes = Vec::with_capacity(Self::SIZE);

        // version (4 bytes)
        bytes.extend_from_slice(&self.version.to_be_bytes());
        // sourceDomain (4 bytes)
        bytes.extend_from_slice(&self.source_domain.as_u32().to_be_bytes());
        // destinationDomain (4 bytes)
        bytes.extend_from_slice(&self.destination_domain.as_u32().to_be_bytes());
        // nonce (32 bytes)
        bytes.extend_from_slice(self.nonce.as_slice());
        // sender (32 bytes)
        bytes.extend_from_slice(self.sender.as_slice());
        // recipient (32 bytes)
        bytes.extend_from_slice(self.recipient.as_slice());
        // destinationCaller (32 bytes)
        bytes.extend_from_slice(self.destination_caller.as_slice());
        // minFinalityThreshold (4 bytes)
        bytes.extend_from_slice(&self.min_finality_threshold.to_be_bytes());
        // finalityThresholdExecuted (4 bytes)
        bytes.extend_from_slice(&self.finality_threshold_executed.to_be_bytes());

        Bytes::from(bytes)
    }

    /// Parses a message header and returns a descriptive error on failure.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        if bytes.len() < Self::SIZE {
            return Err(ParseMessageError::HeaderTooShort {
                actual: bytes.len(),
            });
        }

        let version = read_u32(bytes, 0);
        if version != Self::SUPPORTED_VERSION {
            return Err(ParseMessageError::UnsupportedHeaderVersion { version });
        }

        let source_domain_id = read_u32(bytes, 4);
        let source_domain =
            DomainId::from_u32(source_domain_id).ok_or(ParseMessageError::UnknownSourceDomain {
                domain: source_domain_id,
            })?;
        let destination_domain_id = read_u32(bytes, 8);
        let destination_domain = DomainId::from_u32(destination_domain_id).ok_or(
            ParseMessageError::UnknownDestinationDomain {
                domain: destination_domain_id,
            },
        )?;

        Ok(Self {
            version,
            source_domain,
            destination_domain,
            nonce: FixedBytes::from_slice(&bytes[12..44]),
            sender: FixedBytes::from_slice(&bytes[44..76]),
            recipient: FixedBytes::from_slice(&bytes[76..108]),
            destination_caller: FixedBytes::from_slice(&bytes[108..140]),
            min_finality_threshold: read_u32(bytes, 140),
            finality_threshold_executed: read_u32(bytes, 144),
        })
    }

    /// Returns true when the nonce is still the placeholder zero value from the on-chain event.
    pub fn has_placeholder_nonce(&self) -> bool {
        self.nonce.as_slice().iter().all(|byte| *byte == 0)
    }

    /// Returns the sender as an EVM `Address` when the source domain is EVM.
    ///
    /// Returns `None` for non-EVM domains such as [`DomainId::Solana`] or
    /// [`DomainId::StarknetTestnet`], whose `bytes32` sender words do not use
    /// the EVM trailing-20-byte convention. For those domains, the raw
    /// [`Self::sender`] field is the canonical source of truth.
    #[must_use]
    pub fn sender_address(&self) -> Option<Address> {
        self.source_domain
            .is_evm()
            .then(|| decode_address_word(self.sender.as_slice()))
            .flatten()
    }

    /// Returns the recipient as an EVM `Address` when the destination domain is EVM.
    ///
    /// Returns `None` for non-EVM destination domains. For those, the raw
    /// [`Self::recipient`] field is the canonical source of truth.
    #[must_use]
    pub fn recipient_address(&self) -> Option<Address> {
        self.destination_domain
            .is_evm()
            .then(|| decode_address_word(self.recipient.as_slice()))
            .flatten()
    }

    /// Returns the destination caller as an EVM `Address` when one is set and
    /// the destination domain is EVM.
    ///
    /// Returns `None` when the message is permissionless or the destination
    /// domain is non-EVM. The raw [`Self::destination_caller`] field is
    /// authoritative in the non-EVM case.
    #[must_use]
    pub fn destination_caller_address(&self) -> Option<Address> {
        if self.is_permissionless() || !self.destination_domain.is_evm() {
            return None;
        }
        decode_address_word(self.destination_caller.as_slice())
    }

    /// Returns true when the message can be relayed by anyone.
    pub fn is_permissionless(&self) -> bool {
        self.destination_caller
            .as_slice()
            .iter()
            .all(|byte| *byte == 0)
    }

    /// Returns the requested finality threshold when it matches a known CCTP mode.
    #[must_use]
    pub fn requested_finality(&self) -> Option<FinalityThreshold> {
        FinalityThreshold::from_u32(self.min_finality_threshold)
    }

    /// Returns true when the message requests Fast Transfer finality.
    ///
    /// The header's requested finality is authoritative for transfer kind.
    /// A nonzero burn-body `maxFee` is not sufficient evidence because
    /// Standard Transfer fee-switch messages may also carry a fee cap.
    #[must_use]
    pub fn is_fast_transfer(&self) -> bool {
        matches!(self.requested_finality(), Some(FinalityThreshold::Fast))
    }

    /// Returns the finality threshold that Circle actually used for the attestation.
    #[must_use]
    pub fn attested_finality(&self) -> Option<FinalityThreshold> {
        FinalityThreshold::from_u32(self.finality_threshold_executed)
    }
}

/// CCTP v2 Burn Message Body
///
/// The burn message body contains information about a token burn operation
/// for cross-chain USDC transfers.
///
/// # Format
///
/// - version: uint32 (4 bytes)
/// - burnToken: bytes32 (32 bytes) - address of token being burned
/// - mintRecipient: bytes32 (32 bytes) - address to receive minted tokens
/// - amount: uint256 (32 bytes) - amount being transferred
/// - messageSender: bytes32 (32 bytes) - original sender address
/// - maxFee: uint256 (32 bytes) - maximum fee willing to pay
/// - feeExecuted: uint256 (32 bytes) - actual fee charged
/// - expirationBlock: uint256 (32 bytes) - block number when message expires
/// - hookData: dynamic bytes - arbitrary data for destination chain hooks
///
/// Total fixed size: 4 + 32 + 32 + 32 + 32 + 32 + 32 + 32 = 228 bytes + dynamic hookData
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BurnMessageV2 {
    /// Message body version
    version: u32,
    /// Canonical 32-byte token word from the source domain.
    burn_token: FixedBytes<32>,
    /// Canonical 32-byte recipient word for the destination domain.
    mint_recipient: FixedBytes<32>,
    /// Amount of tokens being transferred (in wei/smallest unit)
    amount: U256,
    /// Canonical 32-byte sender word from the source domain.
    message_sender: FixedBytes<32>,
    /// Maximum fee the sender is willing to pay.
    ///
    /// This is independent of requested finality: Fast Transfer and
    /// fee-switch Standard Transfer messages may both carry a fee cap.
    max_fee: U256,
    /// Actual fee that was charged
    fee_executed: U256,
    /// Block number after which the message expires (anti-replay protection)
    expiration_block: U256,
    /// Optional hook data for programmable transfers
    hook_data: Bytes,
}

impl BurnMessageV2 {
    /// Minimum size of the burn message body in bytes (without hookData)
    pub const MIN_SIZE: usize = 228;
    /// The only CCTP v2 burn body version this parser understands.
    pub const SUPPORTED_VERSION: u32 = CCTP_V2_BURN_BODY_VERSION;

    /// Creates a version-1 burn body from canonical wire words.
    ///
    /// The address-like words are intentionally not projected to EVM
    /// addresses because their meaning depends on the containing header's
    /// domains. [`ParsedV2Message::new`] performs that domain-aware check.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw_words(
        burn_token: FixedBytes<32>,
        mint_recipient: FixedBytes<32>,
        amount: U256,
        message_sender: FixedBytes<32>,
        max_fee: U256,
        fee_executed: U256,
        expiration_block: U256,
        hook_data: Bytes,
    ) -> Self {
        Self {
            version: Self::SUPPORTED_VERSION,
            burn_token,
            mint_recipient,
            amount,
            message_sender,
            max_fee,
            fee_executed,
            expiration_block,
            hook_data,
        }
    }

    /// Returns the validated burn-body version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the canonical 32-byte burn-token word.
    #[must_use]
    pub const fn burn_token(&self) -> FixedBytes<32> {
        self.burn_token
    }

    /// Returns the canonical 32-byte mint-recipient word.
    #[must_use]
    pub const fn mint_recipient(&self) -> FixedBytes<32> {
        self.mint_recipient
    }

    /// Returns the transfer amount in atomic units.
    #[must_use]
    pub const fn amount(&self) -> U256 {
        self.amount
    }

    /// Returns the canonical 32-byte message-sender word.
    #[must_use]
    pub const fn message_sender(&self) -> FixedBytes<32> {
        self.message_sender
    }

    /// Returns the maximum fee cap.
    #[must_use]
    pub const fn max_fee(&self) -> U256 {
        self.max_fee
    }

    /// Returns the executed fee.
    #[must_use]
    pub const fn fee_executed(&self) -> U256 {
        self.fee_executed
    }

    /// Returns the expiration block.
    #[must_use]
    pub const fn expiration_block(&self) -> U256 {
        self.expiration_block
    }

    /// Returns the hook data.
    #[must_use]
    pub const fn hook_data(&self) -> &Bytes {
        &self.hook_data
    }

    /// Creates a new burn message with standard settings (no fast transfer, no hooks)
    pub fn new(
        burn_token: Address,
        mint_recipient: Address,
        amount: U256,
        message_sender: Address,
    ) -> Self {
        Self {
            version: 1,
            burn_token: address_word(burn_token),
            mint_recipient: address_word(mint_recipient),
            amount,
            message_sender: address_word(message_sender),
            max_fee: U256::ZERO,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data: Bytes::new(),
        }
    }

    /// Creates a new burn message body with a fee cap.
    ///
    /// The containing [`MessageHeader`] determines whether the full message
    /// requests Fast or Standard finality.
    pub fn new_with_max_fee(
        burn_token: Address,
        mint_recipient: Address,
        amount: U256,
        message_sender: Address,
        max_fee: U256,
    ) -> Self {
        Self {
            version: 1,
            burn_token: address_word(burn_token),
            mint_recipient: address_word(mint_recipient),
            amount,
            message_sender: address_word(message_sender),
            max_fee,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data: Bytes::new(),
        }
    }

    /// Compatibility constructor retaining the legacy name.
    ///
    /// This only sets the burn body's fee cap; it cannot select Fast Transfer
    /// without a containing header that requests Fast finality. New code
    /// should prefer [`Self::new_with_max_fee`].
    pub fn new_with_fast_transfer(
        burn_token: Address,
        mint_recipient: Address,
        amount: U256,
        message_sender: Address,
        max_fee: U256,
    ) -> Self {
        Self::new_with_max_fee(burn_token, mint_recipient, amount, message_sender, max_fee)
    }

    /// Creates a new burn message with hook data
    pub fn new_with_hooks(
        burn_token: Address,
        mint_recipient: Address,
        amount: U256,
        message_sender: Address,
        hook_data: Bytes,
    ) -> Self {
        Self {
            version: 1,
            burn_token: address_word(burn_token),
            mint_recipient: address_word(mint_recipient),
            amount,
            message_sender: address_word(message_sender),
            max_fee: U256::ZERO,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data,
        }
    }

    /// Sets the hook data for this message
    pub fn with_hook_data(mut self, hook_data: Bytes) -> Self {
        self.hook_data = hook_data;
        self
    }

    /// Sets the maximum fee cap without changing the containing header's finality.
    pub fn with_max_fee(mut self, max_fee: U256) -> Self {
        self.max_fee = max_fee;
        self
    }

    /// Sets the expiration block
    pub fn with_expiration_block(mut self, expiration_block: U256) -> Self {
        self.expiration_block = expiration_block;
        self
    }

    /// Returns `burn_token` as an EVM address when its word is canonically padded.
    #[must_use]
    pub fn burn_token_address(&self) -> Option<Address> {
        decode_address_word(self.burn_token.as_slice())
    }

    /// Returns `mint_recipient` as an EVM address when its word is canonically padded.
    #[must_use]
    pub fn mint_recipient_address(&self) -> Option<Address> {
        decode_address_word(self.mint_recipient.as_slice())
    }

    /// Returns `message_sender` as an EVM address when its word is canonically padded.
    #[must_use]
    pub fn message_sender_address(&self) -> Option<Address> {
        decode_address_word(self.message_sender.as_slice())
    }

    /// Encodes the burn message body to bytes.
    pub fn encode(&self) -> Bytes {
        let mut bytes = Vec::with_capacity(Self::MIN_SIZE + self.hook_data.len());

        bytes.extend_from_slice(&self.version.to_be_bytes());
        push_word(&mut bytes, self.burn_token);
        push_word(&mut bytes, self.mint_recipient);
        bytes.extend_from_slice(&self.amount.to_be_bytes::<32>());
        push_word(&mut bytes, self.message_sender);
        bytes.extend_from_slice(&self.max_fee.to_be_bytes::<32>());
        bytes.extend_from_slice(&self.fee_executed.to_be_bytes::<32>());
        bytes.extend_from_slice(&self.expiration_block.to_be_bytes::<32>());
        bytes.extend_from_slice(&self.hook_data);

        Bytes::from(bytes)
    }

    /// Parses a burn message body and returns a descriptive error on failure.
    ///
    /// Body-only parsing has no source/destination domain context, so it
    /// preserves the three address-like fields as raw words. Use
    /// [`ParsedV2Message::parse`] when domain-aware EVM padding validation is
    /// required.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        if bytes.len() < Self::MIN_SIZE {
            return Err(ParseMessageError::BurnBodyTooShort {
                actual: bytes.len(),
            });
        }

        let version = read_u32(bytes, 0);
        if version != Self::SUPPORTED_VERSION {
            return Err(ParseMessageError::UnsupportedBurnBodyVersion { version });
        }

        Ok(Self {
            version,
            burn_token: FixedBytes::from_slice(&bytes[4..36]),
            mint_recipient: FixedBytes::from_slice(&bytes[36..68]),
            amount: U256::from_be_slice(&bytes[68..100]),
            message_sender: FixedBytes::from_slice(&bytes[100..132]),
            max_fee: U256::from_be_slice(&bytes[132..164]),
            fee_executed: U256::from_be_slice(&bytes[164..196]),
            expiration_block: U256::from_be_slice(&bytes[196..228]),
            hook_data: Bytes::copy_from_slice(&bytes[228..]),
        })
    }

    /// Returns true if this message has hook data
    pub fn has_hooks(&self) -> bool {
        !self.hook_data.is_empty()
    }

    /// Returns true when this body carries a nonzero fee cap.
    ///
    /// Fee presence does not identify the transfer's finality mode: Fast
    /// Transfer can use a zero cap, while Standard Transfer fee-switch
    /// messages can use a nonzero cap. Inspect the containing
    /// [`ParsedV2Message`] or [`MessageHeader`] for transfer kind.
    #[must_use]
    pub fn has_nonzero_max_fee(&self) -> bool {
        self.max_fee > U256::ZERO
    }

    /// Compatibility alias for [`Self::has_nonzero_max_fee`].
    ///
    /// Despite its legacy name, this method reports only fee presence and does
    /// **not** reliably identify Fast Transfer. Use
    /// [`ParsedV2Message::is_fast_transfer`] or
    /// [`MessageHeader::is_fast_transfer`] instead.
    pub fn is_fast_transfer(&self) -> bool {
        self.has_nonzero_max_fee()
    }
}

/// Parsed representation of a canonical CCTP v2 transfer message.
///
/// This combines the fixed-size message header with the burn message body and
/// can be serialized directly for agent or tool responses. Its fields are
/// private so a value accepted by the strict parser remains valid for its
/// entire lifetime.
///
/// ```compile_fail
/// fn replace_header(message: &mut cctp_rs::ParsedV2Message) {
///     message.header = message.header().clone();
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedV2Message {
    header: MessageHeader,
    body: BurnMessageV2,
}

impl ParsedV2Message {
    /// Creates a parsed message from validated component values.
    ///
    /// EVM-domain body words must use canonical 12-zero-byte padding. Raw
    /// words for non-EVM domains are preserved without projection.
    pub fn new(
        header: MessageHeader,
        body: BurnMessageV2,
    ) -> std::result::Result<Self, ParseMessageError> {
        if header.source_domain.is_evm()
            && !is_canonical_evm_address_word(body.burn_token.as_slice())
        {
            return Err(ParseMessageError::NonCanonicalBurnToken);
        }
        if header.destination_domain.is_evm()
            && !is_canonical_evm_address_word(body.mint_recipient.as_slice())
        {
            return Err(ParseMessageError::NonCanonicalMintRecipient);
        }
        if header.source_domain.is_evm()
            && !is_canonical_evm_address_word(body.message_sender.as_slice())
        {
            return Err(ParseMessageError::NonCanonicalMessageSender);
        }
        Ok(Self { header, body })
    }

    /// Returns the validated message header.
    #[must_use]
    pub const fn header(&self) -> &MessageHeader {
        &self.header
    }

    /// Returns the validated burn-message body.
    #[must_use]
    pub const fn body(&self) -> &BurnMessageV2 {
        &self.body
    }

    /// Encodes the full CCTP v2 message.
    pub fn encode(&self) -> Bytes {
        let mut bytes = self.header.encode().to_vec();
        bytes.extend_from_slice(&self.body.encode());
        Bytes::from(bytes)
    }

    /// Parses a canonical CCTP v2 burn-transfer message and returns a
    /// descriptive error on failure.
    ///
    /// Strict parser: every accepted input round-trips byte-for-byte through
    /// [`Self::encode`] and hashes to `keccak256(raw)` via [`Self::message_hash`].
    /// EVM-domain body address words must use the canonical 12-zero-byte
    /// padding convention; non-EVM-domain body words are preserved as raw
    /// `bytes32` values. For a JSON-friendly view of the parsed message, call
    /// [`Self::summary`].
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        let header = MessageHeader::parse(bytes)?;
        let body = BurnMessageV2::parse(&bytes[MessageHeader::SIZE..])?;
        Self::new(header, body)
    }

    /// Returns the keccak256 message hash used by the destination contract.
    #[must_use]
    pub fn message_hash(&self) -> FixedBytes<32> {
        alloy_primitives::keccak256(self.encode())
    }

    /// Returns true when the header requests Fast Transfer finality.
    ///
    /// This deliberately ignores `body.max_fee`, which is an independent fee
    /// cap used by both Fast Transfer and fee-switch Standard Transfer modes.
    #[must_use]
    pub fn is_fast_transfer(&self) -> bool {
        self.header.is_fast_transfer()
    }

    /// Returns a compact summary that is convenient to serialize from tools.
    ///
    /// The canonical `bytes32` header fields are exposed as `sender_bytes`,
    /// `recipient_bytes`, and `destination_caller_bytes` and are always
    /// populated. The EVM-interpreted `sender`, `recipient`, and
    /// `destination_caller` fields are populated only when the corresponding
    /// domain is EVM ([`DomainId::is_evm`]); for non-EVM domains they are
    /// `None` so consumers do not mistake a misleading trailing-20-byte
    /// projection for the authoritative value.
    #[must_use]
    pub fn summary(&self) -> ParsedV2MessageSummary {
        ParsedV2MessageSummary {
            message: self.clone(),
        }
    }
}

/// JSON-friendly summary of a canonical CCTP v2 transfer message.
///
/// `DomainId` values serialize as `snake_case` strings. Future crate releases
/// may add new domain variants, so JSON consumers should treat those values as
/// non-exhaustive.
///
/// # Address fields and non-EVM domains
///
/// The `*_bytes` fields (`sender_bytes`, `recipient_bytes`,
/// `destination_caller_bytes`) carry the canonical 32-byte header words and
/// are always populated. The EVM-shaped fields (`sender`, `recipient`,
/// `destination_caller`) are populated only when the corresponding domain is
/// EVM ([`DomainId::is_evm`]); for non-EVM domains such as
/// [`DomainId::Solana`], [`DomainId::Sui`], [`DomainId::Aptos`],
/// [`DomainId::StarknetTestnet`], or [`DomainId::Stellar`], they are `None`
/// because a trailing-20-byte
/// projection would be misleading.
///
/// The same pattern applies to burn-body words: `burn_token_bytes`,
/// `mint_recipient_bytes`, and `message_sender_bytes` always carry the
/// canonical 32-byte wire values, while the EVM-shaped address projections are
/// populated only for the source or destination domains where that projection
/// is meaningful.
///
/// Summaries are output-only projections. Deserialize canonical message bytes
/// with [`Self::parse`] instead of accepting redundant JSON fields that may
/// contradict one another.
///
/// ```compile_fail
/// let _: cctp_rs::ParsedV2MessageSummary =
///     serde_json::from_str("{}").expect("summaries are not deserializable");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedV2MessageSummary {
    message: ParsedV2Message,
}

impl ParsedV2MessageSummary {
    /// Parses and summarizes a canonical CCTP v2 transfer message.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        ParsedV2Message::parse(bytes).map(|message| message.summary())
    }

    /// Returns the canonical parsed message backing this projection.
    #[must_use]
    pub const fn message(&self) -> &ParsedV2Message {
        &self.message
    }

    /// Returns the keccak256 hash of the canonical message bytes.
    #[must_use]
    pub fn message_hash(&self) -> FixedBytes<32> {
        self.message.message_hash()
    }

    /// Returns the canonical message length in bytes.
    #[must_use]
    pub fn message_len_bytes(&self) -> usize {
        self.message.encode().len()
    }

    /// Returns the source blockchain domain.
    #[must_use]
    pub const fn source_domain(&self) -> DomainId {
        self.message.header.source_domain
    }

    /// Returns the destination blockchain domain.
    #[must_use]
    pub const fn destination_domain(&self) -> DomainId {
        self.message.header.destination_domain
    }

    /// Returns the validated message-header version.
    #[must_use]
    pub const fn message_version(&self) -> u32 {
        self.message.header.version
    }

    /// Returns the validated burn-body version.
    #[must_use]
    pub const fn body_version(&self) -> u32 {
        self.message.body.version
    }

    /// Returns the canonical 32-byte nonce.
    #[must_use]
    pub const fn nonce(&self) -> FixedBytes<32> {
        self.message.header.nonce
    }

    /// Returns whether the nonce is the zero placeholder from an on-chain event.
    #[must_use]
    pub fn has_placeholder_nonce(&self) -> bool {
        self.message.header.has_placeholder_nonce()
    }

    /// Returns the canonical 32-byte sender word.
    #[must_use]
    pub const fn sender_bytes(&self) -> FixedBytes<32> {
        self.message.header.sender
    }

    /// Returns the sender as an EVM address when that projection is valid.
    #[must_use]
    pub fn sender(&self) -> Option<Address> {
        self.message.header.sender_address()
    }

    /// Returns the canonical 32-byte recipient word.
    #[must_use]
    pub const fn recipient_bytes(&self) -> FixedBytes<32> {
        self.message.header.recipient
    }

    /// Returns the recipient as an EVM address when that projection is valid.
    #[must_use]
    pub fn recipient(&self) -> Option<Address> {
        self.message.header.recipient_address()
    }

    /// Returns the canonical 32-byte destination-caller word.
    #[must_use]
    pub const fn destination_caller_bytes(&self) -> FixedBytes<32> {
        self.message.header.destination_caller
    }

    /// Returns the destination caller as an EVM address when one is set and valid.
    #[must_use]
    pub fn destination_caller(&self) -> Option<Address> {
        self.message.header.destination_caller_address()
    }

    /// Returns whether any relayer may submit the message.
    #[must_use]
    pub fn permissionless_relay(&self) -> bool {
        self.message.header.is_permissionless()
    }

    /// Returns the requested finality when it is a known CCTP threshold.
    #[must_use]
    pub fn requested_finality(&self) -> Option<FinalityThreshold> {
        self.message.header.requested_finality()
    }

    /// Returns the executed finality when it is a known CCTP threshold.
    #[must_use]
    pub fn attested_finality(&self) -> Option<FinalityThreshold> {
        self.message.header.attested_finality()
    }

    /// Returns the canonical 32-byte burn-token word.
    #[must_use]
    pub const fn burn_token_bytes(&self) -> FixedBytes<32> {
        self.message.body.burn_token
    }

    /// Returns the burn token as an EVM address when that projection is valid.
    #[must_use]
    pub fn burn_token(&self) -> Option<Address> {
        self.source_domain()
            .is_evm()
            .then(|| self.message.body.burn_token_address())
            .flatten()
    }

    /// Returns the canonical 32-byte mint-recipient word.
    #[must_use]
    pub const fn mint_recipient_bytes(&self) -> FixedBytes<32> {
        self.message.body.mint_recipient
    }

    /// Returns the mint recipient as an EVM address when that projection is valid.
    #[must_use]
    pub fn mint_recipient(&self) -> Option<Address> {
        self.destination_domain()
            .is_evm()
            .then(|| self.message.body.mint_recipient_address())
            .flatten()
    }

    /// Returns the transfer amount in atomic units.
    #[must_use]
    pub const fn amount(&self) -> U256 {
        self.message.body.amount
    }

    /// Returns the canonical 32-byte message-sender word.
    #[must_use]
    pub const fn message_sender_bytes(&self) -> FixedBytes<32> {
        self.message.body.message_sender
    }

    /// Returns the message sender as an EVM address when that projection is valid.
    #[must_use]
    pub fn message_sender(&self) -> Option<Address> {
        self.source_domain()
            .is_evm()
            .then(|| self.message.body.message_sender_address())
            .flatten()
    }

    /// Returns the maximum fee cap.
    #[must_use]
    pub const fn max_fee(&self) -> U256 {
        self.message.body.max_fee
    }

    /// Returns the fee executed by Circle.
    #[must_use]
    pub const fn fee_executed(&self) -> U256 {
        self.message.body.fee_executed
    }

    /// Returns the expiration block.
    #[must_use]
    pub const fn expiration_block(&self) -> U256 {
        self.message.body.expiration_block
    }

    /// Returns the canonical hook data.
    #[must_use]
    pub const fn hook_data(&self) -> &Bytes {
        &self.message.body.hook_data
    }

    /// Returns the hook-data length in bytes.
    #[must_use]
    pub fn hook_data_len_bytes(&self) -> usize {
        self.hook_data().len()
    }

    /// Returns whether the message carries hook data.
    #[must_use]
    pub fn has_hooks(&self) -> bool {
        self.message.body.has_hooks()
    }

    /// Returns whether the header requests Fast Transfer finality.
    #[must_use]
    pub fn is_fast_transfer(&self) -> bool {
        self.message.is_fast_transfer()
    }
}

#[derive(Serialize)]
struct ParsedV2MessageSummaryRef<'a> {
    message_hash: FixedBytes<32>,
    message_len_bytes: usize,
    source_domain: DomainId,
    destination_domain: DomainId,
    message_version: u32,
    body_version: u32,
    nonce: FixedBytes<32>,
    has_placeholder_nonce: bool,
    sender_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sender: Option<Address>,
    recipient_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipient: Option<Address>,
    destination_caller_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destination_caller: Option<Address>,
    permissionless_relay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_finality: Option<FinalityThreshold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attested_finality: Option<FinalityThreshold>,
    burn_token_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    burn_token: Option<Address>,
    mint_recipient_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mint_recipient: Option<Address>,
    amount: U256,
    message_sender_bytes: FixedBytes<32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_sender: Option<Address>,
    max_fee: U256,
    fee_executed: U256,
    expiration_block: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    hook_data: Option<&'a Bytes>,
    hook_data_len_bytes: usize,
    has_hooks: bool,
    is_fast_transfer: bool,
}

impl Serialize for ParsedV2MessageSummary {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = self.message.encode();
        ParsedV2MessageSummaryRef {
            message_hash: alloy_primitives::keccak256(&encoded),
            message_len_bytes: encoded.len(),
            source_domain: self.source_domain(),
            destination_domain: self.destination_domain(),
            message_version: self.message_version(),
            body_version: self.body_version(),
            nonce: self.nonce(),
            has_placeholder_nonce: self.has_placeholder_nonce(),
            sender_bytes: self.sender_bytes(),
            sender: self.sender(),
            recipient_bytes: self.recipient_bytes(),
            recipient: self.recipient(),
            destination_caller_bytes: self.destination_caller_bytes(),
            destination_caller: self.destination_caller(),
            permissionless_relay: self.permissionless_relay(),
            requested_finality: self.requested_finality(),
            attested_finality: self.attested_finality(),
            burn_token_bytes: self.burn_token_bytes(),
            burn_token: self.burn_token(),
            mint_recipient_bytes: self.mint_recipient_bytes(),
            mint_recipient: self.mint_recipient(),
            amount: self.amount(),
            message_sender_bytes: self.message_sender_bytes(),
            message_sender: self.message_sender(),
            max_fee: self.max_fee(),
            fee_executed: self.fee_executed(),
            expiration_block: self.expiration_block(),
            hook_data: self.has_hooks().then(|| self.hook_data()),
            hook_data_len_bytes: self.hook_data_len_bytes(),
            has_hooks: self.has_hooks(),
            is_fast_transfer: self.is_fast_transfer(),
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, hex};

    use crate::error::assert_complete_rendering_cases;

    #[test]
    fn test_message_header_size() {
        assert_eq!(MessageHeader::SIZE, 148);
    }

    #[test]
    fn test_parse_message_errors_render_structured_context() {
        let cases = [
            (
                ParseMessageError::HeaderTooShort { actual: 12 },
                "invalid CCTP v2 message: header requires at least 148 bytes, got 12",
            ),
            (
                ParseMessageError::BurnBodyTooShort { actual: 34 },
                "invalid CCTP v2 message: burn message body requires at least 228 bytes, got 34",
            ),
            (
                ParseMessageError::UnsupportedHeaderVersion { version: 2 },
                "invalid CCTP v2 message: unsupported CCTP v2 message header version 2; supported version is 1",
            ),
            (
                ParseMessageError::UnsupportedBurnBodyVersion { version: 9 },
                "invalid CCTP v2 message: unsupported CCTP v2 burn message body version 9; supported version is 1",
            ),
            (
                ParseMessageError::UnknownSourceDomain { domain: 999 },
                "invalid CCTP v2 message: unknown source domain ID 999",
            ),
            (
                ParseMessageError::UnknownDestinationDomain { domain: 998 },
                "invalid CCTP v2 message: unknown destination domain ID 998",
            ),
            (
                ParseMessageError::NonCanonicalBurnToken,
                "invalid CCTP v2 message: burn_token word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes",
            ),
            (
                ParseMessageError::NonCanonicalMintRecipient,
                "invalid CCTP v2 message: mint_recipient word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes",
            ),
            (
                ParseMessageError::NonCanonicalMessageSender,
                "invalid CCTP v2 message: message_sender word has non-zero leading bytes; canonical CCTP v2 address words must be zero-padded in the first 12 bytes",
            ),
        ];

        assert_complete_rendering_cases(&cases);
        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn test_message_header_encode_parse() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Arbitrum,
            FixedBytes::from([1u8; 32]),
            FixedBytes::from([2u8; 32]),
            FixedBytes::from([3u8; 32]),
            FixedBytes::from([0u8; 32]),
            1000,
            1000,
        );

        let encoded = header.encode();
        assert_eq!(encoded.len(), MessageHeader::SIZE);

        let decoded = MessageHeader::parse(&encoded).expect("should parse");
        assert_eq!(header, decoded);
    }

    #[test]
    fn test_message_header_parse_too_short() {
        let short_bytes = vec![0u8; 100];
        assert_eq!(
            MessageHeader::parse(&short_bytes),
            Err(ParseMessageError::HeaderTooShort { actual: 100 })
        );
    }

    #[test]
    fn test_message_header_parse_invalid_domains() {
        let mut bytes = vec![0u8; MessageHeader::SIZE];
        bytes[0..4].copy_from_slice(&MessageHeader::SUPPORTED_VERSION.to_be_bytes());

        bytes[4..8].copy_from_slice(&999u32.to_be_bytes());
        assert_eq!(
            MessageHeader::parse(&bytes),
            Err(ParseMessageError::UnknownSourceDomain { domain: 999 })
        );

        bytes[4..8].copy_from_slice(&DomainId::Ethereum.as_u32().to_be_bytes());
        bytes[8..12].copy_from_slice(&998u32.to_be_bytes());
        assert_eq!(
            MessageHeader::parse(&bytes),
            Err(ParseMessageError::UnknownDestinationDomain { domain: 998 })
        );
    }

    #[test]
    fn test_message_header_rejects_unsupported_version() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Arbitrum,
            FixedBytes::from([1u8; 32]),
            FixedBytes::from([2u8; 32]),
            FixedBytes::from([3u8; 32]),
            FixedBytes::ZERO,
            1000,
            1000,
        );
        let mut encoded = header.encode().to_vec();
        encoded[0..4].copy_from_slice(&2u32.to_be_bytes());

        let err = MessageHeader::parse(&encoded)
            .expect_err("parse must reject unsupported message header versions");
        assert_eq!(
            err,
            ParseMessageError::UnsupportedHeaderVersion { version: 2 }
        );
    }

    #[test]
    fn test_burn_message_v2_new() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");

        let msg = BurnMessageV2::new(burn_token, mint_recipient, amount, sender);

        assert_eq!(msg.version(), 1);
        assert_eq!(msg.burn_token(), burn_token.into_word());
        assert_eq!(msg.burn_token_address(), Some(burn_token));
        assert_eq!(msg.mint_recipient(), mint_recipient.into_word());
        assert_eq!(msg.mint_recipient_address(), Some(mint_recipient));
        assert_eq!(msg.amount(), amount);
        assert_eq!(msg.message_sender(), sender.into_word());
        assert_eq!(msg.message_sender_address(), Some(sender));
        assert_eq!(msg.max_fee(), U256::ZERO);
        assert_eq!(msg.fee_executed(), U256::ZERO);
        assert_eq!(msg.expiration_block(), U256::ZERO);
        assert!(msg.hook_data().is_empty());
        assert!(!msg.has_hooks());
        assert!(!msg.has_nonzero_max_fee());
    }

    #[test]
    fn test_burn_message_v2_with_max_fee() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");
        let max_fee = U256::from(100u64);

        let msg =
            BurnMessageV2::new_with_max_fee(burn_token, mint_recipient, amount, sender, max_fee);

        assert_eq!(msg.max_fee(), max_fee);
        assert!(msg.has_nonzero_max_fee());
        assert!(!msg.has_hooks());
    }

    #[test]
    fn test_burn_message_v2_with_hooks() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");
        let hook_data = Bytes::from(vec![1, 2, 3, 4]);

        let msg = BurnMessageV2::new_with_hooks(
            burn_token,
            mint_recipient,
            amount,
            sender,
            hook_data.clone(),
        );

        assert_eq!(msg.hook_data(), &hook_data);
        assert!(msg.has_hooks());
        assert!(!msg.has_nonzero_max_fee());
    }

    #[test]
    fn test_burn_message_v2_builder() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");

        let msg = BurnMessageV2::new(burn_token, mint_recipient, amount, sender)
            .with_max_fee(U256::from(100u64))
            .with_hook_data(Bytes::from(vec![1, 2, 3]))
            .with_expiration_block(U256::from(1000u64));

        assert!(msg.has_nonzero_max_fee());
        assert!(msg.has_hooks());
        assert_eq!(msg.expiration_block(), U256::from(1000u64));
    }

    #[test]
    fn test_burn_message_v2_encode_parse_roundtrip() {
        let message = BurnMessageV2::new_with_max_fee(
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
            U256::from(1_000_000u64),
            address!("1234567890abcdef1234567890abcdef12345678"),
            U256::from(100u64),
        )
        .with_hook_data(Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]))
        .with_expiration_block(U256::from(12345u64));

        let encoded = message.encode();
        let decoded = BurnMessageV2::parse(&encoded).expect("burn message should parse");

        assert_eq!(decoded, message);
        assert_eq!(
            decoded.encode(),
            encoded,
            "decode then encode must reproduce the canonical bytes"
        );
    }

    #[test]
    fn test_burn_message_v2_rejects_unsupported_version() {
        let mut encoded = canonical_burn_body_bytes();
        encoded[0..4].copy_from_slice(&9u32.to_be_bytes());

        let err = BurnMessageV2::parse(&encoded)
            .expect_err("parse must reject unsupported burn body versions");
        assert_eq!(
            err,
            ParseMessageError::UnsupportedBurnBodyVersion { version: 9 }
        );
    }

    #[test]
    fn test_burn_message_v2_rejects_short_body() {
        let short_bytes = vec![0u8; 100];
        assert_eq!(
            BurnMessageV2::parse(&short_bytes),
            Err(ParseMessageError::BurnBodyTooShort { actual: 100 })
        );
    }

    fn canonical_burn_body_bytes() -> Vec<u8> {
        BurnMessageV2::new_with_max_fee(
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
            U256::from(1_000_000u64),
            address!("1234567890abcdef1234567890abcdef12345678"),
            U256::from(100u64),
        )
        .encode()
        .to_vec()
    }

    #[test]
    fn test_burn_message_v2_preserves_raw_body_words_without_domain_context() {
        let mut bytes = canonical_burn_body_bytes();
        bytes[4] = 0xff;
        bytes[36] = 0xee;
        bytes[100] = 0xdd;

        let parsed = BurnMessageV2::parse(&bytes)
            .expect("body-only parse preserves raw words without domain context");
        assert_eq!(parsed.encode().as_ref(), bytes.as_slice());
        assert_eq!(parsed.burn_token().as_slice(), &bytes[4..36]);
        assert_eq!(parsed.mint_recipient().as_slice(), &bytes[36..68]);
        assert_eq!(parsed.message_sender().as_slice(), &bytes[100..132]);
        assert_eq!(parsed.burn_token_address(), None);
        assert_eq!(parsed.mint_recipient_address(), None);
        assert_eq!(parsed.message_sender_address(), None);
    }

    #[test]
    fn test_domain_aware_body_parse_rejects_non_canonical_evm_words() {
        // Witness the boundary of the canonical-padding loop: the *last* of
        // the 12 leading zero bytes (byte 11 of each address word). The
        // first-byte variants are also covered below; this pins the iteration
        // bound at 12 (not 11) so a regression that shortened the slice would
        // surface here.
        for (offset, field, expected, source_domain, destination_domain) in [
            (
                4,
                "burn_token",
                ParseMessageError::NonCanonicalBurnToken,
                DomainId::Ethereum,
                DomainId::Base,
            ),
            (
                36,
                "mint_recipient",
                ParseMessageError::NonCanonicalMintRecipient,
                DomainId::Ethereum,
                DomainId::Base,
            ),
            (
                100,
                "message_sender",
                ParseMessageError::NonCanonicalMessageSender,
                DomainId::Ethereum,
                DomainId::Base,
            ),
        ] {
            for padding_offset in [0, 11] {
                let mut body_bytes = canonical_burn_body_bytes();
                body_bytes[offset + padding_offset] = 0xff;
                let header = MessageHeader::new(
                    source_domain,
                    destination_domain,
                    FixedBytes::from([0x11; 32]),
                    address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
                    address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
                    FixedBytes::ZERO,
                    FinalityThreshold::Standard.as_u32(),
                    FinalityThreshold::Standard.as_u32(),
                );
                let mut bytes = header.encode().to_vec();
                bytes.extend_from_slice(&body_bytes);

                let err = ParsedV2Message::parse(&bytes)
                    .expect_err("domain-aware parse must reject non-canonical body words");
                assert_eq!(err, expected.clone(), "wrong error for {field}");
            }
        }
    }

    #[test]
    fn test_safe_full_message_constructor_rejects_non_canonical_evm_words() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Base,
            FixedBytes::from([0x11; 32]),
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            FixedBytes::ZERO,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2::from_raw_words(
            FixedBytes::from([0xff; 32]),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            U256::from(1_000_000u64),
            address!("1234567890abcdef1234567890abcdef12345678").into_word(),
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            Bytes::new(),
        );

        let error = ParsedV2Message::new(header, body)
            .expect_err("safe construction must enforce domain-aware EVM padding");
        assert_eq!(error, ParseMessageError::NonCanonicalBurnToken);
    }

    #[test]
    fn test_parsed_v2_message_rejects_non_canonical_body() {
        // Real Circle message reused from the round-trip test; valid as-is, then mutated.
        let mut bytes = hex::decode("0000000100000003000000062f3cb13cf4a6103f9e3b256495b08c4e05630fcba639565d199ed420a5f2be010000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000000000000000000000000000000000000000000000000007d0000007d00000000100000000000000000000000075faf114eafb1bdbe2f0316df893fd58ce46aa4d0000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a019200000000000000000000000000000000000000000000000000000000000f42400000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a0192000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        // Sanity-check: the canonical form parses round-trip cleanly today.
        let canonical = ParsedV2Message::parse(&bytes).expect("canonical message parses");
        assert_eq!(canonical.encode().as_ref(), bytes.as_slice());

        // Body burn_token word lives at MessageHeader::SIZE + 4; flip a padding byte.
        bytes[MessageHeader::SIZE + 4] = 0xff;

        let err = ParsedV2Message::parse(&bytes)
            .expect_err("ParsedV2Message::parse must reject non-canonical body address words");
        assert_eq!(err, ParseMessageError::NonCanonicalBurnToken);

        // ParsedV2MessageSummary::parse is the agent/tool-facing entry point;
        // it must surface the same per-field rejection.
        let summary_err = ParsedV2MessageSummary::parse(&bytes).expect_err(
            "ParsedV2MessageSummary::parse must reject non-canonical body address words",
        );
        assert_eq!(summary_err, ParseMessageError::NonCanonicalBurnToken);
    }

    #[test]
    fn test_parsed_v2_message_rejects_unsupported_body_version() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Base,
            FixedBytes::from([1u8; 32]),
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            FixedBytes::ZERO,
            1000,
            1000,
        );
        let mut bytes = header.encode().to_vec();
        let mut body = canonical_burn_body_bytes();
        body[0..4].copy_from_slice(&9u32.to_be_bytes());
        bytes.extend_from_slice(&body);

        let err = ParsedV2Message::parse(&bytes)
            .expect_err("full parse must reject unsupported burn body versions");
        assert_eq!(
            err,
            ParseMessageError::UnsupportedBurnBodyVersion { version: 9 }
        );
        let summary_err = ParsedV2MessageSummary::parse(&bytes)
            .expect_err("summary parse must reject unsupported burn body versions");
        assert_eq!(
            summary_err,
            ParseMessageError::UnsupportedBurnBodyVersion { version: 9 }
        );
    }

    #[test]
    fn test_message_header_permissionless_helpers() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Base,
            FixedBytes::from([0u8; 32]),
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            FixedBytes::ZERO,
            FinalityThreshold::Fast.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );

        assert!(header.has_placeholder_nonce());
        assert!(header.is_permissionless());
        assert_eq!(
            header.sender_address(),
            Some(address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"))
        );
        assert_eq!(
            header.recipient_address(),
            Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"))
        );
        assert_eq!(header.requested_finality(), Some(FinalityThreshold::Fast));
        assert_eq!(
            header.attested_finality(),
            Some(FinalityThreshold::Standard)
        );
        assert_eq!(header.destination_caller_address(), None);
    }

    #[test]
    fn test_message_header_does_not_project_non_canonical_evm_words() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Base,
            FixedBytes::from([0x11; 32]),
            FixedBytes::from([0xff; 32]),
            FixedBytes::from([0xee; 32]),
            FixedBytes::from([0xdd; 32]),
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );

        assert_eq!(header.sender_address(), None);
        assert_eq!(header.recipient_address(), None);
        assert_eq!(header.destination_caller_address(), None);
    }

    #[test]
    fn test_transfer_kind_uses_requested_finality_not_max_fee() {
        let message = |finality: FinalityThreshold, max_fee: U256| {
            let header = MessageHeader::new(
                DomainId::Ethereum,
                DomainId::Base,
                FixedBytes::from([0x11; 32]),
                address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
                address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
                FixedBytes::ZERO,
                finality.as_u32(),
                finality.as_u32(),
            );
            let body = BurnMessageV2::new(
                address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"),
                address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
                U256::from(1_000_000u64),
                address!("1234567890abcdef1234567890abcdef12345678"),
            )
            .with_max_fee(max_fee);
            ParsedV2Message::new(header, body).expect("canonical EVM message")
        };

        let zero_fee_fast = message(FinalityThreshold::Fast, U256::ZERO);
        assert!(zero_fee_fast.header().is_fast_transfer());
        assert!(zero_fee_fast.is_fast_transfer());
        assert!(!zero_fee_fast.body().has_nonzero_max_fee());
        assert!(zero_fee_fast.summary().is_fast_transfer());

        let fee_switch_standard = message(FinalityThreshold::Standard, U256::from(42u64));
        assert!(!fee_switch_standard.header().is_fast_transfer());
        assert!(!fee_switch_standard.is_fast_transfer());
        assert!(fee_switch_standard.body().has_nonzero_max_fee());
        assert!(!fee_switch_standard.summary().is_fast_transfer());
    }

    #[test]
    fn test_parsed_v2_message_from_real_circle_message() {
        let raw_message = hex::decode("0000000100000003000000062f3cb13cf4a6103f9e3b256495b08c4e05630fcba639565d199ed420a5f2be010000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000000000000000000000000000000000000000000000000007d0000007d00000000100000000000000000000000075faf114eafb1bdbe2f0316df893fd58ce46aa4d0000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a019200000000000000000000000000000000000000000000000000000000000f42400000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a0192000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        let parsed = ParsedV2Message::parse(&raw_message).expect("message should parse");
        let summary = parsed.summary();

        assert_eq!(parsed.header().source_domain(), DomainId::Arbitrum);
        assert_eq!(parsed.header().destination_domain(), DomainId::Base);
        assert!(!parsed.header().has_placeholder_nonce());
        assert_eq!(
            parsed.header().requested_finality(),
            Some(FinalityThreshold::Standard)
        );
        assert_eq!(
            parsed.header().attested_finality(),
            Some(FinalityThreshold::Standard)
        );
        assert_eq!(
            parsed.body().burn_token(),
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word()
        );
        assert_eq!(
            parsed.body().burn_token_address(),
            Some(address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"))
        );
        assert_eq!(
            parsed.body().mint_recipient(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word()
        );
        assert_eq!(
            parsed.body().mint_recipient_address(),
            Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"))
        );
        assert_eq!(parsed.body().amount(), U256::from(1_000_000u64));
        assert_eq!(
            parsed.body().message_sender(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word()
        );
        assert_eq!(
            parsed.body().message_sender_address(),
            Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"))
        );
        assert_eq!(parsed.body().max_fee(), U256::ZERO);
        assert_eq!(parsed.body().fee_executed(), U256::ZERO);
        assert_eq!(parsed.body().expiration_block(), U256::ZERO);
        assert!(parsed.body().hook_data().is_empty());
        assert_eq!(parsed.encode().as_ref(), raw_message.as_slice());
        assert_eq!(
            parsed.message_hash(),
            alloy_primitives::keccak256(&raw_message)
        );
        assert_eq!(
            summary.message_hash(),
            alloy_primitives::keccak256(&raw_message)
        );
        assert!(summary.permissionless_relay());
        assert!(!summary.has_hooks());
        assert!(!summary.is_fast_transfer());

        let json = serde_json::to_value(&summary).expect("summary should serialize");
        assert_eq!(json["message_hash"], summary.message_hash().to_string());
        assert_eq!(json["message_len_bytes"], raw_message.len());
        assert_eq!(json["permissionless_relay"], true);
        assert_eq!(json["has_hooks"], false);
        assert_eq!(json["is_fast_transfer"], false);
        assert_eq!(summary.message().encode().as_ref(), raw_message.as_slice());
    }

    #[test]
    fn test_parsed_v2_message_summary_omits_empty_optionals() {
        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Base,
            FixedBytes::from([0x22; 32]),
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            FixedBytes::ZERO,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2::new(
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
            U256::from(1_000_000u64),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
        );
        let summary = ParsedV2Message::new(header, body)
            .expect("canonical EVM message")
            .summary();

        let json = serde_json::to_value(summary).expect("summary should serialize");
        assert!(json.get("destination_caller").is_none());
        assert!(json.get("hook_data").is_none());
    }

    #[test]
    fn test_summary_drops_evm_address_for_non_evm_source() {
        let solana_sender_word = FixedBytes::<32>::from([0xABu8; 32]);
        let solana_burn_token_word = FixedBytes::<32>::from([0xCDu8; 32]);
        let solana_message_sender_word = FixedBytes::<32>::from([0xEFu8; 32]);
        let recipient = address!("7F7D081724F0240c64C9E01CDe4626602f9a0192");

        let header = MessageHeader::new(
            DomainId::Solana,
            DomainId::Base,
            FixedBytes::from([0x11u8; 32]),
            solana_sender_word,
            recipient.into_word(),
            FixedBytes::ZERO,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2::from_raw_words(
            solana_burn_token_word,
            recipient.into_word(),
            U256::from(1_000_000u64),
            solana_message_sender_word,
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            Bytes::new(),
        );
        let message = ParsedV2Message::new(header, body).expect("valid non-EVM source words");
        let encoded = message.encode();
        let parsed = ParsedV2Message::parse(&encoded)
            .expect("non-EVM source body words must parse and round-trip");
        assert_eq!(parsed.encode(), encoded);

        assert_eq!(parsed.header().sender_address(), None);
        assert_eq!(parsed.header().recipient_address(), Some(recipient));
        assert_eq!(parsed.body().burn_token(), solana_burn_token_word);
        assert_eq!(parsed.body().burn_token_address(), None);
        assert_eq!(parsed.body().message_sender(), solana_message_sender_word);
        assert_eq!(parsed.body().message_sender_address(), None);
        assert_eq!(parsed.body().mint_recipient_address(), Some(recipient));

        let summary = parsed.summary();
        assert_eq!(summary.sender(), None);
        assert_eq!(summary.sender_bytes(), solana_sender_word);
        assert_eq!(summary.recipient(), Some(recipient));
        assert_eq!(summary.recipient_bytes(), recipient.into_word());
        assert_eq!(summary.burn_token(), None);
        assert_eq!(summary.burn_token_bytes(), solana_burn_token_word);
        assert_eq!(summary.message_sender(), None);
        assert_eq!(summary.message_sender_bytes(), solana_message_sender_word);
        assert_eq!(summary.mint_recipient(), Some(recipient));
        assert_eq!(summary.mint_recipient_bytes(), recipient.into_word());

        let json = serde_json::to_value(&summary).expect("summary should serialize");
        assert!(
            json.get("sender").is_none(),
            "EVM sender field should be omitted for non-EVM source domain"
        );
        assert!(
            json.get("burn_token").is_none(),
            "EVM burn_token field should be omitted for non-EVM source domain"
        );
        assert!(
            json.get("message_sender").is_none(),
            "EVM message_sender field should be omitted for non-EVM source domain"
        );
        assert_eq!(
            json["sender_bytes"].as_str(),
            Some(format!("0x{}", hex::encode(solana_sender_word)).as_str())
        );

        assert_eq!(summary.message().encode(), encoded);
    }

    #[test]
    fn test_summary_drops_evm_address_for_non_evm_destination() {
        let sender = address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D");
        let starknet_recipient_word = FixedBytes::<32>::from([0x42u8; 32]);
        let starknet_caller_word = FixedBytes::<32>::from([0x77u8; 32]);
        let starknet_mint_recipient_word = FixedBytes::<32>::from([0x99u8; 32]);

        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::StarknetTestnet,
            FixedBytes::from([0x22u8; 32]),
            sender.into_word(),
            starknet_recipient_word,
            starknet_caller_word,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2::from_raw_words(
            address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF").into_word(),
            starknet_mint_recipient_word,
            U256::from(1_000_000u64),
            sender.into_word(),
            U256::ZERO,
            U256::ZERO,
            U256::ZERO,
            Bytes::new(),
        );
        let message = ParsedV2Message::new(header, body).expect("valid non-EVM destination words");
        let parsed = ParsedV2Message::parse(&message.encode())
            .expect("non-EVM destination mint recipient words must parse");

        assert!(!parsed.header().is_permissionless());
        assert_eq!(parsed.header().sender_address(), Some(sender));
        assert_eq!(parsed.header().recipient_address(), None);
        assert_eq!(parsed.header().destination_caller_address(), None);
        assert_eq!(parsed.body().mint_recipient(), starknet_mint_recipient_word);
        assert_eq!(parsed.body().mint_recipient_address(), None);

        let summary = parsed.summary();
        assert_eq!(summary.sender(), Some(sender));
        assert_eq!(summary.recipient(), None);
        assert_eq!(summary.recipient_bytes(), starknet_recipient_word);
        assert_eq!(summary.destination_caller(), None);
        assert_eq!(summary.destination_caller_bytes(), starknet_caller_word);
        assert!(!summary.permissionless_relay());
        assert_eq!(
            summary.burn_token(),
            Some(address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF"))
        );
        assert_eq!(summary.mint_recipient(), None);
        assert_eq!(summary.mint_recipient_bytes(), starknet_mint_recipient_word);
        assert_eq!(summary.message_sender(), Some(sender));

        let json = serde_json::to_value(&summary).expect("summary should serialize");
        assert!(
            json.get("recipient").is_none(),
            "EVM recipient field should be omitted for non-EVM destination domain"
        );
        assert!(
            json.get("mint_recipient").is_none(),
            "EVM mint_recipient field should be omitted for non-EVM destination domain"
        );
        assert!(
            json.get("destination_caller").is_none(),
            "EVM destination_caller field should be omitted for non-EVM destination domain"
        );
    }

    #[test]
    fn test_summary_keeps_caller_bytes_for_non_permissionless_non_evm_destination() {
        let sender = address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D");
        let solana_recipient_word = FixedBytes::<32>::from([0x33u8; 32]);
        let solana_caller_word = FixedBytes::<32>::from([0x44u8; 32]);

        let header = MessageHeader::new(
            DomainId::Ethereum,
            DomainId::Solana,
            FixedBytes::from([0x55u8; 32]),
            sender.into_word(),
            solana_recipient_word,
            solana_caller_word,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2::new(
            address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF"),
            address!("1111111111111111111111111111111111111111"),
            U256::from(2_500_000u64),
            sender,
        );
        let message = ParsedV2Message::new(header, body).expect("valid non-EVM destination words");

        let summary = message.summary();
        assert_eq!(summary.recipient(), None);
        assert_eq!(summary.recipient_bytes(), solana_recipient_word);
        assert_eq!(summary.destination_caller(), None);
        assert_eq!(summary.destination_caller_bytes(), solana_caller_word);
        assert!(
            !summary.permissionless_relay(),
            "non-zero caller word must not be reported as permissionless even \
             when the destination is non-EVM and destination_caller is None"
        );
    }
}
