-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
-- SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Finality

/-!
# CCTP v2 transfer-mode dispatch

Models `TransferMode` from `src/bridge/transfer_mode.rs`: the valid
combinations of finality, fee policy, and hook data, and how each maps
to the `minFinalityThreshold` / `maxFee` / `hookData` arguments of the
on-chain `depositForBurn` family. The theorems pin the dispatch table: fast
modes — and only fast modes — request threshold 1000, legacy standard modes
send a zero `maxFee`, fee-switch modes preserve their explicit cap, and hook
data is carried exactly by the hook variants.
-/

namespace CctpSpec

/-- CCTP v2 burn-call configuration. Mirrors `TransferMode` in
`src/bridge/transfer_mode.rs`. Fees are USDC atomic units. -/
inductive TransferMode where
  | standard
  | standardWithFee (maxFee : Nat)
  | fast (maxFee : Nat)
  | standardWithHook (hookData : List UInt8)
  | standardWithFeeAndHook (maxFee : Nat) (hookData : List UInt8)
  | fastWithHook (maxFee : Nat) (hookData : List UInt8)
  | standardForwarded (maxFee : Nat)
  | fastForwarded (maxFee : Nat)
deriving DecidableEq, Repr

namespace TransferMode

/-- Circle's canonical composable version-1 `cctp-forward` frame with an empty
payload. Mirrors `ForwardingServiceHookData` in `src/primitives.rs`. -/
def forwardingServiceHookV1 : List UInt8 :=
  [0x63, 0x63, 0x74, 0x70, 0x2d, 0x66, 0x6f, 0x72,
   0x77, 0x61, 0x72, 0x64, 0x00, 0x00, 0x00, 0x00,
   0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
   0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]

/-- The finality threshold the mode requests from Circle. Mirrors
`TransferMode::finality_threshold`. -/
def finalityThreshold : TransferMode → FinalityThreshold
  | .standard | .standardWithFee _ | .standardWithHook _
  | .standardWithFeeAndHook _ _ | .standardForwarded _ => .standard
  | .fast _ | .fastWithHook _ _ | .fastForwarded _ => .fast

/-- Whether the mode requests fast (confirmed) finality. Mirrors
`TransferMode::is_fast`. -/
def isFast : TransferMode → Bool
  | .fast _ | .fastWithHook _ _ | .fastForwarded _ => true
  | .standard | .standardWithFee _ | .standardWithHook _
  | .standardWithFeeAndHook _ _ | .standardForwarded _ => false

/-- Whether the mode supplies an explicit fee cap. Mirrors
`TransferMode::has_explicit_fee_cap`. -/
def hasExplicitFeeCap : TransferMode → Bool
  | .standard | .standardWithHook _ => false
  | .standardWithFee _ | .standardWithFeeAndHook _ _
  | .fast _ | .fastWithHook _ _ | .standardForwarded _ | .fastForwarded _ => true

/-- Whether Circle's Forwarding Service will relay the destination mint.
Mirrors `TransferMode::is_forwarding`. -/
def isForwarding : TransferMode → Bool
  | .standardForwarded _ | .fastForwarded _ => true
  | _ => false

/-- The fee cap sent on-chain; zero for modes without an explicit cap.
Mirrors `TransferMode::max_fee`. -/
def maxFee : TransferMode → Nat
  | .standardWithFee f | .standardWithFeeAndHook f _
  | .fast f | .fastWithHook f _ | .standardForwarded f | .fastForwarded f => f
  | .standard | .standardWithHook _ => 0

/-- The hook payload, when the mode carries one. Mirrors
`TransferMode::hook_data`. -/
def hookData? : TransferMode → Option (List UInt8)
  | .standardWithHook h | .standardWithFeeAndHook _ h | .fastWithHook _ h => some h
  | .standardForwarded _ | .fastForwarded _ => some forwardingServiceHookV1
  | .standard | .standardWithFee _ | .fast _ => none

/-- The `snake_case` name used in generated fixtures. -/
def jsonName : TransferMode → String
  | .standard => "standard"
  | .standardWithFee _ => "standard_with_fee"
  | .fast _ => "fast"
  | .standardWithHook _ => "standard_with_hook"
  | .standardWithFeeAndHook _ _ => "standard_with_fee_and_hook"
  | .fastWithHook _ _ => "fast_with_hook"
  | .standardForwarded _ => "standard_forwarded"
  | .fastForwarded _ => "fast_forwarded"

/-- Dispatch: a mode requests fast finality iff it is a fast variant. -/
theorem isFast_iff_finality_fast (m : TransferMode) :
    m.isFast = true ↔ m.finalityThreshold = .fast := by
  cases m <;> simp [isFast, finalityThreshold]

/-- Dispatch: the wire value of the requested threshold is 1000 for fast
modes and 2000 for standard modes — never anything else. -/
theorem finality_wire_value (m : TransferMode) :
    m.finalityThreshold.toU32 = if m.isFast then 1000 else 2000 := by
  cases m <;> rfl

/-- Dispatch: modes without an explicit fee cap send zero `maxFee` on-chain. -/
theorem maxFee_eq_zero_of_no_explicit_fee_cap {m : TransferMode}
    (h : m.hasExplicitFeeCap = false) :
    m.maxFee = 0 := by
  cases m <;> simp_all [hasExplicitFeeCap, maxFee]

/-- Dispatch: hook data is carried exactly by the `*WithHook` variants. -/
theorem hookData_isSome_iff (m : TransferMode) :
    m.hookData?.isSome = true ↔
      (∃ h, m = .standardWithHook h) ∨
      (∃ f h, m = .standardWithFeeAndHook f h) ∨
      (∃ f h, m = .fastWithHook f h) ∨
      (∃ f, m = .standardForwarded f) ∨
      (∃ f, m = .fastForwarded f) := by
  cases m <;> simp [hookData?]

/-- Dispatch: forwarding is represented only by the two typed forwarding
variants. -/
theorem isForwarding_iff (m : TransferMode) :
    m.isForwarding = true ↔
      (∃ f, m = .standardForwarded f) ∨ (∃ f, m = .fastForwarded f) := by
  cases m <;> simp [isForwarding]

end TransferMode

end CctpSpec
