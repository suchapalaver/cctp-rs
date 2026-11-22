# Verification architecture review

This document is the durable review surface for cctp-rs formal verification.
`verification/README.md` remains the operator guide: what is proven, how to run
the checks, and how to update the Lean model. This file answers a different
question: what resources make up the verification system, how those resources
depend on each other, and where trust can drift.

The goal is useful verification of this CCTP implementation. The current Lean
slice improves trust in protocol parsing. It does not prove that the whole SDK,
the bridge flow, Circle Iris, relayers, RPC providers, contract deployments, or
published chain metadata are correct.

## Research baseline

The review model is intentionally borrowed from infrastructure and formal
methods practice rather than local preference.

- Terraform's dependency graph treats resources, provider configuration, state,
  diffs, explicit dependencies, orphan resources, and cycle checks as first-class
  planning concepts:
  <https://developer.hashicorp.com/terraform/internals/graph>
- Terraform state maps configuration resources to real remote objects and tracks
  metadata needed for future operations:
  <https://docs.hashicorp.com/terraform/language/state/purpose>
- Mitchell Hashimoto and Armon Dadgar's Terraform design discussion is the
  source for treating graph execution as a deliberately chosen model after
  imperative, finite-state-machine, and actor-style alternatives:
  <https://www.hashicorp.com/en/resources/terraform-fireside-chat-with-mitchell-and-armon>
- Circle's CCTP technical guide is the protocol authority for message structure,
  nonces, Iris attestation, finality thresholds, fees, hooks, and API endpoints:
  <https://developers.circle.com/cctp/references/technical-guide>
- Circle's fee guide is the route-fee authority for Fast Transfer and Standard
  Transfer fee-switch behavior:
  <https://developers.circle.com/cctp/concepts/fees>
- Lean 4 provides machine-checked theorem statements and proof objects, with
  executable definitions usable as reference models:
  <https://docs.lean-lang.org/theorem_proving_in_lean4/>
- TLA+ is the reference lens for precise high-level state-machine design before
  code-level proof:
  <https://lamport.azurewebsites.net/tla/high-level-view.html>
- Alloy is the reference lens for lightweight relational modeling, graph-shaped
  design review, and automated counterexample feedback:
  <https://softwareabstractions.org/>
- CompCert is the reference lens for explicit correspondence between source
  semantics and executable behavior:
  <https://compcert.org/man/>
- QuickCheck is the reference lens for property-based and state-machine testing
  against executable specifications:
  <https://research.chalmers.se/en/publication/154999>

## Engagement model

Expert-panel memos in this repository must be honest about evidence.

- A **published-work panel** names real experts only as lenses grounded in their
  public work. It does not invent quotes, private feedback, or endorsement.
- A **direct-review panel** records actual feedback from named reviewers only
  when the feedback is public or the reviewer has approved the attribution.
- A **role panel** is used when names would imply unavailable endorsement; for
  example, "Circle protocol maintainer" or "Lean proof reviewer".

Every memo should include:

1. Decision under review.
2. Resource graph impact.
3. Current evidence.
4. Expert lens A critique.
5. Expert lens B critique.
6. Synthesis.
7. Action or issue link.

## Graph vocabulary

The verification graph uses these resource kinds:

| Kind | Meaning |
|---|---|
| `provider` | External authority or toolchain trusted by the crate: Circle docs, contracts, Iris, alloy-rs, rustc, Lean, CI, RustSec. |
| `rust_resource` | Rust type, function, module, or test boundary whose behavior is part of a protocol claim. |
| `lean_resource` | Lean definition, theorem, executable model, or generator. |
| `generated_resource` | Artifact emitted from another resource, such as Lean correspondence fixtures. |
| `claim_resource` | README, rustdoc, release note, or issue claim visible to users or maintainers. |
| `policy_resource` | CI job, deny-list, PR template, release rule, or review checklist. |

The graph uses these edge types:

| Edge | Meaning |
|---|---|
| `implements` | Rust behavior implements a protocol or API concept. |
| `mirrors` | One table/model intentionally mirrors another source of truth. |
| `proves` | A theorem establishes a property over a modeled resource. |
| `generates` | One artifact mechanically produces another. |
| `tests` | A test compares observed behavior with expected behavior. |
| `documents` | A claim describes another resource. |
| `trusts` | Correctness depends on an unverified external provider or tool. |
| `blocks_merge` | CI or review policy prevents drift from landing. |
| `observes` | A check samples a live or generated external state. |
| `drifts_from` | Current repo state is known not to match an authority. |

