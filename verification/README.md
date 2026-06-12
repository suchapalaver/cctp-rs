# CCTP invariant verification (Lean 4)

This directory contains a Lean 4 model of the security-critical CCTP v2
protocol invariants the SDK encodes in Rust, machine-checked proofs about
that model, and a generator for correspondence fixtures consumed by the
Rust test suite (`tests/lean_model_correspondence.rs`).

The approach follows the layered-redundancy strategy described in Vitalik
Buterin's ["A shallow dive into formal
verification"](https://vitalik.eth.limo/general/2026/05/18/fv.html): the
goal is **not** "Lean proves the SDK correct". The goal is to specify the
highest-risk invariants redundantly — through Rust types, a readable
executable spec, Lean proofs, generated test vectors, and explicit
trust-boundary documentation — so that a regression has to slip past
several independent layers at once. See issue
[#11](https://github.com/suchapalaver/cctp-rs/issues/11).

## First slice: scope

| Invariant | Lean module | Rust module covered |
|---|---|---|
| `DomainId` ↔ `u32` is a bijection on exactly the 21 announced IDs; `is_evm` classification | `CctpSpec/Domain.lean` | `src/protocol/domain_id.rs` |
| `FinalityThreshold` ↔ `u32` accepts exactly 1000 (fast) and 2000 (standard) | `CctpSpec/Finality.lean` | `src/protocol/finality.rs` |
| `TransferMode` dispatch: fast variants ⇔ threshold 1000; standard variants send zero `maxFee`; hooks carried exactly by `*WithHook` | `CctpSpec/TransferMode.lean` | `src/bridge/transfer_mode.rs` |
| Big-endian field codec round-trips; `bytes32` EVM-address words are canonical (12 zero leading bytes) | `CctpSpec/Bytes.lean` | `src/protocol/message.rs` (`decode_address_word`, field codecs) |
| Canonical v2 message structure: header (148 B), burn body (≥ 228 B), strict parse | `CctpSpec/Message.lean` | `src/protocol/message.rs` (`MessageHeader`, `BurnMessageV2`, `ParsedV2Message`) |

The two central theorems, proved at header, body, and full-message level:

- **Round-trip**: `decode (encode m) = some m` for every well-formed `m`
  (`Message.decode_encode`).
- **Canonicality**: `decode raw = some m → encode m = raw`
  (`Message.encode_of_decode`), with the corollary that the parser is
  injective on accepted inputs (`Message.decode_injective`). This is what
  makes rejecting non-zero address padding a security property: no two
  distinct raw byte strings — which would hash to different on-chain
  message hashes — can parse to the same value.

## Layout

```
verification/
├── lean-toolchain          # pinned Lean version (elan format)
├── lakefile.toml           # lake build config: CctpSpec lib + gen_vectors exe
├── CctpSpec.lean           # library root
├── CctpSpec/
│   ├── Bytes.lean          # BE codec + bytes32 address-word canonicality
│   ├── Domain.lean         # DomainId model + conversion theorems
│   ├── Finality.lean       # FinalityThreshold model + conversion theorems
│   ├── TransferMode.lean   # TransferMode dispatch model + theorems
│   ├── Message.lean        # header/body/message codec + round-trip/canonicality
│   └── Fixtures.lean       # self-checking fixture construction
└── Main.lean               # gen_vectors entry point

tests/fixtures/lean/cctp_v2_vectors.json   # committed generator output
tests/lean_model_correspondence.rs         # Rust side of the correspondence
```

## Running the checks

With [elan](https://github.com/leanprover/elan) (any platform — the
toolchain version is pinned by `lean-toolchain`):

```sh
cd verification
lake build          # type-checks the model and re-checks every proof
```

On NixOS / with nix, `elan` is available in the dev shell (`nix develop`),
or use nixpkgs' Lean directly (must match `lean-toolchain`):

```sh
cd verification
nix shell nixpkgs#lean4 nixpkgs#gcc -c lake build
```

Regenerate the fixtures after changing the model:

```sh
cd verification
lake exe gen_vectors > ../tests/fixtures/lean/cctp_v2_vectors.json
```

Run the Rust side:

```sh
cargo nextest run --test lean_model_correspondence --all-features
```

CI (`.github/workflows/lean.yml`) builds the proofs and fails if the
committed fixtures differ from freshly generated ones, so the fixture file
cannot drift from the model. The normal Rust CI runs the correspondence
test, so the production parser cannot drift from the fixtures.

## The redundancy layers

Mapped to the checklist in issue #11:

1. **Type system and memory safety (Rust).** Already enforced in
   production, independent of this directory: `DomainId` and
   `FinalityThreshold` are closed enums so invalid wire values cannot be
   represented; `TransferMode` makes invalid fee/finality/hook combinations
   unrepresentable; `CctpV2Route::new` is a fallible constructor rejecting
   unsupported chains, self-routes, and mainnet/testnet mixes; `UsdcAmount`
   confines six-decimal parsing to one boundary; parse failures are typed
   (`ParseMessageError`, `InvalidDomainId`, `InvalidFinalityThreshold`).
   Memory safety is Rust's; the crate contains no `unsafe`.
2. **Lean specification and proof.** The modules above. All proofs are
   `sorry`-free and re-checked by `lake build` in CI.
3. **Readable reference implementation.** The Lean model doubles as the
   executable reference: `Message.decode`/`Message.encode` are written for
   clarity (lists of bytes, explicit offsets, no performance tricks) and
   actually execute — the fixture generator runs them on every vector.
4. **Tests and examples.** The generator emits accept/reject vectors with
   expected field values; `tests/lean_model_correspondence.rs` replays them
   against the production parser. Vectors are self-checked against the
   model during generation, so a vector that contradicts the model cannot
   be committed.
5. **Equivalence/correspondence checking.** The same fixture tests are the
   equivalence check between the optimized production parser and the
   reference model, over representative and edge-case inputs (real Circle
   traffic, max-`uint256` values, non-EVM domains, placeholder nonces,
   boundary-length and mutated-padding rejects).
6. **Security properties.** Canonicality (above) is the security property:
   accepting a non-canonical recipient word is unreachable in the model,
   and the fixtures pin the production parser to the same behavior. The
   zero-nonce footgun is modeled as `hasPlaceholderNonce` and exercised by
   the `solana_source_placeholder_nonce` vector.
7. **End-to-end interaction invariants.** Out of the proof boundary in
   this slice (see Roadmap). Route validation, attestation selection, and
   `mint_if_needed` remain covered by Rust unit/integration tests only.
8. **AI-assisted proof workflow.** See below.
9. **Trusted computing base.** See below.
10. **Process and documentation.** The proven/tested/assumed table below
    is part of the review checklist for protocol-parsing changes
    (see Maintenance).

## Proven vs tested vs assumed

**Proven** (Lean theorems, re-checked by CI on every change under
`verification/`):

| Claim | Theorem |
|---|---|
| `DomainId` u32 conversion round-trips and is canonical/injective on exactly the 21 announced IDs | `DomainId.fromU32_toU32`, `DomainId.toU32_of_fromU32`, `DomainId.toU32_injective` |
| Solana and Starknet Testnet are exactly the non-EVM domains | `DomainId.isEvm_eq_false_iff` |
| `FinalityThreshold` accepts exactly 1000/2000, fast = 1000, standard = 2000 | `FinalityThreshold.fromU32_toU32`, `toU32_of_fromU32`, `toU32_fast`, `toU32_standard` |
| Fast transfer modes — and only they — request threshold 1000; standard modes send zero `maxFee`; hooks ⇔ `*WithHook` | `TransferMode.finality_wire_value`, `maxFee_eq_zero_of_not_fast`, `hookData_isSome_iff` |
| Big-endian field encoding round-trips; every byte string is the canonical encoding of its value | `natOfBe_beBytes`, `beBytes_natOfBe` |
| `bytes32` EVM-address words decode iff canonically zero-padded, and decoding is strict | `decodeAddressWord_encodeAddressWord`, `encodeAddressWord_of_decode` |
| Header, burn body, and full message decode∘encode = id on well-formed values | `MessageHeader.decode_encode`, `BurnBody.decode_encode`, `Message.decode_encode` |
| Accepted raw bytes re-encode byte-for-byte (strict canonical parser); parser injective on accepted inputs | `MessageHeader.encode_of_decode`, `BurnBody.encode_of_decode`, `Message.encode_of_decode`, `Message.decode_injective` |

**Tested** (Lean-generated fixtures replayed against production Rust, plus
the existing unit suite):

| Claim | Where |
|---|---|
| Production `DomainId::from_u32` / `is_evm` / serde names match the model over domains 0–27 and sentinels | `domain_id_conversion_matches_lean_model` |
| Production `FinalityThreshold::from_u32` matches the model | `finality_threshold_conversion_matches_lean_model` |
| Production `TransferMode` dispatch matches the model | `transfer_mode_dispatch_matches_lean_model` |
| Production parser accepts the model's accept vectors with identical fields, re-encodes byte-for-byte, hashes to `keccak256(raw)` | `accepted_messages_parse_to_lean_model_fields` |
| Production parser rejects the model's reject vectors, naming the offending field | `rejected_messages_fail_to_parse` |
| Real Circle Iris message (Arbitrum→Base) parses identically in model and production | first accept vector |

**Assumed** (outside the model; documented, not checked here):

| Assumption | Notes |
|---|---|
| `keccak256` (alloy implementation) is correct | The model does not define hashing; the Rust test checks `message_hash() == keccak256(raw)` using alloy itself. |
| Header/body `version` fields are carried, not validated | Production parses any `u32` version with the v1 layout; a future Circle format bump could misparse. Pinned by the `max_values_unvalidated_fields` vector; candidate for a stricter parser. |
| Burn-body address words use EVM padding for **all** source domains | Production (and the model mirroring it) rejects bodies whose `burn_token`/`mint_recipient`/`message_sender` words are not EVM-padded. Genuine non-EVM-source burn messages (e.g. Solana, whose words are full 32-byte pubkeys) would be rejected by `ParsedV2Message::parse` today. |
| Circle Iris returns the canonical message; the on-chain `MessageSent` event has a zeroed nonce | Modeled only as the `hasPlaceholderNonce` flag; selection of the canonical message lives in `CctpV2Bridge::get_attestation` and is covered by Rust tests. |
| Route validity, finality timing, fee quotes, relayer behavior, RPC providers | Out of scope for this slice. |

## Trusted computing base

Everything below is **unverified** and trusted:

- **Alloy** (primitives, providers, contract bindings) and its `keccak256`.
- **reqwest / tokio / HTTP stack** and Circle Iris itself.
- **Chain RPC providers, relayers, and the CCTP contracts' bytecode.**
- **rustc / LLVM and the Lean 4 compiler, kernel, and `lake`** — a
  miscompilation or kernel bug is outside every layer here.
- **The correspondence gap**: Lean models *a specification of* the Rust
  code, not the Rust code. The fixture tests narrow this gap empirically;
  they cannot close it. A behavior both the model and the parser get wrong
  the same way will not be caught.
- **Hardware, OS, CI infrastructure.**

## Formal-verification failure modes (and mitigations)

- **Proving the wrong theorem.** The highest risk. Mitigation: theorem
  statements are deliberately short and human-reviewable; this README names
  each one and what it is supposed to mean; PR review for changes under
  `verification/` must read the *statements*, not the proofs.
- **Weakening the proof target.** A PR could relax `WellFormed` or a
  decoder check and the proofs would still pass. Mitigation: the fixture
  diff in CI — weakening the model changes the generated vectors or the
  model's verdicts, which shows up as a fixture diff and a failing Rust
  correspondence test.
- **Fixture staleness.** The committed JSON could be edited by hand or
  left stale. Mitigation: CI regenerates and `git diff --exit-code`s it;
  the generator self-checks every vector against the model before emitting.
- **Spec/implementation divergence on unexercised inputs.** Fixtures are
  finite. Mitigation: vectors target the boundaries the proofs identify
  (length cutoffs, padding bytes, domain-table gaps, max values); extend
  them when a new edge case is found rather than only fixing the code.
- **Silent scope shrinkage.** "Verified" drifting into marketing.
  Mitigation: the proven/tested/assumed table above is the only claim
  surface; README and AGENTS.md link here instead of restating it.

## AI-assisted proof workflow

AI tooling (Claude or similar) may draft Lean specs, proofs, and fixtures.
The controls that make that safe:

- **Humans review theorem statements and model definitions** — the
  `def`s and `theorem` signatures in `CctpSpec/` are the trust surface.
  Proof *bodies* need less scrutiny: the Lean kernel checks them.
- **CI re-checks everything** (`lake build` + fixture diff + Rust
  correspondence) on every PR touching `verification/`, so an AI-generated
  change cannot weaken the target without a visible diff in model
  definitions, fixtures, or both.
- **No `sorry`/`admit`** may be committed; `lake build` treats the model
  as a library with proofs, and a `sorry` is a build warning that review
  must treat as a failure (CI greps for it).

## Maintenance

When **adding a CCTP domain** (see also AGENTS.md "Adding chain support"):

1. Add the variant to `DomainId` in `verification/CctpSpec/Domain.lean`
   (constructor, `toU32`, `fromU32`, `jsonName`, and `isEvm` if non-EVM).
   The conversion theorems re-prove automatically (`cases`-based); a typo
   in the table fails `lake build`.
2. Extend `domainSweep` in `Fixtures.lean` if the new ID exceeds 27.
3. Regenerate fixtures (command above) and commit the diff.
4. `cargo nextest run --test lean_model_correspondence` — it fails until
   the Rust `DomainId` table agrees with the model.

When **changing message parsing** (`src/protocol/message.rs`): change the
model in `CctpSpec/Message.lean` to match, re-prove (`lake build`),
regenerate fixtures, and update the proven/tested/assumed table if the
contract itself moved. If a proof becomes unprovable, that is the point —
the change broke round-trip or canonicality, and the PR should explain why
that is acceptable before weakening the theorem.

When **only Rust internals change** (no wire-format change): nothing to do
here; the correspondence test keeps passing.

## Roadmap (next slices, in rough priority order)

1. **Route validity** (`CctpV2Route`): model `supports_cctp_v2`,
   mainnet/testnet partitioning, and no-self-route; prove route domain IDs
   always land in the `DomainId` table.
2. **`UsdcAmount` decimal parsing**: six-decimal grammar, overflow, and
   display round-trip.
3. **Attestation selection**: state-machine model of
   event-message vs Iris-canonical-message selection (the zero-nonce
   footgun) across `get_attestation`/`mint_if_needed`.
4. **Non-EVM body addresses**: decide whether the parser should accept
   full-width `bytes32` body words for non-EVM source domains, then model
   the chosen behavior (see Assumed table).
