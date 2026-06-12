-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

/-!
# Byte-level encoding primitives

Big-endian fixed-width natural-number encoding and the CCTP `bytes32`
EVM-address padding convention, with the round-trip and canonicality
theorems the message model is built on.

The corresponding production code lives in `src/protocol/message.rs`
(`u32::to_be_bytes` / `U256::to_be_bytes::<32>` field encoding and
`decode_address_word` / `check_canonical_address_word`).
-/

namespace CctpSpec

/-- Big-endian encoding of `n` into exactly `w` bytes, most significant byte
first. Values `≥ 256 ^ w` are truncated modulo `256 ^ w`, mirroring how the
production encoder is only ever called with values that fit the field width. -/
def beBytes : (w : Nat) → Nat → List UInt8
  | 0, _ => []
  | w + 1, n => UInt8.ofNat (n / 256 ^ w) :: beBytes w (n % 256 ^ w)

/-- Big-endian value of a byte string, most significant byte first. -/
def natOfBe : List UInt8 → Nat
  | [] => 0
  | b :: bs => b.toNat * 256 ^ bs.length + natOfBe bs

@[simp]
theorem length_beBytes (w n : Nat) : (beBytes w n).length = w := by
  induction w generalizing n with
  | zero => rfl
  | succ w ih => simp [beBytes, ih]

theorem natOfBe_lt (bs : List UInt8) : natOfBe bs < 256 ^ bs.length := by
  induction bs with
  | nil => simp [natOfBe]
  | cons b bs ih =>
    have hb : b.toNat < 256 := by simpa using b.toNat_lt
    have e1 : (b.toNat + 1) * 256 ^ bs.length
        = b.toNat * 256 ^ bs.length + 256 ^ bs.length := Nat.succ_mul _ _
    have h2 : (b.toNat + 1) * 256 ^ bs.length ≤ 256 * 256 ^ bs.length :=
      Nat.mul_le_mul_right _ hb
    have e2 : 256 ^ (bs.length + 1) = 256 ^ bs.length * 256 := Nat.pow_succ 256 bs.length
    simp only [natOfBe, List.length_cons]
    omega