Failure modes:

- **Drift**: an external authority changes and the mirrored Rust, Lean, fixture,
  docs, or CI resource does not change with it.
- **Orphan resource**: a proof, fixture, doc claim, or issue remains in the repo
  without a current source, consumer, owner, or release decision.
- **Shared wrongness**: the model and implementation agree with each other but
  both disagree with the protocol authority.
- **Scope inflation**: a parser proof is described as whole-SDK verification.
- **Hidden assumption**: a runtime dependency such as Iris, RPC, relayers, or
  contract bytecode is trusted but not named in the claim surface.

## Current resource graph

| Resource | Kind | Important edges | Status |
|---|---|---|---|
| Circle CCTP technical guide | `provider` | trusted by message parser, finality, Iris, nonce, and hook claims | Trusted authority; not controlled here. |
| Circle CCTP fee and capability guides | `provider` | trusted by fee, fee-switch, Fast source, and Forwarding destination capability and quote-shape claims | Trusted authority; capability drift checked by the protocol snapshot, while forwarding quote decoding is HTTP-tested against the documented response shape. |
| Circle CCTP contract and stablecoin address guides | `provider` | source the v2 contract, USDC, and EURC address tables in the authority snapshot | Published authority; scheduled drift observes changes, but deployed bytecode and RPC truth remain outside this graph. |
| `scripts/fixtures/circle_cctp_protocol_snapshot.json` | `authority_resource` | independently captures Circle tables plus explicitly identified maintainer bridge-scope policy; consumed separately by Lean and Rust oracle checks | Versioned schema with field-level provenance; not generated from Lean or Rust. |
| `src/protocol/domain_id.rs` | `rust_resource` | implements Circle domain IDs; mirrored by `Domain.lean`; documented by README/rustdoc | Tested against Lean fixtures; current domain-table drift handled by the parser/model/doc graph. |
| `verification/CctpSpec/Domain.lean` | `lean_resource` | mirrors Rust domain table; proves conversion canonicality | Proven for all currently modeled IDs. |
| `verification/CctpSpec/Finality.lean` | `lean_resource` | mirrors `FinalityThreshold`; proves accepted wire values | Proven for 1000 and 2000. |
| `verification/CctpSpec/TransferMode.lean` | `lean_resource` | mirrors transfer-mode dispatch; proves fee/finality/hook relationships | Proven for current Rust mode vocabulary. |
| `verification/CctpSpec/Bytes.lean` and `Message.lean` | `lean_resource` | model CCTP v2 bytes and messages; prove round-trip and canonicality | Proven inside the parser slice. |
| `verification/CctpSpec/Fixtures.lean` | `lean_resource` | generates fixture JSON; self-checks accept/reject vectors | CI-checked by Lean workflow. |
| `tests/fixtures/lean/cctp_v2_vectors.json` | `generated_resource` | generated by Lean; consumed by Rust correspondence tests | Freshness enforced by CI. |
| `tests/lean_model_correspondence.rs` | `rust_resource` | tests Rust parser behavior against Lean fixture verdicts | Narrows, but does not close, the spec/implementation gap. |
| `tests/authority_oracle.rs` | `rust_resource` | compares the independent authority artifact separately with Lean executable outputs and production Rust APIs | Blocks shared Lean/Rust table mistakes relative to the captured authority; mutation coverage pins both comparator paths. |
| `.github/workflows/lean.yml` | `policy_resource` | blocks merge on proof errors, fixture drift, and proof-surface escape hatches | Required status check surface. |
| Pull Request API Compatibility CI job | `policy_resource` | compares the candidate Rust API with its actual merge base and blocks unapproved SemVer regressions; a maintainer-reviewed `semver-major-approved` label changes the release type without skipping analysis | Required pull-request signal; independent of the published-release comparison. |
| `verification/README.md` | `claim_resource` | documents proven/tested/assumed scope | Primary claim boundary for verification. |
| `README.md`, rustdoc, `AGENTS.md` | `claim_resource` | document public SDK and verification claims | Must link back to `verification/README.md` and this file when claims change. |
| GitHub-hosted Ubuntu 24.04 runners | `provider` | trusted by CI, Lean, release, protocol drift, live smoke, security, and dependency-refresh workflows | Explicit runner baseline; changes should be reviewed as CI maintenance. |
| Iris fee and Fast Transfer allowance helpers | `rust_resource` | observe Circle Iris fee and allowance endpoints; document Fast vs Standard transfer preflight | Mock-tested for decoding and HTTP errors; live smoke tests are opt-in. |
| `ForwardingServiceRoute`, forwarding hook data, and Iris forwarding quotes | `rust_resource` | compose existing route/capability policy with Circle's version-1 `cctp-forward` frame and `?forward=true` quote shape; feed existing hook transfer modes while preserving a zero `destinationCaller` | Route rejection, exact hook bytes, quote decoding, and fee composition are Rust-tested. Circle service liveness and destination execution remain trusted. Stellar's distinct non-EVM `CctpForwarder`/Soroban flow is explicitly out of scope. |
| Iris attestation wire normalization | `rust_resource` | converts public raw v1/v2 DTOs into exclusive ready, pending, failed, or malformed operational states before bridge decisions; feeds the v2 selector modeled by `Lifecycle.lean` | Missing, empty, and status-inconsistent payloads are deterministic HTTP-tested and property-checked across arbitrary payloads and longer reordered sibling lists; legacy raw DTO shapes remain public for compatibility. Iris authenticity and future wire behavior remain trusted. |
| `CctpV2Route` and chain config | `rust_resource` | implements route validity, directional capabilities, and supported-chain/domain policy; route validity is mirrored by `Route.lean` | The 21-mainnet/21-testnet bridge table is checked exhaustively against the Lean model; Fast and Forwarding capabilities are snapshot-tested against Circle. Alloy `NamedChain` behavior remains trusted; parse-only decisions are recorded in the README matrix. |
| `verification/CctpSpec/Route.lean` | `lean_resource` | mirrors bridge chain support, environment, domain mapping, directional capabilities, modeled asset support, and `CctpV2Route::new` validity | Proves valid endpoints map to modeled domains, differ, share an environment, and remain valid when reversed; executable capability/asset tables are independently authority-tested. |
| `CctpTransferAsset` and asset-aware bridge helpers | `rust_resource` | implement bridge asset policy and token-address lookup; trust Circle token address tables and asset rollout docs | Rust-tested for USDC/EURC/USYC boundaries; non-USDC Iris fee endpoint drift tracked by #53. |
| `CctpV2Bridge` attestation/mint lifecycle | `rust_resource` | implements burn/Iris/receive behavior; mirrored by `Lifecycle.lean`; deterministic HTTP/RPC tests observe the production boundary | Decision safety is modeled and correspondence-tested; external liveness, Iris authenticity, RPC truth, contract execution, and mint confirmation remain trusted. |
| `verification/CctpSpec/Lifecycle.lean` | `lean_resource` | models placeholder event, canonical attestation selection, destination receipt, mint submission, and relayer-race transitions | Proves mint authorization and terminal-state safety; generates exhaustive state/event and bounded-exhaustive Iris-selection vectors. |
| Live protocol drift check | `policy_resource` | observes Circle authority and detects domain, token, capability, CCTP contract-address, USDC-address, and EURC-address drift | Implemented as scheduled/manual CI with an explicitly refreshed checked-in snapshot; ordinary PR CI remains offline. |

