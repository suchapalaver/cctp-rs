-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

/-!
# CCTP v2 finality thresholds

Models `FinalityThreshold` from `src/protocol/finality.rs`: the two finality
levels CCTP v2 attestations distinguish, with their `u32` wire values
(1000 = fast/confirmed, 2000 = standard/finalized). The theorems pin the
conversion: only exactly 1000 and 2000 decode, and each decodes to the
intended level.
-/

namespace CctpSpec

/-- CCTP v2 finality threshold. Mirrors `FinalityThreshold` in
`src/protocol/finality.rs`. -/
inductive FinalityThreshold where
  | fast
  | standard
deriving DecidableEq, Repr

namespace FinalityThreshold

/-- The `u32` wire value: 1000 for fast (confirmed), 2000 for standard
(finalized). -/
def toU32 : FinalityThreshold → Nat
  | .fast => 1000
  | .standard => 2000

/-- Typed view of a `u32` wire value. Mirrors `FinalityThreshold::from_u32`:
every value other than exactly 1000 or 2000 is rejected. -/
def fromU32 : Nat → Option FinalityThreshold
  | 1000 => some .fast
  | 2000 => some .standard
  | _ => none

/-- The `snake_case` serde name used by the Rust SDK's JSON serialization. -/
def jsonName : FinalityThreshold → String
  | .fast => "fast"
  | .standard => "standard"

/-- Round-trip: each threshold's wire value decodes back to itself. -/
theorem fromU32_toU32 (t : FinalityThreshold) : fromU32 (toU32 t) = some t := by
  cases t <;> rfl

/-- Canonicality: an accepted wire value is exactly the encoding of the
threshold it decodes to. -/
theorem toU32_of_fromU32 {n : Nat} {t : FinalityThreshold}
    (h : fromU32 n = some t) : toU32 t = n := by
  unfold fromU32 at h
  split at h <;> cases h <;> rfl

/-- Fast means 1000, by definition — pinned so a transposed constant would
fail the proof check. -/
theorem toU32_fast : toU32 .fast = 1000 := rfl

/-- Standard means 2000. -/
theorem toU32_standard : toU32 .standard = 2000 := rfl

end FinalityThreshold

end CctpSpec
