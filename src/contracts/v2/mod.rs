// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 contract bindings
//!
//! This module contains contract bindings for Circle's CCTP v2 contracts,
//! which add Fast Transfer, programmable hooks, and support for 11
//! v2-capable EVM chain families (the v1 set plus Linea, Sonic, Sei,
//! HyperEVM). See [`crate::DomainId`] for the full set of 30 current CCTP
//! domain IDs the protocol parser recognizes.

mod message_transmitter_v2;
mod token_messenger_v2;

pub use message_transmitter_v2::{MessageTransmitterV2, MessageTransmitterV2Contract};
pub use token_messenger_v2::{TokenMessengerV2, TokenMessengerV2Contract};
