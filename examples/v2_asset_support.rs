// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! Demonstrates CCTP v2 asset-aware route validation without RPC.

use alloy_chains::NamedChain;
use cctp_rs::{CctpError, CctpTransferAsset, CctpV2Route};

fn main() -> Result<(), CctpError> {
    let usdc_route = CctpV2Route::for_asset(
        NamedChain::Mainnet,
        NamedChain::Linea,
        CctpTransferAsset::Usdc,
    )?;
    println!(
        "USDC source token on {}: {}",
        usdc_route.source_chain(),
        usdc_route.source_token_address(CctpTransferAsset::Usdc)?
    );

    let eurc_route = CctpV2Route::for_asset(
        NamedChain::Mainnet,
        NamedChain::Base,
        CctpTransferAsset::Eurc,
    )?;
    println!(
        "EURC source token on {}: {}",
        eurc_route.source_chain(),
        eurc_route.source_token_address(CctpTransferAsset::Eurc)?
    );

    let unannounced_eurc_route = CctpV2Route::for_asset(
        NamedChain::Mainnet,
        NamedChain::Avalanche,
        CctpTransferAsset::Eurc,
    )
    .expect_err("EURC is modeled only for Ethereum <-> Base CCTP routes");
    println!("Rejected unannounced EURC route: {unannounced_eurc_route}");

    let usyc_route = CctpV2Route::for_asset(
        NamedChain::Mainnet,
        NamedChain::Base,
        CctpTransferAsset::Usyc,
    )
    .expect_err("USYC depends on BNB routing, which this bridge SDK does not support");
    println!("Rejected USYC route: {usyc_route}");

    Ok(())
}
