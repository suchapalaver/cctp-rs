// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 contract bindings
//!
//! This module contains contract bindings for Circle's CCTP v2 contracts,
//! which add Fast Transfer, programmable hooks, and support for 10
//! v2-capable EVM chain families (the v1 set plus Linea, Sonic, Sei).
//! See [`crate::DomainId`] for the full set of 21 announced CCTP v2
//! domain IDs the protocol parser recognizes.

mod message_transmitter_v2;
mod token_messenger_v2;

pub use message_transmitter_v2::{MessageTransmitterV2, MessageTransmitterV2Contract};
pub use token_messenger_v2::{TokenMessengerV2, TokenMessengerV2Contract};
