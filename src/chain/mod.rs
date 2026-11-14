// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! Chain configuration and contract addresses for CCTP
//!
//! This module contains chain-specific configuration including contract addresses,
//! confirmation times, and domain ID mappings for all supported CCTP chains.
//!
//! - `CctpV1`: Original 7 chain families
//! - `CctpV2`: 21 v2-capable chain families (the 7-family v1 set plus 14
//!   v2-only families). Fast Transfer and Forwarding Service are directional
//!   capabilities on subsets of those families. The protocol parser
//!   (`DomainId`, `ParsedV2Message`)
//!   recognizes all 31 domain IDs in Circle's current CCTP domain table,
//!   independently of bridge SDK support.

pub mod addresses;
mod config;
mod v2;

pub use config::CctpV1;
pub use v2::{CctpV2, FastTransferFee};