## Claim ledger

Use this ledger when reviewing verification-related PRs.

| Claim class | Current answer |
|---|---|
| Proven | Domain/finality conversion canonicality, transfer-mode dispatch properties, bridge route validity, attestation/mint decision safety over the modeled lifecycle, big-endian codecs, bytes32 EVM address canonicality, and CCTP v2 parser round-trip/canonicality over the Lean model. |
| Tested | Rust parser acceptance and exact typed rejection-category correspondence with Lean-generated fixtures; route-table and normalized Iris-selection correspondence; property-generated v1/v2 normalization partitions plus longer-list selection and reordering invariance; independent Lean/authority and Rust/authority agreement for domains, environments, bridge scope, directional capabilities, modeled asset addresses, and v2 contract addresses; exhaustive modeled chain-pair correspondence and exhaustive lifecycle state/event fixture coverage; deterministic HTTP/RPC coverage of `get_attestation` (including missing, empty, and status-inconsistent Iris payloads), `is_message_received`, and `mint_if_needed`; generated-fixture freshness; fee/allowance helpers. |
| Assumed | Circle's captured docs and the reviewed SDK-policy mapping are correct; deployed bytecode matches published addresses; Iris responses are authentic and its undifferentiated `failed` category is classified correctly; Iris returns current fees and allowance; alloy-rs primitives, `keccak256`, and `NamedChain` behavior are correct; contracts, RPC providers, relayers, transaction inclusion, rustc/LLVM, Lean, Lake, GitHub-hosted runners, hardware, and CI behave correctly. |
| Stale | The committed authority snapshot can lag Circle between scheduled observations and maintainer refreshes; the drift workflow and #35 roadmap own that window. Non-USDC Iris fee and allowance endpoint drift is tracked by #53. |
| Unowned | Direct external expert review notes do not yet have a template beyond this document. |

