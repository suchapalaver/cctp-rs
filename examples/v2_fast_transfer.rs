// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! CCTP v2 Fast Transfer Example
//!
//! This example demonstrates how to perform a fast CCTP v2 transfer with <30 second settlement.
//! Fast transfers use finality threshold 1000 ("confirmed" level) and may incur fees (0-14 bps).
//!
//! Default route: Arbitrum Sepolia -> Base Sepolia. Override with
//! `SOURCE_CHAIN` and `DESTINATION_CHAIN`.
//!
//! Environment variables (set these in .env file):
//! - `TESTNET_PRIVATE_KEY`: Your wallet private key (must start with 0x)
//! - `TESTNET_API_KEY`: Alchemy API key used when RPC URL overrides are not set
//! - `SOURCE_CHAIN`: (optional) Source chain alias, for example `arbitrum-sepolia`
//! - `DESTINATION_CHAIN`: (optional) Destination chain alias, for example `base-sepolia`
//! - `SOURCE_RPC_URL`: (optional) Full source RPC URL override
//! - `DESTINATION_RPC_URL`: (optional) Full destination RPC URL override
//! - chain-specific RPC overrides such as `ARBITRUM_SEPOLIA_RPC_URL` or `BASE_SEPOLIA_RPC_URL`
//! - `SOURCE_USDC_ADDRESS`: (optional) Source USDC override for new testnets
//! - `DESTINATION_USDC_ADDRESS`: (optional) Destination USDC override for new testnets
//! - `EXECUTE_TRANSFER=true`: Required to submit approval, burn, and mint transactions
//!
//! Run with: `cargo run --example v2_fast_transfer`

mod common;

use std::future::IntoFuture;

use alloy_network::EthereumWallet;
use alloy_primitives::U256;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer_local::PrivateKeySigner;
use cctp_rs::{CctpError, CctpV2, CctpV2Bridge, Erc20Contract, MintResult, TransferMode};
use common::funded_v2::{
    format_eth_balance, format_usdc_balance, redacted_rpc_url, supported_testnet_routes,
    FundedV2Route, DEFAULT_TRANSFER_AMOUNT, MIN_NATIVE_BALANCE_WEI, MIN_USDC_BALANCE,
};
use dotenvy::dotenv;
use tracing::{info_span, Instrument};

const DEFAULT_FAST_FEE_BUFFER_PERCENT: u32 = 20;

