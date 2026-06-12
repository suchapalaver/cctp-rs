// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 message format types
//!
//! Circle's CCTP v2 introduces a structured message format with headers and
//! typed body formats for different message types (burn messages, etc.).
//!
//! Reference: <https://developers.circle.com/cctp/technical-guide>

use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use serde::{Deserialize, Serialize};
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

fn check_supported_version(
    layer: &str,
    version: u32,
    supported: u32,
) -> Result<(), ParseMessageError> {
    if version == supported {
        Ok(())
    } else {
        Err(ParseMessageError::new(format!(
            "unsupported CCTP v2 {layer} version {version}; supported version is {supported}"
        )))
    }
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
/// Rejecting non-canonical words preserves the `decode(raw).encode() == raw`
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

fn check_canonical_address_word(bytes: &[u8], field: &str) -> Result<(), ParseMessageError> {
    if bytes.len() != 32 {
        return Err(ParseMessageError::new(format!(
            "{field} word requires 32 bytes, got {len}",
            len = bytes.len()
        )));
    }
    if !is_canonical_evm_address_word(bytes) {
        return Err(ParseMessageError::new(format!(
            "{field} word has non-zero leading bytes; canonical CCTP v2 address \
             words must be zero-padded in the first 12 bytes"
        )));
    }
    Ok(())
}

fn check_word_for_domain(
    domain: DomainId,
    bytes: &[u8],
    field: &str,
) -> Result<(), ParseMessageError> {
    if domain.is_evm() {
        check_canonical_address_word(bytes, field)
    } else if bytes.len() != 32 {
        Err(ParseMessageError::new(format!(
            "{field} word requires 32 bytes, got {len}",
            len = bytes.len()
        )))
    } else {
        Ok(())
    }
}

fn bytes_is_empty(bytes: &Bytes) -> bool {
    bytes.is_empty()
}

/// Error returned when parsing a canonical CCTP v2 message fails.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid CCTP v2 message: {reason}")]
pub struct ParseMessageError {
    reason: String,
}

impl ParseMessageError {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Message format version
    pub version: u32,
    /// Source blockchain domain ID
    pub source_domain: DomainId,
    /// Destination blockchain domain ID
    pub destination_domain: DomainId,
    /// Unique message nonce assigned by Circle
    pub nonce: FixedBytes<32>,
    /// Address that sent the message (padded to 32 bytes)
    pub sender: FixedBytes<32>,
    /// Address that will receive the message (padded to 32 bytes)
    pub recipient: FixedBytes<32>,
    /// Address authorized to call receiveMessage on destination (0 = anyone)
    pub destination_caller: FixedBytes<32>,
    /// Minimum finality threshold required (1000 = Fast, 2000 = Standard)
    pub min_finality_threshold: u32,
    /// Actual finality threshold when message was attested
    pub finality_threshold_executed: u32,
}

impl MessageHeader {
    /// Size of the message header in bytes
    pub const SIZE: usize = 148;
    /// The only CCTP v2 message header version this parser understands.
    pub const SUPPORTED_VERSION: u32 = CCTP_V2_MESSAGE_VERSION;

    /// Creates a new message header
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u32,
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
            version,
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

    /// Decodes a message header from bytes
    ///
    /// Returns `None` if the bytes are not at least [`MessageHeader::SIZE`] bytes long
    /// or if domain IDs are invalid.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }

        let version = read_u32(bytes, 0);
        if version != Self::SUPPORTED_VERSION {
            return None;
        }

        let source_domain = read_u32(bytes, 4);
        let source_domain = DomainId::from_u32(source_domain)?;

        let destination_domain = read_u32(bytes, 8);
        let destination_domain = DomainId::from_u32(destination_domain)?;

        let nonce = FixedBytes::from_slice(&bytes[12..44]);
        let sender = FixedBytes::from_slice(&bytes[44..76]);
        let recipient = FixedBytes::from_slice(&bytes[76..108]);
        let destination_caller = FixedBytes::from_slice(&bytes[108..140]);

        let min_finality_threshold = read_u32(bytes, 140);
        let finality_threshold_executed = read_u32(bytes, 144);

        Some(Self {
            version,
            source_domain,
            destination_domain,
            nonce,
            sender,
            recipient,
            destination_caller,
            min_finality_threshold,
            finality_threshold_executed,
        })
    }

    /// Parses a message header and returns a descriptive error on failure.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        if bytes.len() < Self::SIZE {
            return Err(ParseMessageError::new(format!(
                "header requires at least {} bytes, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }

        let version = read_u32(bytes, 0);
        check_supported_version("message header", version, Self::SUPPORTED_VERSION)?;

        Self::decode(bytes).ok_or_else(|| ParseMessageError::new("failed to decode header"))
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
            .then(|| Address::from_slice(&self.sender.as_slice()[12..32]))
    }

    /// Returns the recipient as an EVM `Address` when the destination domain is EVM.
    ///
    /// Returns `None` for non-EVM destination domains. For those, the raw
    /// [`Self::recipient`] field is the canonical source of truth.
    #[must_use]
    pub fn recipient_address(&self) -> Option<Address> {
        self.destination_domain
            .is_evm()
            .then(|| Address::from_slice(&self.recipient.as_slice()[12..32]))
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
        Some(Address::from_slice(
            &self.destination_caller.as_slice()[12..32],
        ))
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnMessageV2 {
    /// Message body version
    pub version: u32,
    /// Canonical 32-byte token word from the source domain.
    pub burn_token: FixedBytes<32>,
    /// Canonical 32-byte recipient word for the destination domain.
    pub mint_recipient: FixedBytes<32>,
    /// Amount of tokens being transferred (in wei/smallest unit)
    pub amount: U256,
    /// Canonical 32-byte sender word from the source domain.
    pub message_sender: FixedBytes<32>,
    /// Maximum fee the sender is willing to pay (for Fast Transfers)
    pub max_fee: U256,
    /// Actual fee that was charged
    pub fee_executed: U256,
    /// Block number after which the message expires (anti-replay protection)
    pub expiration_block: U256,
    /// Optional hook data for programmable transfers
    pub hook_data: Bytes,
}

impl BurnMessageV2 {
    /// Minimum size of the burn message body in bytes (without hookData)
    pub const MIN_SIZE: usize = 228;
    /// The only CCTP v2 burn body version this parser understands.
    pub const SUPPORTED_VERSION: u32 = CCTP_V2_BURN_BODY_VERSION;

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

    /// Creates a new burn message with fast transfer settings
    pub fn new_with_fast_transfer(
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

    /// Sets the maximum fee for fast transfer
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

    /// Decodes a burn message body from bytes.
    ///
    /// Returns `None` for bytes shorter than [`Self::MIN_SIZE`]. The three
    /// address-like fields are preserved as raw 32-byte words because their
    /// canonical shape is domain-dependent. For any accepted input,
    /// `decode(raw).unwrap().encode() == raw`.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::MIN_SIZE {
            return None;
        }

        let version = read_u32(bytes, 0);
        if version != Self::SUPPORTED_VERSION {
            return None;
        }

        Some(Self {
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

    /// Parses a burn message body and returns a descriptive error on failure.
    ///
    /// Body-only parsing has no source/destination domain context, so it
    /// preserves the three address-like fields as raw words. Use
    /// [`ParsedV2Message::parse`] when domain-aware EVM padding validation is
    /// required.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        if bytes.len() < Self::MIN_SIZE {
            return Err(ParseMessageError::new(format!(
                "burn message body requires at least {} bytes, got {}",
                Self::MIN_SIZE,
                bytes.len()
            )));
        }

        let version = read_u32(bytes, 0);
        check_supported_version("burn message body", version, Self::SUPPORTED_VERSION)?;

        Self::decode(bytes)
            .ok_or_else(|| ParseMessageError::new("failed to decode burn message body"))
    }

    fn parse_for_domains(
        bytes: &[u8],
        source_domain: DomainId,
        destination_domain: DomainId,
    ) -> std::result::Result<Self, ParseMessageError> {
        if bytes.len() < Self::MIN_SIZE {
            return Err(ParseMessageError::new(format!(
                "burn message body requires at least {} bytes, got {}",
                Self::MIN_SIZE,
                bytes.len()
            )));
        }

        let version = read_u32(bytes, 0);
        check_supported_version("burn message body", version, Self::SUPPORTED_VERSION)?;

        check_word_for_domain(source_domain, &bytes[4..36], "burn_token")?;
        check_word_for_domain(destination_domain, &bytes[36..68], "mint_recipient")?;
        check_word_for_domain(source_domain, &bytes[100..132], "message_sender")?;

        Self::decode(bytes)
            .ok_or_else(|| ParseMessageError::new("failed to decode burn message body"))
    }

    fn decode_for_domains(
        bytes: &[u8],
        source_domain: DomainId,
        destination_domain: DomainId,
    ) -> Option<Self> {
        Self::parse_for_domains(bytes, source_domain, destination_domain).ok()
    }

    /// Returns true if this message has hook data
    pub fn has_hooks(&self) -> bool {
        !self.hook_data.is_empty()
    }

    /// Returns true if this message is configured for fast transfer (`max_fee` > 0)
    pub fn is_fast_transfer(&self) -> bool {
        self.max_fee > U256::ZERO
    }
}

/// Parsed representation of a canonical CCTP v2 transfer message.
///
/// This combines the fixed-size message header with the burn message body and
/// can be serialized directly for agent or tool responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedV2Message {
    pub header: MessageHeader,
    pub body: BurnMessageV2,
}

impl ParsedV2Message {
    /// Encodes the full CCTP v2 message.
    pub fn encode(&self) -> Bytes {
        let mut bytes = self.header.encode().to_vec();
        bytes.extend_from_slice(&self.body.encode());
        Bytes::from(bytes)
    }

    /// Decodes a canonical CCTP v2 burn-transfer message.
    ///
    /// Returns `None` for inputs that are too short, carry an unknown domain
    /// ID, or contain non-canonical EVM `bytes32` address words for the
    /// message's source or destination domains. Non-EVM-domain body words are
    /// preserved raw. For any accepted input, `decode(raw).unwrap().encode()
    /// == raw` and `decode(raw).unwrap().message_hash() == keccak256(raw)`.
    /// Use [`Self::summary`] to obtain the JSON-friendly projection used in
    /// agent and tool responses.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let header = MessageHeader::decode(bytes)?;
        let body = BurnMessageV2::decode_for_domains(
            &bytes[MessageHeader::SIZE..],
            header.source_domain,
            header.destination_domain,
        )?;
        Some(Self { header, body })
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
        let body = BurnMessageV2::parse_for_domains(
            &bytes[MessageHeader::SIZE..],
            header.source_domain,
            header.destination_domain,
        )?;
        Ok(Self { header, body })
    }

    /// Returns the keccak256 message hash used by the destination contract.
    #[must_use]
    pub fn message_hash(&self) -> FixedBytes<32> {
        alloy_primitives::keccak256(self.encode())
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
        let encoded = self.encode();
        let message_hash = alloy_primitives::keccak256(&encoded);
        let message_len_bytes = encoded.len();

        ParsedV2MessageSummary {
            message_hash,
            message_len_bytes,
            source_domain: self.header.source_domain,
            destination_domain: self.header.destination_domain,
            message_version: self.header.version,
            body_version: self.body.version,
            nonce: self.header.nonce,
            has_placeholder_nonce: self.header.has_placeholder_nonce(),
            sender_bytes: self.header.sender,
            sender: self.header.sender_address(),
            recipient_bytes: self.header.recipient,
            recipient: self.header.recipient_address(),
            destination_caller_bytes: self.header.destination_caller,
            destination_caller: self.header.destination_caller_address(),
            permissionless_relay: self.header.is_permissionless(),
            requested_finality: self.header.requested_finality(),
            attested_finality: self.header.attested_finality(),
            burn_token_bytes: self.body.burn_token,
            burn_token: self
                .header
                .source_domain
                .is_evm()
                .then(|| self.body.burn_token_address())
                .flatten(),
            mint_recipient_bytes: self.body.mint_recipient,
            mint_recipient: self
                .header
                .destination_domain
                .is_evm()
                .then(|| self.body.mint_recipient_address())
                .flatten(),
            amount: self.body.amount,
            message_sender_bytes: self.body.message_sender,
            message_sender: self
                .header
                .source_domain
                .is_evm()
                .then(|| self.body.message_sender_address())
                .flatten(),
            max_fee: self.body.max_fee,
            fee_executed: self.body.fee_executed,
            expiration_block: self.body.expiration_block,
            hook_data: self.body.hook_data.clone(),
            hook_data_len_bytes: self.body.hook_data.len(),
            has_hooks: self.body.has_hooks(),
            is_fast_transfer: self.body.is_fast_transfer(),
        }
    }
}

/// JSON-friendly summary of a canonical CCTP v2 transfer message.
///
/// `DomainId` values serialize as `snake_case` strings. Future crate releases may
/// add new domain variants, so older versions of the crate may reject summaries
/// containing unknown domain names.
///
/// # Address fields and non-EVM domains
///
/// The `*_bytes` fields (`sender_bytes`, `recipient_bytes`,
/// `destination_caller_bytes`) carry the canonical 32-byte header words and
/// are always populated. The EVM-shaped fields (`sender`, `recipient`,
/// `destination_caller`) are populated only when the corresponding domain is
/// EVM ([`DomainId::is_evm`]); for non-EVM domains such as
/// [`DomainId::Solana`] or [`DomainId::StarknetTestnet`], they are `None`
/// because a trailing-20-byte projection would be misleading.
///
/// The same pattern applies to burn-body words: `burn_token_bytes`,
/// `mint_recipient_bytes`, and `message_sender_bytes` always carry the
/// canonical 32-byte wire values, while the EVM-shaped address projections are
/// populated only for the source or destination domains where that projection
/// is meaningful.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedV2MessageSummary {
    pub message_hash: FixedBytes<32>,
    pub message_len_bytes: usize,
    pub source_domain: DomainId,
    pub destination_domain: DomainId,
    pub message_version: u32,
    pub body_version: u32,
    pub nonce: FixedBytes<32>,
    pub has_placeholder_nonce: bool,
    /// Canonical 32-byte sender word from the header. Always populated.
    pub sender_bytes: FixedBytes<32>,
    /// EVM sender address, populated only when `source_domain.is_evm()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<Address>,
    /// Canonical 32-byte recipient word from the header. Always populated.
    pub recipient_bytes: FixedBytes<32>,
    /// EVM recipient address, populated only when `destination_domain.is_evm()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Canonical 32-byte destination caller word. Zero for permissionless messages.
    pub destination_caller_bytes: FixedBytes<32>,
    /// EVM destination caller address, populated only when the message is not
    /// permissionless and `destination_domain.is_evm()`.
    ///
    /// A `None` here is therefore ambiguous on its own — use
    /// `permissionless_relay` to disambiguate. A `None` with
    /// `permissionless_relay == true` means the message is open to any relayer;
    /// a `None` with `permissionless_relay == false` means a caller is set but
    /// the destination is non-EVM, and `destination_caller_bytes` carries the
    /// canonical value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_caller: Option<Address>,
    pub permissionless_relay: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_finality: Option<FinalityThreshold>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attested_finality: Option<FinalityThreshold>,
    /// Canonical 32-byte burn token word from the body. Always populated.
    pub burn_token_bytes: FixedBytes<32>,
    /// EVM burn token address, populated only when `source_domain.is_evm()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burn_token: Option<Address>,
    /// Canonical 32-byte mint recipient word from the body. Always populated.
    pub mint_recipient_bytes: FixedBytes<32>,
    /// EVM mint recipient address, populated only when `destination_domain.is_evm()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mint_recipient: Option<Address>,
    pub amount: U256,
    /// Canonical 32-byte message sender word from the body. Always populated.
    pub message_sender_bytes: FixedBytes<32>,
    /// EVM message sender address, populated only when `source_domain.is_evm()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_sender: Option<Address>,
    pub max_fee: U256,
    pub fee_executed: U256,
    pub expiration_block: U256,
    #[serde(default, skip_serializing_if = "bytes_is_empty")]
    pub hook_data: Bytes,
    pub hook_data_len_bytes: usize,
    pub has_hooks: bool,
    pub is_fast_transfer: bool,
}

impl ParsedV2MessageSummary {
    /// Parses and summarizes a canonical CCTP v2 transfer message.
    pub fn parse(bytes: &[u8]) -> std::result::Result<Self, ParseMessageError> {
        ParsedV2Message::parse(bytes).map(|message| message.summary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, hex};

    #[test]
    fn test_message_header_size() {
        assert_eq!(MessageHeader::SIZE, 148);
    }

    #[test]
    fn test_message_header_encode_decode() {
        let header = MessageHeader::new(
            1,
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

        let decoded = MessageHeader::decode(&encoded).expect("should decode");
        assert_eq!(header, decoded);
    }

    #[test]
    fn test_message_header_decode_too_short() {
        let short_bytes = vec![0u8; 100];
        assert!(MessageHeader::decode(&short_bytes).is_none());
    }

    #[test]
    fn test_message_header_decode_invalid_domain() {
        let mut bytes = vec![0u8; MessageHeader::SIZE];
        bytes[0..4].copy_from_slice(&MessageHeader::SUPPORTED_VERSION.to_be_bytes());
        // Set invalid source domain ID (999)
        bytes[4..8].copy_from_slice(&999u32.to_be_bytes());
        assert!(MessageHeader::decode(&bytes).is_none());
    }

    #[test]
    fn test_message_header_rejects_unsupported_version() {
        let header = MessageHeader::new(
            MessageHeader::SUPPORTED_VERSION,
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

        assert!(
            MessageHeader::decode(&encoded).is_none(),
            "decode must reject unsupported message header versions"
        );
        let err = MessageHeader::parse(&encoded)
            .expect_err("parse must reject unsupported message header versions");
        assert!(
            err.to_string()
                .contains("unsupported CCTP v2 message header version 2"),
            "parse error should name the unsupported header version: {err}"
        );
    }

    #[test]
    fn test_burn_message_v2_new() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");

        let msg = BurnMessageV2::new(burn_token, mint_recipient, amount, sender);

        assert_eq!(msg.version, 1);
        assert_eq!(msg.burn_token, burn_token.into_word());
        assert_eq!(msg.burn_token_address(), Some(burn_token));
        assert_eq!(msg.mint_recipient, mint_recipient.into_word());
        assert_eq!(msg.mint_recipient_address(), Some(mint_recipient));
        assert_eq!(msg.amount, amount);
        assert_eq!(msg.message_sender, sender.into_word());
        assert_eq!(msg.message_sender_address(), Some(sender));
        assert_eq!(msg.max_fee, U256::ZERO);
        assert_eq!(msg.fee_executed, U256::ZERO);
        assert_eq!(msg.expiration_block, U256::ZERO);
        assert!(msg.hook_data.is_empty());
        assert!(!msg.has_hooks());
        assert!(!msg.is_fast_transfer());
    }

    #[test]
    fn test_burn_message_v2_fast_transfer() {
        let burn_token = address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF");
        let mint_recipient = address!("742d35Cc6634C0532925a3b844Bc9e7595f8fA0d");
        let amount = U256::from(1000000u64);
        let sender = address!("1234567890abcdef1234567890abcdef12345678");
        let max_fee = U256::from(100u64);

        let msg = BurnMessageV2::new_with_fast_transfer(
            burn_token,
            mint_recipient,
            amount,
            sender,
            max_fee,
        );

        assert_eq!(msg.max_fee, max_fee);
        assert!(msg.is_fast_transfer());
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

        assert_eq!(msg.hook_data, hook_data);
        assert!(msg.has_hooks());
        assert!(!msg.is_fast_transfer());
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

        assert!(msg.is_fast_transfer());
        assert!(msg.has_hooks());
        assert_eq!(msg.expiration_block, U256::from(1000u64));
    }

    #[test]
    fn test_burn_message_v2_encode_decode_roundtrip() {
        let message = BurnMessageV2::new_with_fast_transfer(
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"),
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"),
            U256::from(1_000_000u64),
            address!("1234567890abcdef1234567890abcdef12345678"),
            U256::from(100u64),
        )
        .with_hook_data(Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]))
        .with_expiration_block(U256::from(12345u64));

        let encoded = message.encode();
        let decoded = BurnMessageV2::decode(&encoded).expect("burn message should decode");

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

        assert!(
            BurnMessageV2::decode(&encoded).is_none(),
            "decode must reject unsupported burn body versions"
        );
        let err = BurnMessageV2::parse(&encoded)
            .expect_err("parse must reject unsupported burn body versions");
        assert!(
            err.to_string()
                .contains("unsupported CCTP v2 burn message body version 9"),
            "parse error should name the unsupported body version: {err}"
        );
    }

    fn canonical_burn_body_bytes() -> Vec<u8> {
        BurnMessageV2::new_with_fast_transfer(
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

        let decoded = BurnMessageV2::decode(&bytes)
            .expect("body-only decode preserves raw words without domain context");
        assert_eq!(decoded.encode().as_ref(), bytes.as_slice());
        assert_eq!(decoded.burn_token.as_slice(), &bytes[4..36]);
        assert_eq!(decoded.mint_recipient.as_slice(), &bytes[36..68]);
        assert_eq!(decoded.message_sender.as_slice(), &bytes[100..132]);
        assert_eq!(decoded.burn_token_address(), None);
        assert_eq!(decoded.mint_recipient_address(), None);
        assert_eq!(decoded.message_sender_address(), None);

        let parsed = BurnMessageV2::parse(&bytes)
            .expect("body-only parse preserves raw words without domain context");
        assert_eq!(parsed, decoded);
    }

    #[test]
    fn test_domain_aware_body_parse_rejects_non_canonical_evm_words() {
        // Witness the boundary of the canonical-padding loop: the *last* of
        // the 12 leading zero bytes (byte 11 of each address word). The
        // first-byte variants are also covered below; this pins the iteration
        // bound at 12 (not 11) so a regression that shortened the slice would
        // surface here.
        for (offset, field, source_domain, destination_domain) in [
            (4, "burn_token", DomainId::Ethereum, DomainId::Base),
            (36, "mint_recipient", DomainId::Ethereum, DomainId::Base),
            (100, "message_sender", DomainId::Ethereum, DomainId::Base),
        ] {
            for padding_offset in [0, 11] {
                let mut bytes = canonical_burn_body_bytes();
                bytes[offset + padding_offset] = 0xff;

                assert!(
                    BurnMessageV2::decode_for_domains(&bytes, source_domain, destination_domain)
                        .is_none(),
                    "domain-aware decode must reject non-canonical {field} word"
                );
                let err =
                    BurnMessageV2::parse_for_domains(&bytes, source_domain, destination_domain)
                        .expect_err("domain-aware parse must reject non-canonical body words");
                assert!(
                    err.to_string().contains(field),
                    "parse error should name the offending field {field}: {err}"
                );
            }
        }
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

        assert!(
            ParsedV2Message::decode(&bytes).is_none(),
            "ParsedV2Message::decode must reject non-canonical body address words"
        );
        let err = ParsedV2Message::parse(&bytes)
            .expect_err("ParsedV2Message::parse must reject non-canonical body address words");
        assert!(
            err.to_string().contains("burn_token"),
            "parse error should name the offending field: {err}"
        );

        // ParsedV2MessageSummary::parse is the agent/tool-facing entry point;
        // it must surface the same per-field rejection.
        let summary_err = ParsedV2MessageSummary::parse(&bytes).expect_err(
            "ParsedV2MessageSummary::parse must reject non-canonical body address words",
        );
        assert!(
            summary_err.to_string().contains("burn_token"),
            "summary parse error should name the offending field: {summary_err}"
        );
    }

    #[test]
    fn test_parsed_v2_message_rejects_unsupported_body_version() {
        let header = MessageHeader::new(
            MessageHeader::SUPPORTED_VERSION,
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

        assert!(
            ParsedV2Message::decode(&bytes).is_none(),
            "full decode must reject unsupported burn body versions"
        );
        let err = ParsedV2Message::parse(&bytes)
            .expect_err("full parse must reject unsupported burn body versions");
        assert!(
            err.to_string()
                .contains("unsupported CCTP v2 burn message body version 9"),
            "parse error should name the unsupported body version: {err}"
        );
        let summary_err = ParsedV2MessageSummary::parse(&bytes)
            .expect_err("summary parse must reject unsupported burn body versions");
        assert!(
            summary_err
                .to_string()
                .contains("unsupported CCTP v2 burn message body version 9"),
            "summary parse error should name the unsupported body version: {summary_err}"
        );
    }

    #[test]
    fn test_message_header_permissionless_helpers() {
        let header = MessageHeader::new(
            1,
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
    fn test_parsed_v2_message_from_real_circle_message() {
        let raw_message = hex::decode("0000000100000003000000062f3cb13cf4a6103f9e3b256495b08c4e05630fcba639565d199ed420a5f2be010000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000000000000000000000000000000000000000000000000007d0000007d00000000100000000000000000000000075faf114eafb1bdbe2f0316df893fd58ce46aa4d0000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a019200000000000000000000000000000000000000000000000000000000000f42400000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a0192000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

        let parsed = ParsedV2Message::parse(&raw_message).expect("message should parse");
        let summary = parsed.summary();

        assert_eq!(parsed.header.source_domain, DomainId::Arbitrum);
        assert_eq!(parsed.header.destination_domain, DomainId::Base);
        assert!(!parsed.header.has_placeholder_nonce());
        assert_eq!(
            parsed.header.requested_finality(),
            Some(FinalityThreshold::Standard)
        );
        assert_eq!(
            parsed.header.attested_finality(),
            Some(FinalityThreshold::Standard)
        );
        assert_eq!(
            parsed.body.burn_token,
            address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word()
        );
        assert_eq!(
            parsed.body.burn_token_address(),
            Some(address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D"))
        );
        assert_eq!(
            parsed.body.mint_recipient,
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word()
        );
        assert_eq!(
            parsed.body.mint_recipient_address(),
            Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"))
        );
        assert_eq!(parsed.body.amount, U256::from(1_000_000u64));
        assert_eq!(
            parsed.body.message_sender,
            address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word()
        );
        assert_eq!(
            parsed.body.message_sender_address(),
            Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192"))
        );
        assert_eq!(parsed.body.max_fee, U256::ZERO);
        assert_eq!(parsed.body.fee_executed, U256::ZERO);
        assert_eq!(parsed.body.expiration_block, U256::ZERO);
        assert!(parsed.body.hook_data.is_empty());
        assert_eq!(parsed.encode().as_ref(), raw_message.as_slice());
        assert_eq!(
            parsed.message_hash(),
            alloy_primitives::keccak256(&raw_message)
        );
        assert_eq!(
            summary.message_hash,
            alloy_primitives::keccak256(&raw_message)
        );
        assert!(summary.permissionless_relay);
        assert!(!summary.has_hooks);
        assert!(!summary.is_fast_transfer);

        let json = serde_json::to_value(&summary).expect("summary should serialize");
        let round_tripped: ParsedV2MessageSummary =
            serde_json::from_value(json).expect("summary should round-trip");
        assert_eq!(round_tripped, summary);
    }

    #[test]
    fn test_parsed_v2_message_summary_omits_empty_optionals() {
        let summary = ParsedV2MessageSummary {
            message_hash: FixedBytes::from([0x11; 32]),
            message_len_bytes: 376,
            source_domain: DomainId::Ethereum,
            destination_domain: DomainId::Base,
            message_version: 1,
            body_version: 1,
            nonce: FixedBytes::from([0x22; 32]),
            has_placeholder_nonce: false,
            sender_bytes: address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            sender: Some(address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D")),
            recipient_bytes: address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            recipient: Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192")),
            destination_caller_bytes: FixedBytes::ZERO,
            destination_caller: None,
            permissionless_relay: true,
            requested_finality: Some(FinalityThreshold::Standard),
            attested_finality: Some(FinalityThreshold::Standard),
            burn_token_bytes: address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D").into_word(),
            burn_token: Some(address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D")),
            mint_recipient_bytes: address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            mint_recipient: Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192")),
            amount: U256::from(1_000_000u64),
            message_sender_bytes: address!("7F7D081724F0240c64C9E01CDe4626602f9a0192").into_word(),
            message_sender: Some(address!("7F7D081724F0240c64C9E01CDe4626602f9a0192")),
            max_fee: U256::ZERO,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data: Bytes::new(),
            hook_data_len_bytes: 0,
            has_hooks: false,
            is_fast_transfer: false,
        };

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
            1,
            DomainId::Solana,
            DomainId::Base,
            FixedBytes::from([0x11u8; 32]),
            solana_sender_word,
            recipient.into_word(),
            FixedBytes::ZERO,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2 {
            version: 1,
            burn_token: solana_burn_token_word,
            mint_recipient: recipient.into_word(),
            amount: U256::from(1_000_000u64),
            message_sender: solana_message_sender_word,
            max_fee: U256::ZERO,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data: Bytes::new(),
        };
        let message = ParsedV2Message { header, body };
        let encoded = message.encode();
        let parsed = ParsedV2Message::parse(&encoded)
            .expect("non-EVM source body words must parse and round-trip");
        assert_eq!(parsed.encode(), encoded);

        assert_eq!(parsed.header.sender_address(), None);
        assert_eq!(parsed.header.recipient_address(), Some(recipient));
        assert_eq!(parsed.body.burn_token, solana_burn_token_word);
        assert_eq!(parsed.body.burn_token_address(), None);
        assert_eq!(parsed.body.message_sender, solana_message_sender_word);
        assert_eq!(parsed.body.message_sender_address(), None);
        assert_eq!(parsed.body.mint_recipient_address(), Some(recipient));

        let summary = parsed.summary();
        assert_eq!(summary.sender, None);
        assert_eq!(summary.sender_bytes, solana_sender_word);
        assert_eq!(summary.recipient, Some(recipient));
        assert_eq!(summary.recipient_bytes, recipient.into_word());
        assert_eq!(summary.burn_token, None);
        assert_eq!(summary.burn_token_bytes, solana_burn_token_word);
        assert_eq!(summary.message_sender, None);
        assert_eq!(summary.message_sender_bytes, solana_message_sender_word);
        assert_eq!(summary.mint_recipient, Some(recipient));
        assert_eq!(summary.mint_recipient_bytes, recipient.into_word());

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

        let round_tripped: ParsedV2MessageSummary =
            serde_json::from_value(json).expect("non-EVM summary should round-trip");
        assert_eq!(round_tripped, summary);
        assert_eq!(round_tripped.sender, None);
        assert_eq!(round_tripped.sender_bytes, solana_sender_word);
        assert_eq!(round_tripped.burn_token, None);
        assert_eq!(round_tripped.burn_token_bytes, solana_burn_token_word);
    }

    #[test]
    fn test_summary_drops_evm_address_for_non_evm_destination() {
        let sender = address!("75FaF114EAFb1bdbE2f0316Df893Fd58ce46AA4D");
        let starknet_recipient_word = FixedBytes::<32>::from([0x42u8; 32]);
        let starknet_caller_word = FixedBytes::<32>::from([0x77u8; 32]);
        let starknet_mint_recipient_word = FixedBytes::<32>::from([0x99u8; 32]);

        let header = MessageHeader::new(
            1,
            DomainId::Ethereum,
            DomainId::StarknetTestnet,
            FixedBytes::from([0x22u8; 32]),
            sender.into_word(),
            starknet_recipient_word,
            starknet_caller_word,
            FinalityThreshold::Standard.as_u32(),
            FinalityThreshold::Standard.as_u32(),
        );
        let body = BurnMessageV2 {
            version: 1,
            burn_token: address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF").into_word(),
            mint_recipient: starknet_mint_recipient_word,
            amount: U256::from(1_000_000u64),
            message_sender: sender.into_word(),
            max_fee: U256::ZERO,
            fee_executed: U256::ZERO,
            expiration_block: U256::ZERO,
            hook_data: Bytes::new(),
        };
        let message = ParsedV2Message { header, body };
        let parsed = ParsedV2Message::parse(&message.encode())
            .expect("non-EVM destination mint recipient words must parse");

        assert!(!parsed.header.is_permissionless());
        assert_eq!(parsed.header.sender_address(), Some(sender));
        assert_eq!(parsed.header.recipient_address(), None);
        assert_eq!(parsed.header.destination_caller_address(), None);
        assert_eq!(parsed.body.mint_recipient, starknet_mint_recipient_word);
        assert_eq!(parsed.body.mint_recipient_address(), None);

        let summary = parsed.summary();
        assert_eq!(summary.sender, Some(sender));
        assert_eq!(summary.recipient, None);
        assert_eq!(summary.recipient_bytes, starknet_recipient_word);
        assert_eq!(summary.destination_caller, None);
        assert_eq!(summary.destination_caller_bytes, starknet_caller_word);
        assert!(!summary.permissionless_relay);
        assert_eq!(
            summary.burn_token,
            Some(address!("A2d2a41577ce14e20a6c2de999A8Ec2BD9fe34aF"))
        );
        assert_eq!(summary.mint_recipient, None);
        assert_eq!(summary.mint_recipient_bytes, starknet_mint_recipient_word);
        assert_eq!(summary.message_sender, Some(sender));

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
            1,
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
        let message = ParsedV2Message { header, body };

        let summary = message.summary();
        assert_eq!(summary.recipient, None);
        assert_eq!(summary.recipient_bytes, solana_recipient_word);
        assert_eq!(summary.destination_caller, None);
        assert_eq!(summary.destination_caller_bytes, solana_caller_word);
        assert!(
            !summary.permissionless_relay,
            "non-zero caller word must not be reported as permissionless even \
             when the destination is non-EVM and destination_caller is None"
        );
    }
}