## Expert-panel memos

### Artifact Location

Decision: where should the durable review artifact live?

Expert lens A: Terraform graph/state practice, grounded in HashiCorp's internals
docs and Hashimoto/Dadgar's public design discussion, says the artifact must
describe both the graph and the current state mapping. A standalone graph
diagram is not enough because drift is about stale bindings between desired
configuration and external reality.

Expert lens B: Daniel Jackson's Alloy/lightweight-formal-methods lens says the
first artifact should be human-readable and conceptually sharp before it becomes
machine-readable. Premature formalization can preserve the wrong abstraction.

Synthesis: keep this prose-first architecture document as the root review
artifact. Add `verification/resources.toml` only after at least one review cycle
shows which nodes and edges are stable enough to check mechanically.

Action: this file is the first artifact; a manifest is deferred until #41 has
been exercised on at least one implementation slice.

### Expert Representation

Decision: how should expert panels be represented?

Expert lens A: Leslie Lamport's specification lens rewards precise claims over
performative authority. If an expert did not review the repo, the document must
not imply they did.

Expert lens B: Lean/CompCert-style verification practice, using Leonardo de
Moura and Xavier Leroy as public-work lenses, makes the theorem statement and
trusted computing base the review surface. Names are useful only when they point
reviewers to a concrete school of criticism.

Synthesis: use published-work panels for design pressure, role panels for
needed but unavailable feedback, and direct-review panels only when actual
review happened.

Action: every future memo must label which kind of panel it is.

### First Review Target

Decision: should the first review target be the current parser slice or the
next bridge frontier?

Expert lens A: Xavier Leroy's CompCert correspondence lens says existing claims
must first be audited for the exact relation between model and executable
behavior. Otherwise new proofs can stack on an unclear claim boundary.

Expert lens B: John Hughes's QuickCheck/state-machine testing lens says value
rises when the model reaches behavior users actually exercise. For this crate,
the bridge lifecycle is a higher-value operational target than another
parser-only slice.

Synthesis: the parser, route validity, and attestation/mint decision lifecycle
now form the initial verification stack. The next proof slice should follow the
priority order in `verification/README.md` without inflating these safety claims
into external-liveness or whole-bridge verification.

Action: do not add a new proof slice until the PR description says which
resources and edges changed.

### Machine-Readable Graph

Decision: should the resource graph be machine-readable, prose-first, or both?

Expert lens A: Terraform state argues for machine-readable bindings when drift
can be detected mechanically.

Expert lens B: Lamport/Jackson-style modeling practice argues that choosing the
right state space comes before choosing the file format.

Synthesis: prose-first now; machine-readable only for edges that CI can check,
such as Lean fixture freshness, proof escape hatches, domain table parity, or
external Circle doc drift.

Action: if #29 lands a protocol drift check, its checked bindings are candidates
for `verification/resources.toml`.

## Review checklist

For any PR that touches CCTP protocol parsing, bridge behavior, chain metadata,
verification files, CI policy, or public verification claims:

1. Name the changed resources.
2. Name the changed edges.
3. Update the proven/tested/assumed/stale/unowned ledger if the claim boundary
   changed.
4. Run the relevant checks from `verification/README.md`.
5. Link an issue for every real, concrete, harmful, out-of-scope, untracked
   smell found during review.

## Follow-up map

- #35: maintain protocol currency as an ongoing roadmap item.
- #53: track non-USDC Iris fee and allowance endpoints.
