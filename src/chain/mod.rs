// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0
//! Chain configuration and contract addresses for CCTP
//!
//! This module contains chain-specific configuration including contract addresses,
//! confirmation times, and domain ID mappings for all supported CCTP chains.
//!
//! - `CctpV1`: Original 7 chain families
//! - `CctpV2`: 10 v2-capable chain families (the v1 set plus Linea, Sonic,
//!   Sei) with Fast Transfer. The protocol parser (`DomainId`,
//!   `ParsedV2Message`) recognizes all 21 announced CCTP v2 domain IDs
//!   independently of bridge SDK support.

pub mod addresses;
mod config;
mod v2;

pub use config::CctpV1;
pub use v2::{CctpV2, FastTransferFee};
