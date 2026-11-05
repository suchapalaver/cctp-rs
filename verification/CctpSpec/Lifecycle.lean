-- SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Message

/-!
# CCTP v2 attestation and mint lifecycle

An executable safety model for the lifecycle driven by `CctpV2Bridge` after a
burn transaction has been submitted. The model distinguishes the placeholder
message visible in the source-chain event from the canonical message returned
by Iris, and models both ways a permissionless relayer can win the destination
race.

This is a per-invocation safety model, not a liveness proof. Iris, RPC
providers, deployed contracts, relayers, and transaction inclusion remain in
the trusted computing base.
-/

namespace CctpSpec

inductive LifecycleState where
  | burnSubmitted
  | placeholderEventObserved
  | awaitingAttestation
  | canonicalAttestationReady
  | mintRequired
  | mintSubmitted
  | alreadyRelayed
  | attestationExpired
  | unavailable
  | failed
deriving DecidableEq, Repr

namespace LifecycleState

def all : List LifecycleState :=
  [.burnSubmitted, .placeholderEventObserved, .awaitingAttestation,
   .canonicalAttestationReady, .mintRequired, .mintSubmitted,
   .alreadyRelayed, .attestationExpired, .unavailable, .failed]

theorem mem_all (state : LifecycleState) : state ∈ all := by
  cases state <;> decide

def jsonName : LifecycleState → String
  | .burnSubmitted => "burn_submitted"
  | .placeholderEventObserved => "placeholder_event_observed"
  | .awaitingAttestation => "awaiting_attestation"
  | .canonicalAttestationReady => "canonical_attestation_ready"
  | .mintRequired => "mint_required"
  | .mintSubmitted => "mint_submitted"
  | .alreadyRelayed => "already_relayed"
  | .attestationExpired => "attestation_expired"
  | .unavailable => "unavailable"
  | .failed => "failed"

/-- Terminal for one bridge invocation. A caller may start a later invocation
after an unavailable response; that retry is outside this transition system. -/
def isTerminal : LifecycleState → Bool
  | .mintSubmitted | .alreadyRelayed | .attestationExpired | .unavailable
  | .failed => true
  | _ => false

end LifecycleState

inductive LifecycleEvent where
  | observePlaceholderEvent
  | requestCanonicalAttestation
  | irisPending
  | irisComplete
  | irisExpired
  | irisUnavailable
  | irisFailed
  | destinationReceived
  | destinationUnused
  | submitMint
  | relayerWonRace
  | mintFailed
deriving DecidableEq, Repr

namespace LifecycleEvent

def all : List LifecycleEvent :=
  [.observePlaceholderEvent, .requestCanonicalAttestation, .irisPending,
   .irisComplete, .irisExpired, .irisUnavailable, .irisFailed,
   .destinationReceived, .destinationUnused, .submitMint,
   .relayerWonRace, .mintFailed]

theorem mem_all (event : LifecycleEvent) : event ∈ all := by
  cases event <;> decide

def jsonName : LifecycleEvent → String
  | .observePlaceholderEvent => "observe_placeholder_event"
  | .requestCanonicalAttestation => "request_canonical_attestation"
  | .irisPending => "iris_pending"
  | .irisComplete => "iris_complete"
  | .irisExpired => "iris_expired"
  | .irisUnavailable => "iris_unavailable"
  | .irisFailed => "iris_failed"
  | .destinationReceived => "destination_received"
  | .destinationUnused => "destination_unused"
  | .submitMint => "submit_mint"
  | .relayerWonRace => "relayer_won_race"
  | .mintFailed => "mint_failed"

end LifecycleEvent

/-- One valid lifecycle transition. `none` means that the event is invalid in
the current state. Direct Iris lookup from `burnSubmitted` models the normal
SDK path; observing the placeholder event first is an optional diagnostic path. -/
def lifecycleStep : LifecycleState → LifecycleEvent → Option LifecycleState
  | .burnSubmitted, .observePlaceholderEvent => some .placeholderEventObserved
  | .burnSubmitted, .requestCanonicalAttestation => some .awaitingAttestation
  | .placeholderEventObserved, .requestCanonicalAttestation => some .awaitingAttestation
  | .awaitingAttestation, .irisPending => some .awaitingAttestation
  | .awaitingAttestation, .irisComplete => some .canonicalAttestationReady
  | .awaitingAttestation, .irisExpired => some .attestationExpired
  | .awaitingAttestation, .irisUnavailable => some .unavailable
  | .awaitingAttestation, .irisFailed => some .failed
  | .canonicalAttestationReady, .destinationReceived => some .alreadyRelayed
  | .canonicalAttestationReady, .destinationUnused => some .mintRequired
  | .mintRequired, .submitMint => some .mintSubmitted
  | .mintRequired, .relayerWonRace => some .alreadyRelayed
  | .mintRequired, .mintFailed => some .failed
  | _, _ => none

/-- Accepts a source-event message into the diagnostic placeholder state only
when its modeled header carries the all-zero v2 placeholder nonce. -/
def observeSourceEvent (header : MessageHeader) : Option LifecycleState :=
  if header.hasPlaceholderNonce then some .placeholderEventObserved else none

