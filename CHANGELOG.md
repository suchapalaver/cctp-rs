# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [6.3.0] - 2026-06-08

### Added

- Live CCTP v2 route-fee lookup through `CctpV2Bridge`:
  `create_transfer_fees_url`, `get_transfer_fees`,
  `get_transfer_fee`, `get_fast_transfer_fee`,
  `get_standard_transfer_fee`, and
  `calculate_fast_transfer_max_fee`. The lookup calls Circle Iris'
  `/v2/burn/USDC/fees/{sourceDomain}/{destinationDomain}` endpoint
  and returns `TransferFee` entries with fractional-basis-point-safe
  `FeeBps` values. Use `calculate_fast_transfer_max_fee` immediately
  before constructing `TransferMode::Fast { max_fee }` so the
  on-chain cap reflects the current route fee plus the caller's
  selected buffer. Tracks issue #4.
- Deterministic fee lookup tests for optional `forwardFee` fields,
  standard-fee selection, HTTP status failures, malformed JSON, and
  zero/large-value fee math. Tracks issue #5.
- Ignored no-wallet live Iris fee smoke tests for sandbox and mainnet
  fee hosts, plus a scheduled/manual `Live CCTP Fee Smoke` workflow.
  These checks detect Iris response-shape drift without requiring
  wallets, RPC endpoints, funded accounts, or default PR CI network
  calls. Tracks issues #6 and #7.

### Changed

- Documentation now points production fee quoting to live route lookup
  on `CctpV2Bridge`. The chain-level `fast_transfer_fee_bps()` helper
  remains static metadata and currently reports
  `FastTransferFee::Unknown` for supported chains; it is not a
  substitute for route-aware Iris fee lookup when calculating `maxFee`.
  Tracks issues #8 and #9.

## [6.2.0] - 2026-05-28

### Added

- `UsdcAmount`, a shared six-decimal USDC amount primitive for parsing
  decimal user input into atomic USDC units without duplicating amount
  validation in downstream applications.
- `CctpV2Route`, a validated route primitive for CCTP v2 applications
  that need to validate source and destination chains before constructing
  providers or prompting for wallet signatures.
- `CctpError::InvalidAmount` for typed amount validation failures.

## [6.1.0] - 2026-05-27

### Added

