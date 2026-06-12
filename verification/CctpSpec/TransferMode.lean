-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Finality

/-!
# CCTP v2 transfer-mode dispatch

Models `TransferMode` from `src/bridge/transfer_mode.rs`: the four valid
combinations of finality (fast vs. standard) and hook data, and how each maps
to the `minFinalityThreshold` / `maxFee` / `hookData` arguments of the
on-chain `depositForBurn` family. The theorems pin the dispatch table: fast
modes — and only fast modes — request threshold 1000, standard modes always
send a zero `maxFee`, and hook data is carried exactly by the `*WithHook`
variants.
-/

namespace CctpSpec

/-- CCTP v2 burn-call configuration. Mirrors `TransferMode` in
`src/bridge/transfer_mode.rs`. Fees are USDC atomic units. -/
inductive TransferMode where
  | standard
  | fast (maxFee : Nat)
  | standardWithHook (hookData : List UInt8)
  | fastWithHook (maxFee : Nat) (hookData : List UInt8)
deriving DecidableEq, Repr

namespace TransferMode

/-- The finality threshold the mode requests from Circle. Mirrors
`TransferMode::finality_threshold`. -/
def finalityThreshold : TransferMode → FinalityThreshold
  | .standard | .standardWithHook _ => .standard
  | .fast _ | .fastWithHook _ _ => .fast

/-- Whether the mode requests fast (confirmed) finality. Mirrors
`TransferMode::is_fast`. -/
def isFast : TransferMode → Bool
  | .fast _ | .fastWithHook _ _ => true
  | .standard | .standardWithHook _ => false

/-- The fast-transfer fee cap sent on-chain; zero for standard modes.
Mirrors `TransferMode::max_fee`. -/
def maxFee : TransferMode → Nat
  | .fast f | .fastWithHook f _ => f
  | .standard | .standardWithHook _ => 0

/-- The hook payload, when the mode carries one. Mirrors
`TransferMode::hook_data`. -/
def hookData? : TransferMode → Option (List UInt8)
  | .standardWithHook h | .fastWithHook _ h => some h
  | .standard | .fast _ => none

/-- The `snake_case` name used in generated fixtures. -/
def jsonName : TransferMode → String
  | .standard => "standard"
  | .fast _ => "fast"
  | .standardWithHook _ => "standard_with_hook"
  | .fastWithHook _ _ => "fast_with_hook"

/-- Dispatch: a mode requests fast finality iff it is a fast variant. -/
theorem isFast_iff_finality_fast (m : TransferMode) :
    m.isFast = true ↔ m.finalityThreshold = .fast := by
  cases m <;> simp [isFast, finalityThreshold]

/-- Dispatch: the wire value of the requested threshold is 1000 for fast
modes and 2000 for standard modes — never anything else. -/
theorem finality_wire_value (m : TransferMode) :
    m.finalityThreshold.toU32 = if m.isFast then 1000 else 2000 := by
  cases m <;> rfl

/-- Dispatch: standard modes always send a zero `maxFee` on-chain. -/
theorem maxFee_eq_zero_of_not_fast {m : TransferMode} (h : m.isFast = false) :
    m.maxFee = 0 := by
  cases m <;> simp_all [isFast, maxFee]

/-- Dispatch: hook data is carried exactly by the `*WithHook` variants. -/
theorem hookData_isSome_iff (m : TransferMode) :
    m.hookData?.isSome = true ↔
      (∃ h, m = .standardWithHook h) ∨ (∃ f h, m = .fastWithHook f h) := by
  cases m <;> simp [hookData?]

end TransferMode

end CctpSpec
