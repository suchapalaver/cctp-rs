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
| Circle CCTP fee guide | `provider` | trusted by fee and fee-switch claims | Trusted authority; protocol drift tracked by #31, #32, #33, and #35. |
| `src/protocol/domain_id.rs` | `rust_resource` | implements Circle domain IDs; mirrored by `Domain.lean`; documented by README/rustdoc | Tested against Lean fixtures; known protocol drift tracked by #28 and #38. |
| `verification/CctpSpec/Domain.lean` | `lean_resource` | mirrors Rust domain table; proves conversion canonicality | Proven for currently modeled IDs; stale wording tracked by #38. |
| `verification/CctpSpec/Finality.lean` | `lean_resource` | mirrors `FinalityThreshold`; proves accepted wire values | Proven for 1000 and 2000. |
| `verification/CctpSpec/TransferMode.lean` | `lean_resource` | mirrors transfer-mode dispatch; proves fee/finality/hook relationships | Proven for current Rust mode vocabulary. |
| `verification/CctpSpec/Bytes.lean` and `Message.lean` | `lean_resource` | model CCTP v2 bytes and messages; prove round-trip and canonicality | Proven inside the parser slice. |
| `verification/CctpSpec/Fixtures.lean` | `lean_resource` | generates fixture JSON; self-checks accept/reject vectors | CI-checked by Lean workflow. |
| `tests/fixtures/lean/cctp_v2_vectors.json` | `generated_resource` | generated by Lean; consumed by Rust correspondence tests | Freshness enforced by CI. |
| `tests/lean_model_correspondence.rs` | `rust_resource` | tests Rust parser behavior against Lean fixture verdicts | Narrows, but does not close, the spec/implementation gap. |
| `.github/workflows/lean.yml` | `policy_resource` | blocks merge on proof errors, fixture drift, and proof-surface escape hatches | Required status check surface. |
| `verification/README.md` | `claim_resource` | documents proven/tested/assumed scope | Primary claim boundary for verification. |
| `README.md`, rustdoc, `AGENTS.md` | `claim_resource` | document public SDK and verification claims | Must link back to `verification/README.md` and this file when claims change. |
| `CctpV2Route` and chain config | `rust_resource` | implements route validity and supported-chain policy | Not formally modeled yet; tracked by #15, #31, and #34. |
| `CctpV2Bridge` attestation/mint lifecycle | `rust_resource` | implements burn/Iris/receive behavior | Not formally modeled yet; tracked by #42. |
| Live protocol drift check | `policy_resource` | would observe Circle authority and detect repo drift | Proposed by #29. |

## Claim ledger

Use this ledger when reviewing verification-related PRs.

| Claim class | Current answer |
|---|---|
| Proven | Domain/finality conversion canonicality, transfer-mode dispatch properties, big-endian codecs, bytes32 EVM address canonicality, and CCTP v2 parser round-trip/canonicality over the Lean model. |
| Tested | Rust parser correspondence with Lean-generated fixtures; parser rejection behavior over modeled edge cases; generated-fixture freshness in CI. |
| Assumed | Circle docs are current; Iris returns canonical messages; alloy-rs primitives and `keccak256` are correct; contracts, RPC providers, relayers, rustc/LLVM, Lean, Lake, OS, hardware, and CI behave correctly. |
| Stale | Domain coverage wording and tables can lag Circle's published support; tracked by #28, #29, #35, and #38. |
| Unowned | Whole bridge lifecycle verification was not tracked before #42; direct external expert review notes do not yet have a template beyond this document. |

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

Synthesis: first use this graph to audit the current parser slice as
calibration, then move to route validity (#15), followed by attestation/mint
lifecycle (#42).

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

- #15: model `CctpV2Route` validity in Lean.
- #28: update the protocol domain table for current Circle domains.
- #29: add a Circle CCTP protocol drift check.
- #31: align fast-transfer support with Circle source capability.
- #32: add Fast Transfer allowance preflight.
- #33: support Standard Transfer fee switch.
- #34: triage bridge route support for current CCTP EVM domains.
- #35: maintain protocol currency as an ongoing roadmap item.
- #38: remove stale announced-domain claims.
- #42: model the v2 attestation and mint lifecycle.