#[tokio::main]
async fn main() -> Result<(), CctpError> {
    // Load .env file
    dotenv().ok();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let route = FundedV2Route::from_env()?;
    let source_chain = route.source_chain();
    let destination_chain = route.destination_chain();

    println!("⚡ CCTP v2 Fast Transfer: {source_chain} → {destination_chain}");
    println!("==========================================================\n");

    // Load environment variables
    let private_key_str =
        std::env::var("TESTNET_PRIVATE_KEY").expect("TESTNET_PRIVATE_KEY must be set in .env file");

    // Parse private key and get wallet address
    let signer: PrivateKeySigner = private_key_str
        .parse()
        .expect("Invalid TESTNET_PRIVATE_KEY format");
    let wallet_address = signer.address();

    println!("📍 Configuration:");
    println!("   Wallet: {wallet_address}");
    println!("   Source: {source_chain}");
    println!("   Destination: {destination_chain}");
    println!("   Transfer Mode: ⚡ Fast (Confirmed)");
    println!(
        "   Supported testnet aliases: {}",
        supported_testnet_routes()
    );
    println!("   Source RPC: {}", redacted_rpc_url(&route.source_rpc_url));
    println!(
        "   Destination RPC: {}\n",
        redacted_rpc_url(&route.destination_rpc_url)
    );

    // Create wallet from signer
    let wallet = EthereumWallet::from(signer);

    // Create providers with wallet for signing transactions
    println!("1️⃣  Creating blockchain providers...");

    let source_provider = ProviderBuilder::new()
        .wallet(wallet.clone())
        .connect_http(route.source_rpc_url.clone());

    let destination_provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(route.destination_rpc_url.clone());

    println!("   ✅ Providers created (with wallet signer)\n");

    println!("2️⃣  Checking balances...");

    let source_usdc_contract = Erc20Contract::new(route.source_usdc_address, &source_provider);
    let destination_usdc_contract =
        Erc20Contract::new(route.destination_usdc_address, &destination_provider);

    let (
        source_eth_balance,
        source_usdc_balance,
        destination_eth_balance,
        destination_usdc_balance,
    ) = tokio::try_join!(
        async {
            source_provider
                .get_balance(wallet_address)
                .into_future()
                .instrument(info_span!("get_eth_balance", chain = %source_chain))
                .await
                .map_err(CctpError::from)
        },
        async {
            source_usdc_contract
                .balance_of(wallet_address)
                .instrument(info_span!("get_usdc_balance", chain = %source_chain))
                .await
                .map_err(CctpError::from)
        },
        async {
            destination_provider
                .get_balance(wallet_address)
                .into_future()
                .instrument(info_span!("get_eth_balance", chain = %destination_chain))
                .await
                .map_err(CctpError::from)
        },
        async {
            destination_usdc_contract
                .balance_of(wallet_address)
                .instrument(info_span!("get_usdc_balance", chain = %destination_chain))
                .await
                .map_err(CctpError::from)
        },
    )?;

    println!("   {source_chain}:");
    println!(
        "     ETH Balance:  {} ETH",
        format_eth_balance(source_eth_balance)
    );
    println!(
        "     USDC Balance: {} USDC",
        format_usdc_balance(source_usdc_balance)
    );
    println!("   {destination_chain}:");
    println!(
        "     ETH Balance:  {} ETH",
        format_eth_balance(destination_eth_balance)
    );
    println!(
        "     USDC Balance: {} USDC",
        format_usdc_balance(destination_usdc_balance)
    );
    println!("   ✅ Balances retrieved\n");

    // Summary
    println!("📊 Balance Summary:");
    println!("┌─────────────────────┬──────────────────┬──────────────────┐");
    println!("│ Chain               │ ETH Balance      │ USDC Balance     │");
    println!("├─────────────────────┼──────────────────┼──────────────────┤");
    println!(
        "│ {:<19} │ {:>16} │ {:>16} │",
        source_chain.to_string(),
        format_eth_balance(source_eth_balance),
        format_usdc_balance(source_usdc_balance)
    );
    println!(
        "│ {:<19} │ {:>16} │ {:>16} │",
        destination_chain.to_string(),
        format_eth_balance(destination_eth_balance),
        format_usdc_balance(destination_usdc_balance)
    );
    println!("└─────────────────────┴──────────────────┴──────────────────┘\n");

    // Check if we have sufficient balances
    let min_eth = U256::from(MIN_NATIVE_BALANCE_WEI); // 0.001 native token minimum
    let min_usdc = U256::from(MIN_USDC_BALANCE); // 1 USDC minimum (6 decimals)

    let mut issues: Vec<String> = Vec::new();

    if source_eth_balance < min_eth {
        issues.push(format!(
            "❌ Insufficient native gas token on {source_chain}: {} (need >= 0.001)\n   \
             → Fund source gas before running with EXECUTE_TRANSFER=true",
            format_eth_balance(source_eth_balance)
        ));
    }

    if destination_eth_balance < min_eth {
        issues.push(format!(
            "❌ Insufficient native gas token on {destination_chain}: {} (need >= 0.001)\n   \
             → Fund destination gas in case this run needs to self-relay",
            format_eth_balance(destination_eth_balance)
        ));
    }

    if source_usdc_balance < min_usdc {
        issues.push(format!(
            "❌ Insufficient USDC on {source_chain}: {} (need >= 1 USDC)\n   \
             → Get testnet USDC: https://faucet.circle.com/",
            format_usdc_balance(source_usdc_balance)
        ));
    }

    if !issues.is_empty() {
        println!("⚠️  Cannot proceed - insufficient balances:\n");
        for issue in &issues {
            println!("   {issue}\n");
        }
        println!("Please fund your wallet and try again.");
        return Ok(());
    }

    println!("✅ All balance requirements met!\n");

    // Fast transfer info
    println!("4️⃣  Fast Transfer Configuration:");
    println!("   ┌─────────────────┬─────────────┬──────────────┐");
    println!("   │ Feature         │ Fast        │ Standard     │");
    println!("   ├─────────────────┼─────────────┼──────────────┤");
    println!("   │ Settlement Time │ <30 seconds │ 10-15 minutes│");
    println!("   │ Finality Level  │ 1000        │ 2000         │");
    println!("   │ Fee             │ 0-14 bps    │ Free         │");
    println!("   │ Poll Interval   │ 5 seconds   │ 60 seconds   │");
    println!("   │ Security        │ Confirmed   │ Finalized    │");
    println!("   └─────────────────┴─────────────┴──────────────┘\n");

    // Safety exit - remove this line to proceed with the actual transfer
    println!("🛑 Dry run complete. To execute the actual fast transfer:");
    println!("   Set the environment variable: EXECUTE_TRANSFER=true");
    println!("   Then run: cargo run --example v2_fast_transfer\n");

    if std::env::var("EXECUTE_TRANSFER").unwrap_or_default() != "true" {
        return Ok(());
    }

    println!("🚀 EXECUTE_TRANSFER=true detected, proceeding with fast transfer...\n");

    // Create bridge with fast transfer enabled
    println!("5️⃣  Setting up CCTP v2 bridge with FAST TRANSFER...");

    let amount = U256::from(DEFAULT_TRANSFER_AMOUNT); // 1 USDC (6 decimals)

    let fee_quote_bridge = CctpV2Bridge::builder()
        .source_chain(source_chain)
        .destination_chain(destination_chain)
        .source_provider(source_provider.clone())
        .destination_provider(destination_provider.clone())
        .recipient(wallet_address)
        .build();

    // Fetch the live route fee and add a 20% buffer before setting maxFee.
    let max_fee = fee_quote_bridge
        .calculate_fast_transfer_max_fee(amount, DEFAULT_FAST_FEE_BUFFER_PERCENT)
        .await?;

    let bridge = CctpV2Bridge::builder()
        .source_chain(source_chain)
        .destination_chain(destination_chain)
        .source_provider(source_provider)
        .destination_provider(destination_provider)
        .recipient(wallet_address)
        .transfer_mode(TransferMode::Fast { max_fee }) // <30s settlement, capped fee
        .build();

    println!("   ✅ Fast transfer bridge created\n");

    // Display configuration
    println!("6️⃣  Bridge Configuration:");
    println!("   Transfer Type: ⚡ Fast");
    println!("   Finality Threshold: {}", bridge.finality_threshold());
    println!("   Fast Transfer Enabled: {}", bridge.is_fast_transfer());
    println!(
        "   Max Fee: {max_fee} USDC atomic units (live fee + {DEFAULT_FAST_FEE_BUFFER_PERCENT}% buffer)"
    );
    println!("   Expected Settlement: <30 seconds\n");

    // Validate domain IDs
    println!("7️⃣  Domain ID Validation:");
    let source_domain = bridge.source_chain().cctp_v2_domain_id()?;
    let dest_domain = bridge.destination_domain_id()?;

    println!("   Source Domain ({source_chain}): {source_domain}");
    println!("   Destination Domain ({destination_chain}): {dest_domain}");
    println!("   ✅ Domain IDs correct\n");

    // Validate API endpoint
    println!("8️⃣  API Endpoint:");
    let api_url = bridge.api_url();
    println!("   {}", api_url.as_str());
    assert!(
        api_url.as_str().contains("sandbox"),
        "Should use sandbox API for testnet"
    );
    println!("   ✅ Using sandbox API\n");

    println!("9️⃣  Transfer Details:");
    println!("   Token: USDC ({source_chain})");
    println!("   Source Token Address: {}", route.source_usdc_address);
    println!(
        "   Destination Token Address: {}",
        route.destination_usdc_address
    );
    println!("   Amount: 1.0 USDC");
    println!("   From: {wallet_address}");
    println!("   To: {wallet_address} (same address on {destination_chain})");
    println!("   Mode: ⚡ Fast Transfer\n");

    // Execute the transfer
    println!("\n🚀 Starting Fast Transfer...\n");

    // Check and handle ERC20 approval
    println!("🔟 Approval Phase:");
    println!("   Checking TokenMessenger allowance...");

    let token_messenger = bridge.token_messenger_v2_contract()?;
    let current_allowance = bridge
        .get_allowance(route.source_usdc_address, wallet_address)
        .await?;

    println!(
        "   Current allowance: {} USDC",
        format_usdc_balance(current_allowance)
    );
    println!("   TokenMessenger: {token_messenger}");

    if current_allowance < amount {
        println!("   ⚠️  Insufficient allowance, sending approval transaction...");

        let approval_tx = bridge
            .approve(route.source_usdc_address, wallet_address, amount)
            .await?;
        println!("   ✅ Approval TX: {approval_tx}");
        println!("   Explorer: {}", route.source_tx_url(approval_tx));

        // Wait for approval to be mined
        println!("   Waiting for approval confirmation...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    } else {
        println!("   ✅ Sufficient allowance already granted");
    }

    println!("\n1️⃣1️⃣ Burn Phase (Fast Transfer):");
    println!("   Burning 1 USDC on {source_chain} with fast finality...");

    let burn_tx = bridge
        .burn(amount, wallet_address, route.source_usdc_address)
        .await?;
    println!("   ✅ Burn TX: {burn_tx}");
    println!("   Explorer: {}", route.source_tx_url(burn_tx));

    println!("\n1️⃣2️⃣ Attestation Phase (Fast Polling):");
    println!("   Polling Circle API for attestation and message...");
    println!("   ⚡ Fast transfer polling interval: 5 seconds");
    println!("   Expected wait: <30 seconds (vs 10-15 minutes standard)\n");

    // Poll for attestation with progress updates
    // V2 API uses transaction hash, not message hash
    // Fast transfers poll more frequently (5s vs 60s)
    let (message, attestation) = bridge
        .get_attestation(burn_tx, cctp_rs::PollingConfig::fast_transfer())
        .await?;
    println!("\n   ✅ Attestation and message received!");
    println!("   Message length: {} bytes", message.len());
    println!("   Attestation length: {} bytes", attestation.len());

    println!("\n1️⃣3️⃣ Mint Phase:");
    println!("   Minting 1 USDC on {destination_chain}...");

    let mint_result = bridge
        .mint_if_needed(message, attestation, wallet_address)
        .await?;
    let mint_summary = match mint_result {
        MintResult::Minted(mint_tx) => {
            println!("   ✅ Mint TX: {mint_tx}");
            println!("   Explorer: {}", route.destination_tx_url(mint_tx));
            mint_tx.to_string()
        }
        MintResult::AlreadyRelayed => {
            println!("   ✅ Already relayed by a third party");
            "already relayed".to_string()
        }
    };

    println!("\n🎉 Fast Transfer Complete!");
    println!("   Your 1 USDC has been bridged from {source_chain} to {destination_chain}.");
    println!("   ⚡ Settlement time: <30 seconds (fast transfer mode)");
    println!("\n   Summary:");
    println!("   - Burn TX: {burn_tx}");
    println!("   - Mint result: {mint_summary}");
    println!("   - Transfer Mode: Fast (finality level 1000)");
    println!("\n✅ CCTP v2 Fast Transfer Validation: PASSED");

    Ok(())
}