- HyperEVM (Hyperliquid's EVM chain) bridge support via
  `NamedChain::Hyperliquid`. Domain ID 19, unified V2 contract
  addresses, and 5-second confirmation times for both fast and
  standard transfers (native finality). This brings the bridge SDK
  to 11 v2-capable chain families.

## [6.0.1] - 2026-05-27

### Changed

- Repository moved to [suchapalaver/cctp-rs](https://github.com/suchapalaver/cctp-rs) for independent maintenance.
- Updated author contact email.

## [6.0.0] - 2026-05-26

### Added

- New `TransferMode` enum that names the four valid CCTP v2 burn
  configurations (`Standard`, `Fast { max_fee }`,
  `StandardWithHook { hook_data }`,
  `FastWithHook { max_fee, hook_data }`). Selected via a single
  `transfer_mode` builder field on `CctpV2Bridge`.
- `CctpV2Bridge::transfer_mode()` accessor.
- `ParsedV2MessageSummary` now carries the canonical 32-byte header
  words as `sender_bytes`, `recipient_bytes`, and
  `destination_caller_bytes`. These are always populated and are the
  authoritative source of truth for non-EVM domains such as
  `DomainId::Solana` and `DomainId::StarknetTestnet`. Tracks issue
  #219.
- New `CctpError::ReceiveTimeout` variant, distinct from
  `CctpError::AttestationTimeout`. Returned by
  `CctpV2Bridge::wait_for_receive` when polling for destination-chain
  receipt confirmation exhausts its attempt budget — at that point
  attestation has already succeeded, so the failure is observing the
  message as received in time, not waiting on attestation. Both
  variants continue to satisfy `CctpError::is_timeout()` and therefore
  `is_transient()`. Tracks issue #217.
- New `cctp_rs.wait_for_receive` OpenTelemetry span emitted around
  `CctpV2Bridge::wait_for_receive`'s polling loop, plus a public
  `spans::wait_for_receive` constructor. On polling exhaustion the
  span records `error.type = "ReceiveTimeout"`, making Tempo/Jaeger
  queries like `{ span.error.type = "ReceiveTimeout" }` count
  receive-side timeouts separately from attestation-side ones. The
  existing `wait_for_receive_timeout` log event also gains an
  `error_type = "ReceiveTimeout"` field for log-stream parity with
  `attestation_timeout`. Tracks issue #240.

### Changed

- `CctpV2Bridge::wait_for_receive` now returns
  `CctpError::ReceiveTimeout` instead of `CctpError::AttestationTimeout`
  when the receive-confirmation polling loop exhausts its attempts.
  `CctpError` is `#[non_exhaustive]`, so this is not a compile-time
  break, but downstream code that pattern-matches on
  `CctpError::AttestationTimeout` to detect a `wait_for_receive`
  timeout must update its match to cover the new variant (or rely on
  `is_timeout()` / `is_transient()`, which already include both).
  Tracks issue #217.
- **Breaking:** `ParsedV2MessageSummary::sender` and `recipient` are
  now `Option<Address>` instead of `Address`, and `destination_caller`
  is `None` for non-EVM destinations as well as for permissionless
  messages. The EVM `Address` projections are populated only when the
  corresponding `DomainId::is_evm()` is true; consumers that need the
  raw header word for non-EVM domains should read the new
  `*_bytes` fields. JSON output omits the EVM fields entirely for
  non-EVM domains (via `skip_serializing_if`), so it no longer looks
  precise and EVM-shaped when it is not. Tracks issue #219.
- **Breaking:** `MessageHeader::sender_address` and
  `MessageHeader::recipient_address` now return `Option<Address>`
  (was `Address`), and `MessageHeader::destination_caller_address`
  returns `None` for non-EVM destinations in addition to the existing
  permissionless case. The raw `bytes32` header fields remain
  authoritative for non-EVM domains. Tracks issue #219.
- **Breaking:** `CctpV2Bridge::builder()` no longer accepts the
  independent `fast_transfer(bool)`, `hook_data(Bytes)`, and
  `max_fee(U256)` fields. Use `transfer_mode(TransferMode::…)`
  instead. The `is_fast_transfer()`, `hook_data()`, `max_fee()`, and
  `finality_threshold()` accessors are preserved and now derive from
  the configured mode. Note that `max_fee()` returns
  `Some(U256::ZERO)` (not `None`) for `TransferMode::Fast { max_fee:
  U256::ZERO }` and `TransferMode::FastWithHook { max_fee:
  U256::ZERO, .. }` — the prior shape always returned `None` when
  the optional `.max_fee(...)` builder method was unset.
- **Breaking:**
  `TokenMessengerV2Contract::deposit_for_burn_with_hooks_transaction`
  now takes `max_fee: U256` and `min_finality_threshold: u32`
  arguments. The previous signature hardcoded standard finality with
  zero fee, which excluded the hook + fast-finality burn that
  Circle's `depositForBurnWithHook` supports natively.
- **Breaking:** `TokenMessengerV2Contract::deposit_for_burn_transaction`
  and `deposit_for_burn_fast_transaction` now take an additional
  `min_finality_threshold: u32` argument. The previous signatures
  hardcoded `2000` and `1000` internally, so the bridge could only
  guarantee the wire `minFinalityThreshold` matched
  `transfer_mode().finality_threshold()` by routing each
  `TransferMode` to the right helper. Parameterizing both lets
  `CctpV2::burn` derive every wire value from a single
  `self.transfer_mode.finality_threshold().as_u32()` call and
  extends the issue #218 structural defense to all four
  `TransferMode` variants. Tracks issue #224.
- README and AGENTS.md now distinguish the bridge SDK's supported
  chains (the set where `NamedChain::supports_cctp_v2()` returns
  `true`) from the broader set of CCTP v2 domain IDs that the
  protocol parser can decode. The previous "26+ networks" claim
  conflated protocol parsing reach with bridge SDK reach; the
  bridge SDK currently routes 10 v2-capable chain families with
  testnets, while the parser recognizes all 21 announced domains
  including non-EVM ones. Tracks issue #216.
- **Breaking:** `CctpV2::fast_transfer_fee_bps` now returns
  `Result<FastTransferFee>` instead of `Result<Option<u32>>`, where
  `FastTransferFee` is a new public enum with `Known(u32)` and
  `Unknown` variants. Previous versions returned `Ok(Some(0))` for
  every v2-capable chain because per-chain fees had not yet been
  sourced — making unknown external protocol data look like a
  confirmed zero fee to downstream consumers. Until each chain's
  fee is sourced against an authoritative Circle reference, every
  chain now reports `FastTransferFee::Unknown`. Callers handling
  user funds should fetch the live fee rather than treating
  `Unknown` as zero. The enum is `#[non_exhaustive]` so future
  variants (e.g. a sourced-with-provenance form) can be added
  without another major break, and the trait method carries
  `#[must_use]` to surface accidental drops. Tracks issue #215.

### Fixed

- **Breaking:** `BurnMessageV2::decode`, `BurnMessageV2::parse`,
  `ParsedV2Message::decode`, and `ParsedV2Message::parse` now reject
  messages whose `burn_token`, `mint_recipient`, or `message_sender`
  `bytes32` words are not zero-padded in the leading 12 bytes.
  Previously such non-canonical inputs decoded successfully with the
  stray bytes dropped, so `decode(raw).encode() != raw` and
  `message_hash(decode(raw)) != keccak256(raw)`. The strict parser
  restores the round-trip and hash invariants for every accepted
  input. `BurnMessageV2::parse` and `ParsedV2Message::parse` surface
  a per-field error naming the offending word (`burn_token`,
  `mint_recipient`, or `message_sender`); `decode` returns `None`.
  Downstream consumers that previously fed unvalidated bytes to these
  parsers must now handle the rejection path. Tracks issue #214.
- Fast transfer + hook configurations now actually send a
  fast-finality burn on-chain. Previous versions silently fell back
  to the standard-finality hook path while still reporting
  `FinalityThreshold::Fast` from `finality_threshold()`, so the
  reported mode and the transmitted `minFinalityThreshold` (and
  `maxFee`) diverged. The accessor result and the burn now agree by
  construction. Tracks issue #218.

## [5.2.0] - 2026-05-24

### Added

- New `api_url_override: Option<Url>` builder field on `Cctp` and
  `CctpV2`. When set, `api_url()` returns the override unchanged;
  when unset, the existing mainnet/testnet selection over
  `IRIS_API` / `IRIS_API_SANDBOX` is preserved. Lets callers point
  the attestation polling loop at a custom Iris-compatible endpoint
  (e.g. a self-hosted mirror or a local mock) without forking the
  crate. Non-breaking — existing builders compile unchanged.

### Fixed

- Per-attempt and per-response inner spans
  (`cctp_rs.get_attestation`, `cctp_rs.process_attestation_response`)
  now carry the `error.type`, `error.message`, `error.context`, and
  `otel.status_code` fields that `record_error_with_context` writes
  for `HttpRequestFailed`, `AttestationFailed`,
  `AttestationDataMissing`, and `MessageDataMissing`. Previous
  versions omitted these field declarations at span creation, so
  `tracing::Span::record` silently dropped the writes — TraceQL
  queries like `{ span.error.type = "AttestationDataMissing" }`
  returned zero hits even while the SDK was emitting the errors in
  production. Operators relying on inner-span error attributes for
  alerting will now see matches.

## [5.1.2] - 2026-05-23

### Fixed

- OpenTelemetry traces no longer attribute events from unrelated async
  tasks to CCTP attestation and message-extraction spans. Previous
  versions held a `Span::enter()` guard live across `.await` points
  inside `Cctp::get_attestation`, `Cctp::get_message_sent_event`, and
  their v2 counterparts; in async Rust that leaves the span "current"
  on the executor thread after the future yields, so events emitted
  by other tasks landed under attestation spans and made production
  traces misleading. The affected futures now attach their span via
  `tracing::Instrument`, which correctly enters on poll and exits on
  yield. The retry-loop demo in `examples/complete_bridge_trace.rs`
  was updated to the same shape.

### Changed

- `cctp_rs::spans` module documentation no longer demonstrates the
  unsafe `let _guard = span.enter();` pattern for async work. The
  module-level example uses `tracing::Instrument`, and the
  `record_error` / `record_error_with_context` doctests note that the
  entered-guard pattern shown is for synchronous scopes only.

## [5.1.1] - 2026-05-19

### Changed

- Updated alloy dependencies — `alloy-primitives` and `alloy-sol-types`
  from 1.5 to 1.6, and the workspace 2.0.x crates (`alloy-contract`,
  `alloy-json-rpc`, `alloy-network`, `alloy-provider`, `alloy-rpc-types`,
  `alloy-transport`, dev-only `alloy-signer-local`) from 2.0.4 to 2.0.5.
  No source changes required; semver-compatible for callers on alloy
  1.x / 2.0.x.

## [5.0.0] - 2026-05-06

### Added

- New `AttestationFailureKind` enum (`#[non_exhaustive]`) with three
  variants — `ApiReportedFailed`, `AttestationMissing`,
  `MessageMissing` — exposed at the crate root. Lets callers
  `match` on a specific attestation-poll failure mode instead of
  substring-matching on a free-form `reason` string.

- New `CctpError::TransactionNotFound { tx_hash }` variant signals
  that the source-chain RPC returned no receipt for the given
  hash (likely not yet mined or not indexed by the queried node).
  The hash is carried on the variant so callers can log or retry
  without parsing it out of a message string.

- New `CctpError::MessageSentEventMissing { tx_hash }` variant
  signals that the receipt was found but did not contain the
  CCTP `MessageSent` log — typically because the call hit the
  wrong contract or reverted before reaching the burn step. Also
  carries the `tx_hash`.

### Changed

- **Breaking**: `CctpError::AttestationFailed` is now a tuple variant
  carrying `AttestationFailureKind`
  (`AttestationFailed(AttestationFailureKind)`) instead of the
  struct variant `AttestationFailed { reason: String }`. The five
  internal construction sites in `bridge/cctp.rs` and
  `bridge/v2.rs` collapse onto three named cases. The `Display`
  output changes from the prior free-text `reason` to a fixed
  prose phrase per kind (`Attestation failed: Iris API reported
  failed status`, `... attestation field missing in complete
  response`, or `... message field missing in complete response`);
  telemetry that scraped the prior strings should switch to
  matching on the typed kind.

- **Breaking**: `CctpError::InvalidUrl` is now a tuple variant
  carrying `url::ParseError` (`InvalidUrl(url::ParseError)`) instead
  of the struct variant `InvalidUrl { reason: String }`. Callers
  that destructured `{ reason }` must switch to tuple-pattern
  matching. The new shape exposes the structured parse error
  (kind, position) for diagnostics; the `Display` output retains
  the `Invalid URL: <details>` shape, though the per-call-site
  prefix (`Failed to construct attestation URL:` /
  `Failed to construct v2 messages URL:`) is no longer included.

### Removed

- **Breaking**: `CctpError::Provider(String)` has been removed.
  Callers that constructed `Provider(...)` directly should switch
  to `Rpc(...)` (which accepts `RpcError<TransportErrorKind>` via
  `From`) for transport errors, or `Contract(...)` for typed
  contract-call errors.

- **Breaking**: `CctpError::TransactionFailed { reason: String }`
  has been removed. Its three internal construction patterns now
  flow through typed variants: RPC-fetch failures propagate
  through `CctpError::Rpc` via the existing `#[from]` impl; the
  "receipt missing" case routes to the new `TransactionNotFound
  { tx_hash }`; the "MessageSent log absent" case routes to the
  new `MessageSentEventMissing { tx_hash }`. Callers that
  destructured `TransactionFailed { reason }` for substring
  matching should switch to matching the typed variants directly.

## [4.0.0] - 2026-05-06

### Added

- New `CctpError::Contract(alloy_contract::Error)` variant preserves
  alloy's structured contract-call error so callers can use
  `alloy_contract::Error::as_revert_data` and
  `as_decoded_interface_error` for revert decoding without re-parsing
  Display strings. Bridge call sites that previously stringified
  contract errors via `CctpError::ContractCall(format!(...))` now
  propagate the typed variant through `?`, and `is_already_relayed`
  inspects the underlying `TransportError` payload directly.

### Changed

- **Breaking**: `CctpError` is now `#[non_exhaustive]`. Downstream
  matches must include a wildcard arm. In return, future variant
  additions to `CctpError` will be non-breaking — `DomainId` already
  follows this pattern.
- **Breaking**: `CctpError::ContractCall(String)` has been removed.
  All internal call sites now use the typed `Contract(alloy_contract::Error)`
  variant; callers that constructed `ContractCall` for their own
  contract-call paths should switch to `Contract` (which accepts an
  `alloy_contract::Error` directly via `From`) or to `Provider(String)`
  for non-alloy contract code.

### Removed

- **Breaking**: `batch_token_checks`, deprecated in 3.3.0, has been
  removed. Use `batch_token_state` instead — it returns the same
  values inside a `TokenState` with named fields and predicate
  helpers (`can_transfer`, `needs_approval`, `has_sufficient_balance`).

### Fixed

- `CctpError::is_already_relayed` now detects already-relayed reverts
  on contract calls consistently. The previous implementation only
  matched the legacy `ContractCall(String)` variant, so a revert
  surfaced through alloy's typed error path was missed and the bridge
  helpers reported a hard failure instead of recognizing the message
  as already delivered.

### Internal

- Dropped a dead type-erasing fallback in `is_already_relayed`: only
  `alloy_contract::Error::TransportError` carries chain-level revert
  data, so the other variants now answer `false` directly rather than
  being stringified through pattern matching.
- Refactored bridge and example error handling to use `?` with span-
  based error context instead of repeated `map_err` + `format!`
  bridges.
- Examples now read USDC balances via the public `Erc20Contract` API
  and parallelize the four pre-flight balance reads with
  `tokio::join!`. ETH balance reads were aligned with the same
  span-based `?` idiom.
- Added regression tests covering `is_already_relayed` against both
  the typed `ContractNotDeployed` variant (no revert payload) and an
  RPC `ErrorPayload` carrying a revert message.
- Documentation: clarified why BNB Smart Chain is excluded from CCTP
  v2 (USYC-only on domain 17) and removed stale TODOs; spelled out
  the threshold semantics of `TokenState::has_sufficient_balance`.

## [3.3.0] - 2026-05-06

### Deprecated

- `batch_token_checks` is deprecated in favor of `batch_token_state`.
  The new helper returns a typed `TokenState` with `can_transfer`,
  `needs_approval`, and `has_sufficient_balance` predicates instead of
  a positional `(allowance, balance)` tuple, eliminating the field-order
  hazard between the two return shapes. `batch_token_checks` continues to
  work and now delegates to `batch_token_state`; callers will see a
  `#[deprecated]` warning steering them to the new API.

### Changed

- Module-level rustdoc and the `AGENTS.md` helper table now lead with
  `batch_token_state` so new code reaches for the predicate API first.
- `batch_token_state` holds the join logic and error mapping that
  `batch_token_checks` previously duplicated; both now share a single
  implementation.

## [3.2.0] - 2026-05-05

### Added

- `InvalidDomainId` and `InvalidFinalityThreshold` are now re-exported
  from the crate root. Consumers of `DomainId::try_from(u32)` and
  `FinalityThreshold::try_from(u32)` can name the error types directly
  to match on them, propagate them, or wrap them in their own error
  enums without piercing private module paths. Closes #175.

### Changed

- Accessor methods on `FinalityThreshold` and `DomainId` now carry
  `#[must_use]`, so dropping a return value triggers a lint at the
  call site instead of silently discarding the result.
- `InvalidDomainId` and `InvalidFinalityThreshold` now derive
  `thiserror::Error` to match `CctpError`'s style. The `Display`
  message format and `std::error::Error` impl are unchanged — purely
  an internal cleanup.
- The `Default for FinalityThreshold` doc comment no longer restates
  the impl signature; the rationale ("Standard transfers have no fees
  and are the safest option") is now the lead. Closes #176.

## [3.1.2] - 2026-05-05

### Changed

- Crate metadata now names the actual problem space — `description` mentions
  USDC, Circle, CCTP v1/v2, and bridging; `keywords` are the specific terms
  agents and users search for (`cctp`, `usdc`, `circle`, `bridge`, `alloy`)
  rather than generic `defi`/`web3` filler. Improves crates.io discoverability.
- Module-level rustdoc on `lib.rs` now opens with a `Choosing an API` table
  that points readers to `CctpV2Bridge` (recommended), `Cctp` (legacy v1-only
  chains), `mint_if_needed` / `wait_for_receive`, and the v2 message
  inspection types. The V2 Quick Start now appears before the V1 Quick Start.
- `AGENTS.md` is the canonical agent-facing guide; `CLAUDE.md` points to it
  to prevent drift. Adds a v1-vs-v2 decision matrix, a footguns section
  (zeroed v2 nonces, permissionless relayer races, bytes32 recipient padding,
  mainnet/testnet Iris hosts), and a public-API map keyed to source.

## [3.1.1] - 2026-05-05

### Changed

- `batch_token_checks` no longer emits its own `debug!`/`info!` records.
  The inner `Erc20Contract::allowance` and `balance_of` calls already log
  every read, so each batch was producing three duplicate `info!` entries
  per call. Log volume for callers using the batch helper is reduced
  accordingly; structured fields and event names from the inner ERC20
  layer are unchanged.

## [3.1.0] - 2026-05-05

### Fixed

- `record_error_with_context` now actually populates `error.context` on the
  top-level bridge operation spans. The field was previously written to spans
  that did not declare it, making the call a silent no-op.

### Changed

- **Observability schema (breaking for dashboards/alerts)**:
  - Renamed the span field `error.source` to `error.context` on
    `cctp_rs.get_message_sent_event`, `cctp_rs.get_attestation_with_retry`,
    `cctp_rs.get_v2_attestation_with_retry`, and `cctp_rs.deposit_for_burn`.
  - Renamed the v2 attestation polling span from
    `cctp_rs.get_attestation_with_retry` to
    `cctp_rs.get_v2_attestation_with_retry`, and dropped the
    `version = "v2"` attribute (the span name now disambiguates).
  - `record_error` now sets `error.type` to `std::any::type_name::<E>()`
    rather than the first colon-delimited fragment of the Display string.
- **Span helper API**:
  - `spans::send_transaction` now takes `tx_hash: TxHash` instead of
    `tx_hash: &str`, matching the typing used by the other span helpers.
- Switched `message_hash` span attributes to `FixedBytes<32>`'s `Display`
  impl (output now includes the `0x` prefix) and removed `#[inline]` from
  the trivial span constructors.

### Internal

- Documented why v1 and v2 attestation-retry spans require separate
  constructors (`tracing::info_span!` field identifiers must be literals).
- Fixed a typo in the module-level rustdoc example (`poll intervali`).

---

## [3.0.0] - 2026-04-17

### Changed

- **Breaking**: Upgraded the alloy workspace crates (`alloy-contract`,
  `alloy-json-rpc`, `alloy-network`, `alloy-provider`, `alloy-rpc-types`,
  `alloy-transport`, and the dev-only `alloy-signer-local`) from 1.7 to 2.0.
  The cctp-rs source does not use any of the APIs broken by alloy 2.0
  (`GasFiller` unit struct, split `TransactionBuilder` trait, blob-tx
  builder reshuffle, strict `AnyNetwork` signing, exhaustive `ChainConfig`
  matches), so migration is transparent for callers who only depend on
  cctp-rs. Callers who also depend on alloy directly must upgrade their
  alloy dependency to 2.0 in the same step.
- Scoped `dotenvy` to dev-dependencies so it is no longer resolved
  transitively by downstream consumers.

### Removed

- Unused `alloy-dyn-abi` root dependency was dropped — no public API
  impact, but shrinks the dependency surface resolved by downstream.

### Fixed

- Pulled in `rustls-webpki` 0.103.12 via a lockfile refresh to resolve
  RUSTSEC-2026-0098 (name constraints for URI names incorrectly accepted)
  and RUSTSEC-2026-0099 (name constraints accepted for certificates
  asserting a wildcard name).

### Security

- Kept the advisory baseline clean: only the pre-existing transitive
  `derivative` (RUSTSEC-2024-0388) and `paste` (RUSTSEC-2024-0436)
  unmaintained-crate warnings remain, both tracked consistently in
  `deny.toml` and `.cargo/audit.toml`.

### Internal

- Extracted cargo-deny policy from the security workflow heredoc into a
  repo-root `deny.toml`, so contributors can reproduce CI's license and
  advisory checks locally.
- Modernized GitHub Actions workflows to use `Swatinem/rust-cache` for
  Rust-aware caching and `taiki-e/install-action` to install
  `cargo-audit`, `cargo-deny`, and `cargo-semver-checks` from prebuilt
  binaries instead of compiling from source on every run.
- Removed the dependabot ignore rule for alloy semver-major updates so
  future breaking alloy releases surface as reviewable PRs.
- Applied pedantic clippy cleanups (named format-string interpolation,
  `u64::from` in place of `as u64` for widening conversions, type and
  event names in rustdoc backticks).

---

## [2.2.0] - 2026-03-25

### Added

- Agent/tooling-friendly CCTP v2 message inspection API:
  - `ParsedV2Message` for structured decoding of canonical Circle v2 messages
  - `ParsedV2MessageSummary` for JSON-friendly summaries suitable for tool outputs
  - Header helpers for nonce inspection, permissionless relay detection, address extraction, and finality interpretation
  - Burn message body encode/decode helpers for round-tripping canonical transfer payloads
- New `ParseMessageError` type for v2 message parsing failures without changing the existing bridge/runtime error surface

### Changed

- Enabled `serde` support on agent-facing public types including:
  - `PollingConfig`
  - `MintResult`
  - `AttestationResponse`, `V2AttestationResponse`, `V2Message`, `AttestationStatus`
  - `DomainId` and `FinalityThreshold`
  - `MessageHeader` and `BurnMessageV2`
- Expanded README and crate docs with agent-tooling guidance for turning canonical Circle v2 messages into structured JSON
- Avoided redundant message re-encoding in `ParsedV2Message::summary()` when computing both hash and length

### Fixed

- Preserved semver compatibility for `2.x` by keeping `CctpError` unchanged and isolating parser failures in `ParseMessageError`
- Documented forward-compatibility expectations for `DomainId` serde values and clarified that the current address helpers use EVM `bytes32` address conventions

---

## [2.1.4] - 2026-03-21

### Security

- Fixed RUSTSEC-2026-0044, RUSTSEC-2026-0048 (AWS-LC X.509/CRL vulnerabilities) by updating aws-lc-sys to 0.39.0
- Fixed RUSTSEC-2026-0049 (CRL distribution point matching) by updating rustls-webpki to 0.103.10

### Changed

- Bumped minimum tokio version to 1.50

---

## [2.1.3] - 2026-03-18

### Changed

- Updated cargo dependencies to latest stable versions
- Fixed RUSTSEC-2026-0037 security vulnerability in quinn-proto (upgraded to 0.11.14)

---

## [2.1.2] - 2026-02-06

### Changed

- Updated alloy dependencies from 1.4 to 1.6

---

## [2.1.1] - 2026-01-19

### Changed

- Updated alloy dependencies to 1.4

---

## [2.1.0] - 2026-01-04

### Added

- **Typed error detection methods** on `CctpError` for robust error handling:
  - `is_already_relayed()`: Detect when a CCTP message was already relayed by a third party
  - `is_timeout()`: Check for timeout errors (attestation or network)
  - `is_rate_limited()`: Detect HTTP 429 rate limiting responses
  - `is_transient()`: Identify retriable errors (timeouts, rate limits, network issues)

- **Batch token check helpers** for parallel RPC calls:
  - `batch_token_checks()`: Fetch allowance and balance concurrently using `tokio::join!`
  - `batch_token_state()`: Returns structured `TokenState` with helper methods
  - `TokenState` struct with `can_transfer()`, `needs_approval()`, `has_sufficient_balance()`

- **Provider utilities** for gas estimation and configuration:
  - `estimate_gas_with_buffer()`: Gas estimation with configurable safety buffer (default 20%)
  - `calculate_gas_price_with_buffer()`: EIP-1559 gas price calculation with tip buffer
  - `ProviderConfig` struct with presets: `fast_transfer()`, `high_reliability()`, `rate_limited()`

- **Production retry documentation** with patterns for throttle usage and custom retry logic

### Changed

- Refactored `mint_if_needed()` to use typed `is_already_relayed()` instead of string matching

---

## [2.0.0] - 2025-11-26

### Changed

- **BREAKING**: Replaced `Option<u32>, Option<u64>` parameters in `get_attestation()` with `PollingConfig` struct
  - Before: `bridge.get_attestation(hash, Some(20), Some(30)).await?`
  - After: `bridge.get_attestation(hash, PollingConfig::default().with_max_attempts(20).with_poll_interval_secs(30)).await?`

### Added

- New `PollingConfig` struct for self-documenting attestation polling configuration
  - `PollingConfig::default()` - Standard v1 transfers (30 attempts, 60s intervals)
  - `PollingConfig::fast_transfer()` - Optimized for v2 fast transfers (30 attempts, 5s intervals)
  - Builder methods: `with_max_attempts()`, `with_poll_interval_secs()`
  - Helper method: `total_timeout_secs()` to calculate maximum wait time

### Migration Guide

```rust
// V1 - Before (1.x)
let attestation = bridge.get_attestation(message_hash, None, None).await?;
let attestation = bridge.get_attestation(message_hash, Some(20), Some(30)).await?;

// V1 - After (2.0)
use cctp_rs::PollingConfig;
let attestation = bridge.get_attestation(message_hash, PollingConfig::default()).await?;
let attestation = bridge.get_attestation(
    message_hash,
    PollingConfig::default()
        .with_max_attempts(20)
        .with_poll_interval_secs(30),
).await?;

// V2 - Before (1.x)
let (message, attestation) = bridge.get_attestation(tx_hash, None, None).await?;

// V2 - After (2.0)
let (message, attestation) = bridge.get_attestation(
    tx_hash,
    PollingConfig::fast_transfer(),  // or PollingConfig::default() for standard
).await?;
```

---

## [1.1.0] - 2025-11-26

### Changed

- Removed unused chain confirmation configuration from bridge config module
  - Removed `DEFAULT_CONFIRMATION_TIMEOUT` constant
  - Removed `CHAIN_CONFIRMATION_CONFIG` constant
  - Removed `get_chain_confirmation_config()` function
  - These were dead code (marked with `#[allow(dead_code)]`) and never called
  - The SDK correctly delegates finality validation to Circle's attestation service

---

## [1.0.0] - 2025-11-25

### Summary

**First stable release!** This release marks production-readiness with comprehensive CCTP v1 and v2 support, 155 passing tests, and zero clippy warnings.

### Highlights

- **Full CCTP v1/v2 Protocol Support**: Type-safe interfaces for both protocol versions
- **Relayer-Aware API**: Graceful handling of permissionless relay model with `MintResult` enum
- **Production Observability**: Comprehensive OpenTelemetry instrumentation
- **Robust Error Handling**: Consolidated error types with HTTP client timeouts

### Added

- HTTP client timeout (30 seconds) to prevent indefinite hangs on network issues

### Changed

- **BREAKING**: Consolidated duplicate error variants - removed `ChainNotSupported { chain: String }`, keeping only `UnsupportedChain(NamedChain)` for type safety and zero allocation

### Fixed

- Error handling now uses consistent, type-safe error variants throughout

---

## [0.16.0] - 2025-11-25

### Added

- **Relayer-Aware API for CCTP v2**: New methods to gracefully handle the permissionless relay model
  - `MintResult` enum: Distinguishes between `Minted(TxHash)` (we relayed) and `AlreadyRelayed` (third party completed)
  - `is_message_received()`: Query on-chain nonce status to check if transfer is complete
  - `wait_for_receive()`: Poll until transfer completes (by relayer or self), with configurable attempts and interval
  - `mint_if_needed()`: Conservative mint strategy that checks status first, avoiding wasted gas on races

- **AlreadyRelayed Error Variant**: New `CctpError::AlreadyRelayed` for explicit race condition handling

### Documentation

- Added "Relayer-Aware Patterns (V2)" section to README with three usage patterns:
  - Option A: Wait for completion (recommended for most use cases)
  - Option B: Self-relay with graceful race handling
  - Option C: Manual status checking

## [0.15.0] - 2025-11-25

### Changed

- **BREAKING**: Replaced `confirmation_average_time_seconds()` in `CctpV2` trait with two explicit methods:
  - `fast_transfer_confirmation_time_seconds()` - Returns attestation times with Fast Transfer enabled (5-20 seconds)
  - `standard_transfer_confirmation_time_seconds()` - Returns attestation times for Standard Transfer (5 seconds to 8 hours)

### Fixed

- Fixed v2 `CctpV2` trait returning block production times instead of Circle attestation times
  - Previously returned 1-12 seconds (block times)
  - Now returns actual attestation confirmation times based on Circle's documentation
  - Fast Transfer: Ethereum 20s, most chains 8s, high-perf chains 5s
  - Standard Transfer: Ethereum/L2s 19min, Avalanche 20s, Polygon 8min, Linea 8hrs

### Migration Guide

```rust
// Before (0.14.0)
let time = chain.confirmation_average_time_seconds()?; // Returned block times (wrong!)

// After (0.15.0)
// Choose based on your transfer mode:
let time = chain.fast_transfer_confirmation_time_seconds()?;     // Fast Transfer
let time = chain.standard_transfer_confirmation_time_seconds()?; // Standard (default)
```

## [0.14.0] - 2025-11-25

### Added

- **ERC20 Contract Wrapper**: New `Erc20Contract` for token approval and allowance operations
  - `approve()` for granting token spending permission before CCTP burns
  - `allowance()` for checking current approval amounts
  - Essential for the complete CCTP transfer workflow
  - Exposed publicly for direct use

- **Public Chain Addresses Module**: Exposed `chain::addresses` module for accessing contract addresses directly

- **V2-specific span function**: Added `get_v2_attestation_with_retry` for proper v2 observability with `TxHash` instead of message hash

### Changed

- **BREAKING**: Simplified attestation API with type-safe methods per version
  - V1 `Cctp`: `get_attestation(message_hash, ...)` - takes message hash directly
  - V2 `CctpV2`: `get_attestation(tx_hash, ...)` - takes transaction hash directly
  - Removed `AttestationQuery` enum in favor of compile-time type safety
  - Method signatures now clearly indicate what parameters are needed

- **BREAKING**: V2 `get_attestation()` now returns `(Vec<u8>, AttestationBytes)` instead of just `AttestationBytes`
  - Always returns both the canonical message and attestation from Circle's API
  - Eliminates foot-gun where users might use wrong message for minting
  - The `MessageSent` event log contains zeros in the nonce field; Circle fills this in before signing

- **BREAKING**: Removed `get_attestation_with_message()` from V2 - use `get_attestation()` instead

### Removed

- **BREAKING**: Removed `BridgeParams` from public API (was unused)

### Fixed

- Fixed v2 API path: renamed constant to `MESSAGES_PATH_V2` reflecting the actual endpoint format
- V2 now correctly uses `/v2/messages/{domain}?transactionHash={tx}` format
- Fixed critical v2 attestation bug: MessageSent event contains template message with zero nonce; now always uses canonical message from Circle's API

### Migration Guide

```rust
// V1 - Before (0.13.0)
bridge.get_attestation(AttestationQuery::by_message_hash(hash), max_attempts, poll_interval).await?;

// V1 - After (0.14.0)
let attestation = bridge.get_attestation(hash, None, None).await?;

// V2 - Before (0.13.0)
let attestation = bridge.get_attestation(tx_hash, None, None).await?;
let (message, _hash) = bridge.get_message_sent_event(tx_hash).await?; // BUG: wrong message!

// V2 - After (0.14.0)
let (message, attestation) = bridge.get_attestation(tx_hash, None, None).await?;
// Now use `message` (canonical from Circle API) for minting - this is the correct message!
```

## [0.13.0] - 2025-01-24

### Added

- **v1 MessageTransmitterContract**: Exposed `MessageTransmitterContract` in the public API
  - `receive_message_transaction()` for receiving cross-chain messages with attestation
  - `is_nonce_used()` for checking anti-replay protection
  - Matches v2 API symmetry for consistent developer experience

### Changed

- Removed unnecessary `#[allow(dead_code)]` attributes from public contract wrappers
  - Applies to `TokenMessengerContract`, `MessageTransmitterContract` (v1)
  - Applies to `TokenMessengerV2Contract`, `MessageTransmitterV2Contract` (v2)

## [0.12.0] - 2025-01-24

### Added

- **CCTP v2 Support**: Complete implementation of Circle's CCTP v2 protocol
  - Support for 26+ chains (10 mainnet, 6 testnet v2 chains + v1 chains)
  - New v2-only chains: Linea, Sonic, Sei
  - `CctpV2Bridge` and `CctpV2` traits with unified contract addresses
  - Comprehensive test suite with 149 unit tests (100% pass rate)

- **Fast Transfers**: Sub-30 second settlement with optimized finality thresholds
  - Standard transfers: 2000 confirmations (finalized)
  - Fast transfers: 1000 confirmations (confirmed)
  - Optional fee support for fast finality
  - Smart contract method selection based on configuration

- **Programmable Hooks**: Advanced integration capabilities
  - `depositForBurnWithHook()` support for custom destination logic
  - Hook data validation and configuration
  - Priority handling when combined with fast transfers

- **Enhanced Testing Infrastructure**
  - 149 comprehensive unit tests covering all business logic
  - `v2_integration_validation` example for CI/CD validation (no network required)
  - Comprehensive examples: `v2_standard_transfer`, `v2_fast_transfer`
  - Testing guidelines documentation for maintaining test quality

- **Contract Wrappers**: Direct access to type-safe contract interfaces
  - Exported `TokenMessengerContract` (v1) for direct contract interaction
  - Exported `TokenMessengerV2Contract` and `MessageTransmitterV2Contract` (v2)
  - Enables advanced use cases beyond the bridge abstraction
  - All wrappers include OpenTelemetry instrumentation built-in

### Changed

- **BREAKING**: New trait system for v1/v2 polymorphism
  - `CctpBridge` trait enables version-independent code
  - `CctpV1` and `CctpV2` traits for version-specific configuration
  - Clean separation between v1 (legacy) and v2 (current) implementations

- v2-specific modules added: `bridge/v2.rs`, `chain/v2.rs`, `contracts/v2/`

### Documentation

- Added comprehensive testing strategy section to README
- Documented integration test limitations and pragmatic approach
- Added v2-specific usage examples
- Created testing-guidelines.md for maintaining test quality
- Documented fast transfer behavior and fee structures

## [0.11.0] - 2025-01-24

### Changed

- **BREAKING**: Replaced primitive domain ID types with strongly-typed `DomainId` enum
  - Domain IDs are now type-safe with compile-time validation
  - Added `DomainId::from_u32()`, `DomainId::as_u32()`, `DomainId::name()`, and `Display` trait support
  - Improved API ergonomics with meaningful type names instead of raw integers
- Reorganized crate into conceptual modules for better code organization
  - Split code into logical modules: `bridge`, `chain`, `contracts`, `protocol`, and `spans`
  - Improved maintainability and discoverability of functionality
- Updated all Cargo dependencies to latest versions

### Added

- Enhanced OpenTelemetry instrumentation with comprehensive error tracking
  - Added structured error recording with full context preservation
  - Improved observability for debugging production issues
  - Better span hierarchy for cross-chain transfer tracing

## [0.10.1] - 2025-01-21

### Fixed

- **CRITICAL**: Fixed deserialization failure when Circle's Iris API returns `"attestation": "PENDING"` as a string instead of `null`
  - Added custom deserializer that gracefully handles the "PENDING" string by treating it as `None`
  - This fixes production crashes on Arbitrum and other chains where attestation polling would fail with: `JSON error: invalid value: string "PENDING", expected a valid hex string`
  - Enhanced error logging to include raw response body, message hash, and attempt number for better debugging
  - Added comprehensive test coverage for all attestation response formats (valid hex, "PENDING" string, null, empty string, missing field)
  - No breaking changes to public API

### Documentation

- Documented Circle API quirk where attestation field may be "PENDING" instead of null
- Added inline comments explaining the custom deserializer workaround

## [0.10.0] - 2024-11-16

### Changed

- **BREAKING**: Minimized public API surface for improved semver stability
  - Reduced from 30+ exports to 6 core types: `AttestationBytes`, `AttestationResponse`, `AttestationStatus`, `BridgeParams`, `Cctp`, `CctpV1`, `CctpError`, `Result`
  - Removed public exports of domain ID constants, contract address constants, and internal configuration
  - Internal modules (`domain_id`, `message_transmitter`, `token_messenger`) remain accessible via trait methods
- **BREAKING**: Made `BridgeParams` fields private (public getter methods remain available)
- **BREAKING**: Made `TokenMessengerContract.instance` field private
- Refactored `bridge.rs` (589 lines) into clean submodules (`bridge/cctp.rs`, `bridge/params.rs`, `bridge/config.rs`)

### Added

- Exposed `spans` module publicly for advanced OpenTelemetry instrumentation
- Added comprehensive documentation to `spans` module with usage examples

### Fixed

- Fixed all documentation warnings by referencing types instead of private modules

## [0.9.0] - 2025-01-XX

### Changed

- Updated Cargo dependencies to latest versions
- Converted snapshot tests to inline snapshots for better maintainability

### Fixed

- Replaced Anvil provider with HTTP provider in tests for improved stability

## [0.8.1] - 2025-01-XX

### Fixed

- Fixed attestation URL construction

## [0.8.0] - 2025-01-XX

### Changed

- Updated Cargo dependencies to latest versions

### Fixed

- Fixed typo in Iris API URL creation

## [0.7.0] - 2025-01-XX

### Added

- Implemented OpenTelemetry logging with structured spans for observability

### Changed

- Updated Cargo dependencies to latest versions

## [0.6.1] - 2025-01-XX

### Changed

- Refactored attestation to use `Bytes` type for improved type safety

## [0.6.0] - 2025-01-XX

### Changed

- **BREAKING**: Refactored to use `Url` type instead of `String` for API endpoints
- Improved type safety for URL handling

## [0.5.1] - 2025-01-XX

### Changed

- Updated Cargo dependencies

## [0.5.0] - 2025-01-XX

### Added

- Initial support for improved error handling
- Enhanced API ergonomics

## [0.4.0] - 2025-10-14

### Changed

- **BREAKING**: Replaced all `const &str` address constants with typed `const Address` using `address!()` macro
- **BREAKING**: Removed `InvalidAddress` error variant from `CctpError` enum (addresses now validated at compile time)
- Improved structured logging with static messages and event fields for better observability
- Eliminated runtime address parsing overhead with compile-time validation

### Fixed

- Removed potential runtime failures from address string parsing

## [0.3.0]

### Added

- Comprehensive CI/CD pipeline with GitHub Actions
- Security audit workflows with cargo-audit and cargo-deny
- Automated dependency updates with Dependabot
- Code coverage reporting with codecov
- Documentation generation and deployment
- Issue and PR templates for better contribution workflow
- Contribution guidelines and security policy

### Changed

- Updated examples to use modern Alloy provider API
- Improved error handling with detailed error types
- Enhanced documentation with better examples

### Fixed

- All clippy warnings resolved with strict linting
- Deprecated method usage in examples
- Format string improvements for better performance

## [0.2.2] - 2024-01-XX

### Added

- Support for Unichain network
- Comprehensive test suite with 69 tests
- Custom error types replacing anyhow
- Builder pattern for CCTP bridge configuration
- Attestation polling with configurable retry logic
- Multi-chain examples and documentation

### Changed

- Improved type safety with custom error handling
- Better API design with builder patterns
- Enhanced documentation with usage examples

### Fixed

- Removed all unsafe unwrap() and panic! calls
- Fixed chain support validation logic
- Improved error propagation throughout the library

### Security

- Eliminated potential panics from unsafe operations
- Added input validation for addresses and parameters
- Implemented proper error handling for network operations

## [0.2.1] - 2024-01-XX

### Fixed

- Repository field in Cargo.toml manifest

## [0.2.0] - 2024-01-XX

### Added

- Initial CCTP bridge implementation
- Support for major EVM chains (Ethereum, Arbitrum, Base, Optimism, Avalanche, Polygon)
- Circle Iris API integration for attestations
- Contract bindings for TokenMessenger and MessageTransmitter
- Basic examples and documentation

### Changed

- Updated to Alloy 1.0 framework
- Improved chain configuration system

## [0.1.0]

### Added

- Initial project structure
- Basic CCTP domain ID support
- Chain configuration foundations