theorem observe_source_event_iff_placeholder (header : MessageHeader) :
    observeSourceEvent header = some .placeholderEventObserved ↔
      header.hasPlaceholderNonce = true := by
  simp [observeSourceEvent]

/-- A source-event placeholder can never authorize a destination mint. -/
theorem placeholder_event_cannot_submit_mint :
    lifecycleStep .placeholderEventObserved .submitMint = none := rfl

/-- Mint submission is reachable only after the canonical attestation was
ready and the destination nonce was observed unused. -/
theorem mint_submitted_only_from_required {state event}
    (h : lifecycleStep state event = some .mintSubmitted) :
    state = .mintRequired ∧ event = .submitMint := by
  cases state <;> cases event <;> simp_all [lifecycleStep]

theorem mint_required_only_after_destination_unused {state event}
    (h : lifecycleStep state event = some .mintRequired) :
    state = .canonicalAttestationReady ∧ event = .destinationUnused := by
  cases state <;> cases event <;> simp_all [lifecycleStep]

/-- Both permissionless-relayer races converge to the same successful result. -/
theorem already_relayed_paths :
    lifecycleStep .canonicalAttestationReady .destinationReceived =
      some .alreadyRelayed ∧
    lifecycleStep .mintRequired .relayerWonRace = some .alreadyRelayed := by
  decide

/-- Expired, unavailable, failed, already-relayed, and submitted outcomes do
not authorize further transitions within the invocation. -/
theorem terminal_states_do_not_transition {state}
    (h : state.isTerminal = true) (event : LifecycleEvent) :
    lifecycleStep state event = none := by
  cases state <;> cases event <;> simp_all [LifecycleState.isTerminal, lifecycleStep]

/-! ## Iris multi-message selection -/

inductive ModelAttestationStatus where
  | complete
  | pending
  | pendingConfirmations
  | failed
deriving DecidableEq, Repr

structure AttestationEntry where
  status : ModelAttestationStatus
  hasMessage : Bool
  hasAttestation : Bool
deriving DecidableEq, Repr

namespace AttestationEntry

def isUsable (entry : AttestationEntry) : Bool :=
  entry.status == .complete && entry.hasMessage && entry.hasAttestation

def isPending (entry : AttestationEntry) : Bool :=
  entry.status == .pending || entry.status == .pendingConfirmations

/-- Every status/data-presence combination used for exhaustive selection
vectors. -/
def all : List AttestationEntry :=
  [⟨.complete, false, false⟩, ⟨.complete, false, true⟩,
   ⟨.complete, true, false⟩, ⟨.complete, true, true⟩,
   ⟨.pending, false, false⟩, ⟨.pending, false, true⟩,
   ⟨.pending, true, false⟩, ⟨.pending, true, true⟩,
   ⟨.pendingConfirmations, false, false⟩,
   ⟨.pendingConfirmations, false, true⟩,
   ⟨.pendingConfirmations, true, false⟩,
   ⟨.pendingConfirmations, true, true⟩,
   ⟨.failed, false, false⟩, ⟨.failed, false, true⟩,
   ⟨.failed, true, false⟩, ⟨.failed, true, true⟩]

theorem mem_all (entry : AttestationEntry) : entry ∈ all := by
  cases entry with
  | mk status hasMessage hasAttestation =>
      cases status <;> cases hasMessage <;> cases hasAttestation <;> decide

end AttestationEntry

inductive AttestationSelection where
  | ready
  | pending
  | attestationMissing
  | messageMissing
  | failed
  | empty
deriving DecidableEq, Repr

namespace AttestationSelection

def jsonName : AttestationSelection → String
  | .ready => "ready"
  | .pending => "pending"
  | .attestationMissing => "attestation_missing"
  | .messageMissing => "message_missing"
  | .failed => "failed"
  | .empty => "empty"

end AttestationSelection

/-- Mirrors the precedence in Rust `select_v2_attestation`: usable complete,
then any pending sibling, then malformed complete, then failed, then empty. -/
def selectAttestation (entries : List AttestationEntry) : AttestationSelection :=
  if entries.any AttestationEntry.isUsable then .ready
  else if entries.any AttestationEntry.isPending then .pending
  else match entries.find? (fun entry => entry.status == .complete) with
    | some entry =>
        if !entry.hasAttestation then .attestationMissing else .messageMissing
    | none =>
        if entries.any (fun entry => entry.status == .failed) then .failed else .empty

theorem usable_complete_selects_ready {entries : List AttestationEntry}
    (h : entries.any AttestationEntry.isUsable = true) :
    selectAttestation entries = .ready := by
  simp [selectAttestation, h]

theorem pending_selected_when_no_usable {entries : List AttestationEntry}
    (hUsable : entries.any AttestationEntry.isUsable = false)
    (hPending : entries.any AttestationEntry.isPending = true) :
    selectAttestation entries = .pending := by
  simp [selectAttestation, hUsable, hPending]

end CctpSpec