/-- Decoding a big-endian encoding recovers the value (assuming it fits). -/
theorem natOfBe_beBytes (w n : Nat) (h : n < 256 ^ w) : natOfBe (beBytes w n) = n := by
  induction w generalizing n with
  | zero =>
    simp only [Nat.pow_zero] at h
    simp [beBytes, natOfBe]
    omega
  | succ w ih =>
    have hp : 0 < 256 ^ w := Nat.pow_pos (by omega)
    have hpow : 256 ^ (w + 1) = 256 ^ w * 256 := Nat.pow_succ 256 w
    have hdiv : n / 256 ^ w < 256 := Nat.div_lt_of_lt_mul (by omega)
    have hofNat : (UInt8.ofNat (n / 256 ^ w)).toNat = n / 256 ^ w := by
      simp [UInt8.toNat_ofNat']
      omega
    simp only [beBytes, natOfBe, length_beBytes]
    rw [ih _ (Nat.mod_lt _ hp), hofNat, Nat.mul_comm (n / 256 ^ w) (256 ^ w)]
    exact Nat.div_add_mod n (256 ^ w)

/-- Encoding the big-endian value of a byte string at its own width is the
identity: every byte string is the canonical encoding of its value. -/
theorem beBytes_natOfBe (bs : List UInt8) : beBytes bs.length (natOfBe bs) = bs := by
  induction bs with
  | nil => rfl
  | cons b bs ih =>
    have hlt : natOfBe bs < 256 ^ bs.length := natOfBe_lt bs
    have hp : 0 < 256 ^ bs.length := Nat.pow_pos (by omega)
    have hdiv : (b.toNat * 256 ^ bs.length + natOfBe bs) / 256 ^ bs.length = b.toNat := by
      rw [Nat.mul_comm b.toNat, Nat.add_comm, Nat.add_mul_div_left _ _ hp,
        Nat.div_eq_of_lt hlt, Nat.zero_add]
    have hmod : (b.toNat * 256 ^ bs.length + natOfBe bs) % 256 ^ bs.length = natOfBe bs := by
      rw [Nat.mul_comm b.toNat, Nat.add_comm, Nat.add_mul_mod_self_left,
        Nat.mod_eq_of_lt hlt]
    simp only [beBytes, natOfBe]
    rw [hdiv, hmod, UInt8.ofNat_toNat, ih]

/-- `slice bs start len` is the sub-list of `bs` beginning at `start` with
length `len` (shorter if `bs` runs out). -/
def slice (bs : List UInt8) (start len : Nat) : List UInt8 :=
  (bs.drop start).take len

theorem length_slice (bs : List UInt8) (start len : Nat)
    (h : start + len ≤ bs.length) : (slice bs start len).length = len := by
  simp [slice]
  omega

/-- Splitting off a slice: dropping `a` bytes is the same as taking the
`b`-byte slice at `a` and then dropping `a + b` bytes. The workhorse for
reassembling a parsed message back into its raw bytes. -/
theorem drop_eq_slice_append (bs : List UInt8) (a b c : Nat) (h : c = a + b) :
    bs.drop a = slice bs a b ++ bs.drop c := by
  subst h
  rw [show slice bs a b = (bs.drop a).take b from rfl, ← List.drop_drop,
    List.take_append_drop]

/-- A slice from `0` covering the whole list is the list itself. -/
theorem slice_zero_length (bs : List UInt8) (len : Nat) (h : bs.length ≤ len) :
    slice bs 0 len = bs := by
  simp [slice, List.take_of_length_le h]

/-- A prefix slice of a known-length prefix is that prefix. -/
theorem slice_append_left (xs ys : List UInt8) (len : Nat) (h : xs.length = len) :
    slice (xs ++ ys) 0 len = xs := by
  subst h
  simp [slice]

/-- A slice starting at or past a known-length prefix slices the suffix. -/
theorem slice_append_right (xs ys : List UInt8) (start len : Nat)
    (h : xs.length ≤ start) :
    slice (xs ++ ys) start len = slice ys (start - xs.length) len := by
  simp [slice, List.drop_append, List.drop_eq_nil_of_le h]

/-- Dropping at or past a known-length prefix drops into the suffix. -/
theorem drop_append_of_le (xs ys : List UInt8) (n : Nat) (h : xs.length ≤ n) :
    (xs ++ ys).drop n = ys.drop (n - xs.length) := by
  simp [List.drop_append, List.drop_eq_nil_of_le h]

/-! ## CCTP `bytes32` EVM-address words

CCTP v2 burn-message bodies store 20-byte EVM addresses as `bytes32` words
whose 12 leading bytes must be zero. The production parser rejects words with
non-zero padding so that `decode (encode m) = m` *and* `decode raw = some m →
encode m = raw` both hold (see `decode_address_word` in
`src/protocol/message.rs`).
-/

/-- The 12 zero bytes that prefix a canonical EVM address word. -/
def addressPadding : List UInt8 := List.replicate 12 0

/-- Encodes a 20-byte EVM address as a canonical CCTP `bytes32` word. -/
def encodeAddressWord (addr : List UInt8) : List UInt8 :=
  addressPadding ++ addr

/-- Decodes a canonical CCTP `bytes32` EVM-address word. Rejects words that
are not exactly 32 bytes or whose 12 leading bytes are not all zero. -/
def decodeAddressWord (w : List UInt8) : Option (List UInt8) :=
  if w.length = 32 ∧ w.take 12 = addressPadding then some (w.drop 12) else none

theorem decodeAddressWord_encodeAddressWord (addr : List UInt8)
    (h : addr.length = 20) :
    decodeAddressWord (encodeAddressWord addr) = some addr := by
  have hpad : addressPadding.length = 12 := by simp [addressPadding]
  simp [decodeAddressWord, encodeAddressWord, h, hpad, List.take_left' hpad,
    List.drop_left' hpad]

/-- Canonicality: any accepted word is byte-for-byte the encoding of the
address it decodes to. This is the invariant that makes the parser strict —
no two distinct raw words decode to the same address. -/
theorem encodeAddressWord_of_decode {w addr : List UInt8}
    (h : decodeAddressWord w = some addr) :
    encodeAddressWord addr = w ∧ addr.length = 20 := by
  unfold decodeAddressWord at h
  split at h
  · rename_i hcond
    obtain ⟨hlen, htake⟩ := hcond
    cases h
    refine ⟨?_, by simp [hlen]⟩
    unfold encodeAddressWord
    rw [← htake, List.take_append_drop]
  · cases h

end CctpSpec
