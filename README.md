# cctp-rs

[![Crates.io](https://img.shields.io/crates/v/cctp-rs.svg)](https://crates.io/crates/cctp-rs)
[![Documentation](https://docs.rs/cctp-rs/badge.svg)](https://docs.rs/cctp-rs)
[![License](https://img.shields.io/crates/l/cctp-rs.svg)](LICENSE)
[![Build Status](https://img.shields.io/github/actions/workflow/status/suchapalaver/cctp-rs/ci.yml?branch=main)](https://github.com/suchapalaver/cctp-rs/actions)
[![REUSE status](https://api.reuse.software/badge/github.com/suchapalaver/cctp-rs)](https://api.reuse.software/info/github.com/suchapalaver/cctp-rs)

A production-ready Rust implementation of Circle's Cross-Chain Transfer Protocol (CCTP), enabling seamless USDC transfers and asset-aware EURC bridge flows across Circle-supported blockchain networks.

cctp-rs is independently maintained community Rust tooling. It is not official
Circle software, and protocol currency work is tracked publicly in the
[CCTP protocol currency roadmap](https://github.com/suchapalaver/cctp-rs/issues/35).

## Features

- 🚀 **Type-safe** contract interactions using Alloy
- 🔄 **Bridge SDK** for 11 v2-capable EVM mainnet chains plus 6 USDC
  testnets, with EURC helpers for Ethereum <-> Base; **protocol parser**
  currently implements all 30 domain IDs in Circle's current CCTP
  domain table
- 📦 **Builder pattern** for intuitive API usage
- ⚡ **CCTP v2 support** with fast transfers (<30s settlement)
- 🤝 **Relayer-aware** APIs for permissionless v2 relay handling
- 🎯 **Programmable hooks** for advanced use cases
- 🔍 **Comprehensive observability** with OpenTelemetry integration
- 🤖 **Agent/tooling-friendly message inspection** with serializable v2 parsers

## Supported Chains

cctp-rs has two layers with different coverage. Read both before
choosing an integration path.

- **Bridge SDK** — `CctpV2Bridge`, `Cctp`, and the `CctpV1` / `CctpV2`
  traits on `alloy_chains::NamedChain` can burn modeled CCTP assets and
  relay attestations end-to-end for the chains listed below. These are
  the chains where `NamedChain::supports_cctp_v2()` returns `true`.
- **Protocol parser** — `ParsedV2Message`, `ParsedV2MessageSummary`,
  and the `DomainId` enum recognize all 30 domain IDs in Circle's
  current CCTP domain table, including non-EVM domains.
  Parsing a domain is independent of whether the bridge SDK can route
  to or from it. Non-EVM body address words are preserved as raw
  `bytes32` values; EVM address projections are exposed only when the
  source or destination domain uses the EVM padding convention.

### Bridge SDK — supported chains

Chains returning `true` from `NamedChain::supports_cctp_v2()`:

#### Mainnet

- Ethereum, Arbitrum, Base, Optimism, Avalanche, Polygon, Unichain
- Linea, Sonic, Sei, HyperEVM (v2-only chains)

#### Testnet

- Sepolia, Arbitrum Sepolia, Base Sepolia, Optimism Sepolia
- Avalanche Fuji, Polygon Amoy

### Bridge SDK — supported assets

- **USDC** — asset-aware helpers resolve Circle-published EVM token
  addresses for every bridge-supported v2 chain listed above.
- **EURC** — modeled for Ethereum <-> Base on mainnet and Sepolia <-> Base
  Sepolia on testnet, matching Circle's September 2, 2026 CCTP EURC
  announcement and Circle's EURC contract address table.
- **USYC** — represented explicitly as `CctpTransferAsset::Usyc`, but
  rejected by `CctpV2Bridge` today because Circle documents USYC only for
  Ethereum and BNB Smart Chain, while this bridge SDK does not currently
  route BNB Smart Chain.

Prefer `burn_asset`, `approve_asset`, `ensure_asset_approval`, and
`transfer_asset` when the SDK should validate asset support and resolve
token addresses. The raw `burn`, `approve`, `ensure_approval`, and
`transfer` methods still accept explicit ERC-20 addresses for advanced
contract workflows.

### Protocol parser — additional domains

`DomainId` and `ParsedV2MessageSummary` recognize the following CCTP
v2 domains, but the bridge SDK does **not** currently accept them as
source or destination — `NamedChain::supports_cctp_v2()` returns
`false` and the bridge builder will reject them:

- Solana (5, non-EVM), Aptos (9, non-EVM), Codex (12),
  World Chain (14), Monad (15), BNB Smart Chain (17, USYC only),
  XDC (18), Ink (21), Plume (22), Starknet (25, non-EVM),
  Arc Testnet (26), Stellar (27, non-EVM), EDGE (28),
  Injective (29), Morph (30), Pharos (31), Cronos (32),
  Plasma (33), X Layer (37)

Use this list when you need to *inspect* messages crossing these
domains (for indexing, analytics, or wallet UIs) without routing
through them. To extend bridge SDK support to one of these domains,
follow [AGENTS.md → Adding chain support](AGENTS.md#adding-chain-support).

### Protocol currency roadmap

Circle's current
[supported blockchains and domains](https://developers.circle.com/cctp/concepts/supported-chains-and-domains)
docs list 30 current CCTP domain IDs through X Layer (37), plus the
V1 legacy-only Noble (4) and Sui (8) domains. The parser table tracks
the current 30-domain CCTP table; bridge routing remains a separate,
validated support boundary.

The public protocol currency roadmap is tracked in
[#35](https://github.com/suchapalaver/cctp-rs/issues/35), including
fast-transfer capability drift, Forwarding Service and Stellar-safe
flows, Fast Transfer allowance preflight, Standard Transfer fee-switch
support, current EVM route coverage, non-USDC Iris fee endpoint drift,
([#53](https://github.com/suchapalaver/cctp-rs/issues/53)), and an
automated drift check.

### Asset-aware V2 burns

```rust
use cctp_rs::{CctpTransferAsset, CctpV2Bridge, CctpV2Route, CctpError, TransferMode};
use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;

async fn bridge_eurc_standard<P: Provider<Ethereum> + Clone>(
    source_provider: P,
    destination_provider: P,
    owner: Address,
    recipient: Address,
) -> Result<(), CctpError> {
    let route = CctpV2Route::for_asset(
        NamedChain::Mainnet,
        NamedChain::Base,
        CctpTransferAsset::Eurc,
    )?;
    let bridge = CctpV2Bridge::from_route(route)
        .source_provider(source_provider)
        .destination_provider(destination_provider)
        .recipient(recipient)
        .transfer_mode(TransferMode::Standard)
        .build();

    let amount = U256::from(1_000_000u64); // 1 EURC, 6 decimals
    bridge
        .ensure_asset_approval(CctpTransferAsset::Eurc, owner, amount)
        .await?;
    let _burn_tx = bridge
        .burn_asset(CctpTransferAsset::Eurc, amount, owner)
        .await?;

    Ok(())
}
```

Circle's public CCTP fee and allowance endpoints are still documented as
USDC-specific (`/v2/burn/USDC/fees` and `/v2/fastBurn/USDC/allowance`).
Accordingly, `create_transfer_fees_url_for_asset(CctpTransferAsset::Eurc)`
and `calculate_fast_transfer_max_fee_for_asset(CctpTransferAsset::Eurc, ...)`
return `CctpError::TransferFeeEndpointUnavailable` until Circle publishes an
EURC Iris fee endpoint. Standard EURC burns can use `TransferMode::Standard`;
fast EURC burns require callers to supply an externally sourced `max_fee`.

### CCTP v1 (Legacy)

The original v1 chain families (Ethereum, Arbitrum, Base, Optimism,
Avalanche, Polygon, Unichain) and the six USDC testnets above remain
supported through `Cctp` and the `CctpV1` trait for backwards
compatibility. v1 has no Unichain testnet entry.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
cctp-rs = "6"
```

### Basic Example

```rust
use cctp_rs::{Cctp, CctpError};
use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};

#[tokio::main]
async fn main() -> Result<(), CctpError> {
    // Create providers for source and destination chains
    let eth_provider = ProviderBuilder::new()
        .on_http("https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY".parse()?);
    
    let arb_provider = ProviderBuilder::new()
        .on_http("https://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY".parse()?);

    // Set up the CCTP bridge
    let bridge = Cctp::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Arbitrum)
        .source_provider(eth_provider)
        .destination_provider(arb_provider)
        .recipient("0xYourRecipientAddress".parse()?)
        .build();

    // Get contract addresses
    let token_messenger = bridge.token_messenger_contract()?;
    let destination_domain = bridge.destination_domain_id()?;
    
    println!("Token Messenger: {}", token_messenger);
    println!("Destination Domain: {}", destination_domain);
    
    Ok(())
}
```

### Bridging USDC (V1)

```rust
use cctp_rs::{Cctp, CctpError, PollingConfig};
use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use alloy_provider::Provider;

async fn bridge_usdc_v1<P: Provider + Clone>(bridge: &Cctp<P>) -> Result<(), CctpError> {
    // Step 1: Burn USDC on source chain (get tx hash from your burn transaction)
    let burn_tx_hash = "0x...".parse()?;

    // Step 2: Get message and message hash from the burn transaction
    let (message, message_hash) = bridge.get_message_sent_event(burn_tx_hash).await?;

    // Step 3: Wait for attestation from Circle's API
    let attestation = bridge.get_attestation(message_hash, PollingConfig::default()).await?;

    println!("V1 Bridge successful!");
    println!("Message: {} bytes", message.len());
    println!("Attestation: {} bytes", attestation.len());

    // Step 4: Mint on destination chain using message + attestation
    // mint_on_destination(&message, &attestation).await?;

    Ok(())
}
```

### Bridging USDC (V2 - Recommended)

```rust
use cctp_rs::{CctpV2Bridge, CctpError, CctpV2Route, PollingConfig, TransferMode};
use alloy_chains::NamedChain;
use alloy_primitives::U256;
use alloy_provider::{Provider, ProviderBuilder};

async fn route_first_v2_bridge() -> Result<(), Box<dyn std::error::Error>> {
    let eth_provider = ProviderBuilder::new()
        .connect("https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY")
        .await?;
    let linea_provider = ProviderBuilder::new()
        .connect("https://linea-mainnet.g.alchemy.com/v2/YOUR_API_KEY")
        .await?;
    let route = CctpV2Route::new(NamedChain::Mainnet, NamedChain::Linea)?;

    let bridge = CctpV2Bridge::from_route(route)
        .source_provider(eth_provider)
        .destination_provider(linea_provider)
        .recipient("0xYourRecipientAddress".parse()?)
        .transfer_mode(TransferMode::Fast { max_fee: U256::from(100) })
        .build();

    println!("Destination domain: {}", bridge.destination_domain_id()?);
    Ok(())
}

async fn bridge_usdc_v2<P: Provider + Clone>(bridge: &CctpV2Bridge<P>) -> Result<(), CctpError> {
    // Step 1: Burn USDC on source chain (get tx hash from your burn transaction)
    let burn_tx_hash = "0x...".parse()?;

    // Step 2: Get canonical message AND attestation from Circle's API
    // Note: V2 returns both because the on-chain message has zeros in the nonce field
    let (message, attestation) = bridge.get_attestation(
        burn_tx_hash,
        PollingConfig::fast_transfer(),  // Optimized for v2 fast transfers
    ).await?;

    println!("V2 Bridge successful!");
    println!("Message: {} bytes", message.len());
    println!("Attestation: {} bytes", attestation.len());

    // Step 3: Mint on destination chain using message + attestation
    // bridge.mint(message, attestation, recipient).await?;

    Ok(())
}
```

### Fast Transfer Fees (V2)

Fast transfers require a `maxFee` cap in burn-token atomic units. The built-in
live fee helpers currently use Circle's USDC fee endpoint, where fees are
dynamic and route-aware. Fetch the live USDC route fee from Circle Iris before
constructing your fast-transfer mode. Do not use
`NamedChain::fast_transfer_fee_bps()` for production quotes; that helper is
chain-level static metadata and currently reports `FastTransferFee::Unknown`
until static fee tables are deliberately sourced.

```rust
use cctp_rs::{CctpV2Bridge, CctpError, TransferMode};
use alloy_primitives::U256;
use alloy_provider::Provider;

async fn fast_mode_with_live_fee<P: Provider + Clone>(
    bridge: &CctpV2Bridge<P>,
) -> Result<TransferMode, CctpError> {
    let amount = U256::from(10_500_000u64); // 10.5 USDC, 6 decimals
    let max_fee = bridge
        .calculate_fast_transfer_max_fee(amount, 20) // 20% buffer
        .await?;

    Ok(TransferMode::Fast { max_fee })
}
```

The live lookup methods are no-wallet, no-RPC HTTP calls against Iris:
`get_transfer_fees()` returns every route entry,
`get_fast_transfer_fee()` and `get_standard_transfer_fee()` select a finality
threshold, and `calculate_fast_transfer_max_fee()` converts the Fast Transfer
fee into an amount-denominated cap with your selected buffer. Funded transfer
execution remains separate: after computing `maxFee`, pass it into
`TransferMode::Fast { max_fee }` before calling burn/transfer helpers.

### Agent Tooling: Inspect a Canonical V2 Message

Tooling layers usually need structured JSON instead of raw message bytes. `ParsedV2Message`
and `ParsedV2MessageSummary` decode the canonical message returned by Circle's v2 API
into serializable Rust types.

Parsing failures return `ParseMessageError`, so this inspection path does not expand the
existing `CctpError` surface used by bridge operations.

The parser is strict about the current Circle CCTP v2 wire format: both the
message header and burn body must use version `1`. Future format revisions are
reported as unsupported instead of being decoded with today's field offsets.

`DomainId` values in serialized summaries use `snake_case` strings. Future releases may
add new domain variants, so older tooling should treat unknown domain strings as a
forward-compatibility case.

Header and burn-body address-like fields are exposed in two layers:
canonical `*_bytes` fields always carry the 32-byte wire value, while EVM
address fields such as `sender`, `mint_recipient`, and `message_sender` are
optional and omitted for non-EVM domains.

```rust
use cctp_rs::{CctpV2Bridge, ParsedV2MessageSummary, PollingConfig};

async fn inspect_v2_message<P: alloy_provider::Provider + Clone>(
    bridge: &CctpV2Bridge<P>,
    burn_tx_hash: alloy_primitives::TxHash,
) -> Result<(), Box<dyn std::error::Error>> {
    let (message, _attestation) = bridge
        .get_attestation(burn_tx_hash, PollingConfig::fast_transfer())
        .await?;

    let summary = ParsedV2MessageSummary::parse(&message)?;
    let json = serde_json::to_string_pretty(&summary)?;

    println!("{json}");
    Ok(())
}
```

## Architecture

The library is organized into several key modules:

- **`bridge`** - Core CCTP bridge implementation
- **`chain`** - Chain-specific configurations and support
- **`attestation`** - Attestation response types from Circle's Iris API
- **`protocol`** - Serializable protocol types, live fee response types, and canonical v2 message parsing
- **`error`** - Comprehensive error types for proper error handling
- **`contracts`** - Type-safe bindings for TokenMessenger and MessageTransmitter

## Formal Verification

The protocol parser's core invariants — domain ID conversions, finality
thresholds, transfer-mode dispatch, and the canonical v2 message layout —
are modeled in Lean 4 with machine-checked round-trip and canonicality
proofs. The model generates correspondence fixtures that the Rust test
suite replays against the production parser
(`tests/lean_model_correspondence.rs`), so the parser cannot silently
drift from the verified model.

See [`verification/README.md`](verification/README.md) for exactly what is
proven vs tested vs assumed, how to run the proof checks, and how to update
the model when protocol parsing changes. See
[`verification/architecture.md`](verification/architecture.md) for the
resource-graph review process used to keep verification claims tied to the
protocol implementation and external Circle authority.

## Error Handling

cctp-rs provides detailed error types for different failure scenarios:

```rust
use cctp_rs::{CctpError, PollingConfig};

// V1 example
match bridge.get_attestation(message_hash, PollingConfig::default()).await {
    Ok(attestation) => println!("Success: {} bytes", attestation.len()),
    Err(CctpError::AttestationTimeout) => println!("Timeout waiting for attestation"),
    Err(CctpError::UnsupportedChain(chain)) => println!("Chain {chain:?} not supported"),
    Err(e) => println!("Other error: {}", e),
}

// V2 example (returns both message and attestation)
match v2_bridge.get_attestation(tx_hash, PollingConfig::fast_transfer()).await {
    Ok((message, attestation)) => {
        println!("Message: {} bytes", message.len());
        println!("Attestation: {} bytes", attestation.len());
    }
    Err(CctpError::AttestationTimeout) => println!("Timeout waiting for attestation"),
    Err(e) => println!("Error: {}", e),
}
```

## Advanced Usage

### Custom Polling Configuration

```rust
use cctp_rs::PollingConfig;

// V1: Wait up to 10 minutes with 30-second intervals
let attestation = bridge.get_attestation(
    message_hash,
    PollingConfig::default()
        .with_max_attempts(20)
        .with_poll_interval_secs(30),
).await?;

// V2: Use preset for fast transfers (5 second intervals)
let (message, attestation) = v2_bridge.get_attestation(
    tx_hash,
    PollingConfig::fast_transfer(),
).await?;

// V2: Or customize for your needs
let (message, attestation) = v2_bridge.get_attestation(
    tx_hash,
    PollingConfig::default()
        .with_max_attempts(60)
        .with_poll_interval_secs(10),
).await?;

// Check total timeout
let config = PollingConfig::default();
println!("Max wait time: {} seconds", config.total_timeout_secs());
```

### Chain Configuration

```rust
use cctp_rs::{CctpV1, CctpV2};
use alloy_chains::NamedChain;

// Get v1 chain-specific information
let chain = NamedChain::Arbitrum;
let confirmation_time = chain.confirmation_average_time_seconds()?; // Standard: 19 minutes
let domain_id = chain.cctp_domain_id()?;
let token_messenger = chain.token_messenger_address()?;

println!("Arbitrum V1 confirmation time: {} seconds", confirmation_time);

// Get v2 attestation times (choose based on transfer mode)
let fast_time = chain.fast_transfer_confirmation_time_seconds()?;     // ~8 seconds
let standard_time = chain.standard_transfer_confirmation_time_seconds()?; // ~19 minutes

println!("V2 Fast Transfer: {} seconds", fast_time);
println!("V2 Standard Transfer: {} seconds", standard_time);
```

### Relayer-Aware Patterns (V2)

CCTP v2 is **permissionless** - anyone can relay a message once Circle's attestation is available. Third-party relayers (Synapse, LI.FI, etc.) actively monitor for burns and may complete transfers before your application does. This is a feature, not a bug!

#### Option A: Wait for Completion (Recommended)

If you don't need to self-relay, just wait for the transfer to complete:

```rust
use cctp_rs::{CctpV2Bridge, PollingConfig};

async fn wait_for_transfer<P: Provider + Clone>(bridge: &CctpV2Bridge<P>) -> Result<(), CctpError> {
    let burn_tx = bridge.burn(amount, from, usdc).await?;
    let (message, _attestation) = bridge.get_attestation(
        burn_tx,
        PollingConfig::fast_transfer(),
    ).await?;

    // Wait for completion (by relayer or self)
    bridge.wait_for_receive(&message, None, None).await?;
    println!("Transfer complete!");
    Ok(())
}
```

#### Option B: Self-Relay with Graceful Handling

If you want to try minting yourself but handle relayer races:

```rust
use cctp_rs::{CctpV2Bridge, MintResult, PollingConfig};

async fn self_relay<P: Provider + Clone>(bridge: &CctpV2Bridge<P>) -> Result<(), CctpError> {
    let burn_tx = bridge.burn(amount, from, usdc).await?;
    let (message, attestation) = bridge.get_attestation(
        burn_tx,
        PollingConfig::fast_transfer(),
    ).await?;

    match bridge.mint_if_needed(message, attestation, from).await? {
        MintResult::Minted(tx) => println!("We minted: {tx}"),
        MintResult::AlreadyRelayed => println!("Relayer completed it for us!"),
    }
    Ok(())
}
```

#### Option C: Check Status Manually

```rust
let is_complete = bridge.is_message_received(&message).await?;
if is_complete {
    println!("Transfer already completed by relayer");
}
```

## Examples

Check out the [`examples/`](examples/) directory for complete working examples:

### CCTP v2 Examples

- [`v2_integration_validation.rs`](examples/v2_integration_validation.rs) - Comprehensive v2 validation (no network required)
- [`v2_asset_support.rs`](examples/v2_asset_support.rs) - Asset-aware route and token-address validation (no network required)
- [`v2_standard_transfer.rs`](examples/v2_standard_transfer.rs) - Standard transfer with finality
- [`v2_fast_transfer.rs`](examples/v2_fast_transfer.rs) - Fast transfer (<30s settlement)
- [`testnet_validation.rs`](examples/testnet_validation.rs) - Opt-in funded, chain-configurable testnet transfer harness

### CCTP v1 Examples (Legacy)

- [`basic_bridge.rs`](examples/basic_bridge.rs) - Simple USDC bridge example
- [`attestation_monitoring.rs`](examples/attestation_monitoring.rs) - Monitor attestation status
- [`multi_chain.rs`](examples/multi_chain.rs) - Bridge across multiple chains

Run examples with:

```bash
# Recommended: Run v2 integration validation
cargo run --example v2_integration_validation

# Or run specific examples
cargo run --example v2_fast_transfer
cargo run --example basic_bridge
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

Routine maintainer and agent changes also go through pull requests. Direct
pushes to `main` are not part of the normal maintenance path; branch protection
requires review and required checks before merge.

## Testing

### Unit Tests

Run the full test suite with:

```bash
cargo test --all-features
```

The default, network-free tests validate:

- Contract method selection logic
- Domain ID resolution and mapping
- Configuration validation
- URL construction for Circle's Iris API
- Error handling and edge cases
- Cross-chain compatibility
- Fast transfer support
- Hooks integration

### Integration Validation

We provide comprehensive runnable examples that validate the complete v2 API without requiring network access:

```bash
# Validate all v2 configurations (no network required)
cargo run --example v2_integration_validation

# Educational examples showing complete flows
cargo run --example v2_standard_transfer
cargo run --example v2_fast_transfer
```

The `v2_integration_validation` example validates:

- Chain support matrix (11 v2-capable mainnet chain families plus 6 testnets)
- Domain ID mappings against Circle's official values
- Contract address consistency (unified v2 addresses)
- Bridge configuration variations (standard, fast, hooks)
- API endpoint construction (mainnet vs testnet)
- Fast transfer support and fee structures
- Error handling for unsupported chains
- Cross-chain compatibility

### Live Fee Smoke Testing

The `Live CCTP Fee Smoke` GitHub Actions workflow runs weekly and can be
started manually before a release. It calls only no-wallet Iris fee endpoints,
not funded transfer flows, and stays out of default PR/push CI.

For a cheap local pre-release drift check against Circle's Iris fee endpoints
that does not require wallets, RPC endpoints, or funded accounts:

```bash
cargo test --test transfer_fees live_ --all-features -- --ignored --nocapture
```

These opt-in tests query both the Sepolia -> Base Sepolia sandbox route and the
Ethereum -> Base mainnet route (`/v2/burn/USDC/fees/0/6` on each Iris host),
verify the responses decode as CCTP v2 transfer fees, and check that Iris
returns a Fast Transfer threshold.

### Live Testnet Testing

For opt-in pre-release validation on testnet:

1. Copy `.env.example` to `.env`.
2. Set `TESTNET_PRIVATE_KEY` and either `TESTNET_API_KEY` or full RPC URL overrides.
3. Optionally set `SOURCE_CHAIN` and `DESTINATION_CHAIN`. Defaults are `arbitrum-sepolia` -> `base-sepolia`; built-in aliases are `sepolia`, `arbitrum-sepolia`, `base-sepolia`, `optimism-sepolia`, `avalanche-fuji`, and `polygon-amoy`.
4. Fund the wallet with native testnet gas token on both chains and source-chain USDC from [Circle's faucet](https://faucet.circle.com).
5. Run a dry-run balance and configuration check:

```bash
cargo run --example v2_fast_transfer
```

6. Execute the funded fast-transfer path only when ready:

```bash
EXECUTE_TRANSFER=true cargo run --example v2_fast_transfer
```

Example alternate route:

```bash
SOURCE_CHAIN=sepolia \
DESTINATION_CHAIN=base-sepolia \
cargo run --example v2_fast_transfer
```

For a standard-finality transfer, use `cargo run --example testnet_validation`
with the same environment variables. Both examples check balances and allowance,
submit approval when needed, burn USDC, poll Iris for the canonical v2 message
and attestation, then call `mint_if_needed` so a third-party relayer completing
the transfer first is treated as success. They refuse mainnet routes; set
`SOURCE_USDC_ADDRESS` or `DESTINATION_USDC_ADDRESS` only when testing a supported
testnet before its USDC address has been added to the example allowlist.

**Note**: Integration tests requiring Circle's Iris API and live blockchains are not run in CI due to:

- Cost (gas fees on every test run)
- Time (10-15 minutes per transfer for attestation)
- Flakiness (network dependencies and rate limits)
- Complexity (requires funded wallets with private keys)

Instead, we validate via extensive unit tests and runnable examples. This approach ensures reliability while maintaining fast CI/CD pipelines.

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Circle](https://www.circle.com/) for creating CCTP
- [Alloy](https://github.com/alloy-rs) for the excellent Ethereum libraries
- The Rust community for amazing tools and support

## Resources

- [CCTP Documentation](https://developers.circle.com/stablecoins/cctp-getting-started)
- [API Reference](https://docs.rs/cctp-rs)
- [GitHub Repository](https://github.com/suchapalaver/cctp-rs)
