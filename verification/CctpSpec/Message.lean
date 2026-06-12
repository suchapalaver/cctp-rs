-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Bytes
import CctpSpec.Domain
import CctpSpec.Finality

/-!
# Canonical CCTP v2 message structure

Models the strict CCTP v2 message parser from `src/protocol/message.rs`:
the 148-byte header (`MessageHeader`), the burn-message body
(`BurnMessageV2`, minimum 228 bytes plus dynamic hook data), and the
combined message (`ParsedV2Message`).

The two theorems proved for each layer are the parser's contract:

* **round-trip** — `decode (encode m) = some m` for every well-formed `m`;
* **canonicality** — `decode raw = some m → encode m = raw`, i.e. the parser
  accepts only the exact canonical byte string for each message, so distinct
  raw inputs never alias to the same parsed value.

Canonicality is what makes rejecting non-zero `bytes32` address padding a
security property rather than pedantry: without it, two different raw
messages (one canonical, one with stray padding bits) would parse to the
same value while hashing differently on-chain.
-/

namespace CctpSpec

/-- CCTP v2 message header. Field-for-field mirror of `MessageHeader` in
`src/protocol/message.rs`; `bytes32` fields are raw 32-byte strings with no
padding constraint, exactly as on the wire. -/
structure MessageHeader where
  version : Nat
  sourceDomain : DomainId
  destinationDomain : DomainId
  nonce : List UInt8
  sender : List UInt8
  recipient : List UInt8
  destinationCaller : List UInt8
  minFinalityThreshold : Nat
  finalityThresholdExecuted : Nat
deriving DecidableEq, Repr

namespace MessageHeader

/-- Fixed header size in bytes: 4 + 4 + 4 + 32 + 32 + 32 + 32 + 4 + 4. -/
def size : Nat := 148

/-- Structural well-formedness: numeric fields fit their wire width and
`bytes32` fields are exactly 32 bytes. -/
def WellFormed (h : MessageHeader) : Prop :=
  h.version < 2 ^ 32 ∧
  h.nonce.length = 32 ∧
  h.sender.length = 32 ∧
  h.recipient.length = 32 ∧
  h.destinationCaller.length = 32 ∧
  h.minFinalityThreshold < 2 ^ 32 ∧
  h.finalityThresholdExecuted < 2 ^ 32

/-- Encodes the header per Circle's v2 message format. Mirrors
`MessageHeader::encode`. -/
def encode (h : MessageHeader) : List UInt8 :=
  beBytes 4 h.version ++
  beBytes 4 h.sourceDomain.toU32 ++
  beBytes 4 h.destinationDomain.toU32 ++
  h.nonce ++
  h.sender ++
  h.recipient ++
  h.destinationCaller ++
  beBytes 4 h.minFinalityThreshold ++
  beBytes 4 h.finalityThresholdExecuted

/-- Decodes an exactly-148-byte header. Domain IDs must be known; all other
fields are carried as-is. Mirrors `MessageHeader::decode` applied to the
148-byte prefix of a message. -/
def decode (bs : List UInt8) : Option MessageHeader :=
  if bs.length = size then
    match DomainId.fromU32 (natOfBe (slice bs 4 4)),
          DomainId.fromU32 (natOfBe (slice bs 8 4)) with
    | some src, some dst =>
      some {
        version := natOfBe (slice bs 0 4)
        sourceDomain := src
        destinationDomain := dst
        nonce := slice bs 12 32
        sender := slice bs 44 32
        recipient := slice bs 76 32
        destinationCaller := slice bs 108 32
        minFinalityThreshold := natOfBe (slice bs 140 4)
        finalityThresholdExecuted := natOfBe (slice bs 144 4)
      }
    | _, _ => none
  else none

/-- True when the nonce is the all-zero placeholder that v2 `MessageSent`
events carry before Iris assigns the real nonce. Mirrors
`MessageHeader::has_placeholder_nonce`. -/
def hasPlaceholderNonce (h : MessageHeader) : Bool :=
  h.nonce.all (· == 0)

/-- True when any relayer may complete the message (zero destination
caller). Mirrors `MessageHeader::is_permissionless`. -/
def isPermissionless (h : MessageHeader) : Bool :=
  h.destinationCaller.all (· == 0)

/-- The requested finality threshold, when it matches a known CCTP mode.
Mirrors `MessageHeader::requested_finality`. -/
def requestedFinality (h : MessageHeader) : Option FinalityThreshold :=
  FinalityThreshold.fromU32 h.minFinalityThreshold

/-- The finality threshold Circle attested at, when it matches a known
mode. Mirrors `MessageHeader::attested_finality`. -/
def attestedFinality (h : MessageHeader) : Option FinalityThreshold :=
  FinalityThreshold.fromU32 h.finalityThresholdExecuted

theorem length_encode (h : MessageHeader) (wf : h.WellFormed) :
    h.encode.length = size := by
  obtain ⟨-, hn, hs, hr, hd, -, -⟩ := wf
  simp [encode, size, hn, hs, hr, hd]

/-- Round-trip: every well-formed header decodes from its own encoding. -/
theorem decode_encode (h : MessageHeader) (wf : h.WellFormed) :
    decode h.encode = some h := by
  obtain ⟨hv, hn, hs, hr, hd, hm, hf⟩ := wf
  have hlen : h.encode.length = size := length_encode h ⟨hv, hn, hs, hr, hd, hm, hf⟩
  unfold decode
  rw [hlen, if_pos rfl]
  simp [encode, slice_append_right, slice_append_left, slice_zero_length,
    hn, hs, hr, hd,
    natOfBe_beBytes 4 h.version (by simpa using hv),
    natOfBe_beBytes 4 h.minFinalityThreshold (by simpa using hm),
    natOfBe_beBytes 4 h.finalityThresholdExecuted (by simpa using hf),
    natOfBe_beBytes 4 h.sourceDomain.toU32 (by simpa using h.sourceDomain.toU32_lt),
    natOfBe_beBytes 4 h.destinationDomain.toU32
      (by simpa using h.destinationDomain.toU32_lt),
    DomainId.fromU32_toU32]

/-- Canonicality: an accepted byte string is exactly the encoding of the
header it decodes to, and that header is well-formed. -/
theorem encode_of_decode {bs : List UInt8} {h : MessageHeader}
    (hdec : decode bs = some h) : h.encode = bs ∧ h.WellFormed := by
  unfold decode at hdec
  split at hdec
  · rename_i hlen
    have hlen' : bs.length = 148 := hlen
    split at hdec
    · rename_i src dst hsrc hdst
      cases hdec
      have hbe : ∀ start len, start + len ≤ bs.length →
          beBytes len (natOfBe (slice bs start len)) = slice bs start len := by
        intro start len hle
        have hb := beBytes_natOfBe (slice bs start len)
        rwa [length_slice bs start len hle] at hb
      have hlt : ∀ start, start + 4 ≤ bs.length →
          natOfBe (slice bs start 4) < 2 ^ 32 := by
        intro start hle
        have hl := natOfBe_lt (slice bs start 4)
        rw [length_slice bs start 4 hle] at hl
        simpa using hl
      constructor
      · dsimp only [encode]
        rw [DomainId.toU32_of_fromU32 hsrc, DomainId.toU32_of_fromU32 hdst]
        rw [hbe 0 4 (by omega), hbe 4 4 (by omega), hbe 8 4 (by omega),
          hbe 140 4 (by omega), hbe 144 4 (by omega)]
        simp only [List.append_assoc]
        conv =>
          rhs
          rw [show bs = bs.drop 0 from rfl,
            drop_eq_slice_append bs 0 4 4 rfl,
            drop_eq_slice_append bs 4 4 8 rfl,
            drop_eq_slice_append bs 8 4 12 rfl,
            drop_eq_slice_append bs 12 32 44 rfl,
            drop_eq_slice_append bs 44 32 76 rfl,
            drop_eq_slice_append bs 76 32 108 rfl,
            drop_eq_slice_append bs 108 32 140 rfl,
            drop_eq_slice_append bs 140 4 144 rfl,
            drop_eq_slice_append bs 144 4 148 rfl]
        rw [List.drop_eq_nil_of_le (by omega)]
        simp
      · exact ⟨hlt 0 (by omega), length_slice bs 12 32 (by omega),
          length_slice bs 44 32 (by omega), length_slice bs 76 32 (by omega),
          length_slice bs 108 32 (by omega), hlt 140 (by omega),
          hlt 144 (by omega)⟩
    · cases hdec
  · cases hdec

end MessageHeader

/-- CCTP v2 burn-message body. Field-for-field mirror of `BurnMessageV2` in
`src/protocol/message.rs`; address-like fields are raw 32-byte words because
their EVM projection is domain-dependent, amounts are `uint256`. -/
structure BurnBody where
  version : Nat
  burnToken : List UInt8
  mintRecipient : List UInt8
  amount : Nat
  messageSender : List UInt8
  maxFee : Nat
  feeExecuted : Nat
  expirationBlock : Nat
  hookData : List UInt8
deriving DecidableEq, Repr

namespace BurnBody

/-- Minimum body size in bytes (everything but the dynamic hook data):
4 + 32 + 32 + 32 + 32 + 32 + 32 + 32. -/
def minSize : Nat := 228

/-- Structural well-formedness: numeric fields fit their wire width and
address-like words are exactly 32 bytes. Hook data is unconstrained. -/
def WellFormed (b : BurnBody) : Prop :=
  b.version < 2 ^ 32 ∧
  b.burnToken.length = 32 ∧
  b.mintRecipient.length = 32 ∧
  b.amount < 2 ^ 256 ∧
  b.messageSender.length = 32 ∧
  b.maxFee < 2 ^ 256 ∧
  b.feeExecuted < 2 ^ 256 ∧
  b.expirationBlock < 2 ^ 256

/-- Encodes the body per Circle's v2 burn-message format. Mirrors
`BurnMessageV2::encode`. -/
def encode (b : BurnBody) : List UInt8 :=
  beBytes 4 b.version ++
  b.burnToken ++
  b.mintRecipient ++
  beBytes 32 b.amount ++
  b.messageSender ++
  beBytes 32 b.maxFee ++
  beBytes 32 b.feeExecuted ++
  beBytes 32 b.expirationBlock ++
  b.hookData

/-- Decodes a burn-message body: at least 228 bytes, preserving the three
address-like words raw; everything past byte 228 is hook data. Domain-aware
EVM padding checks happen at the full-message layer. Mirrors
`BurnMessageV2::decode`. -/
def decode (bs : List UInt8) : Option BurnBody :=
  if minSize ≤ bs.length then
    some {
      version := natOfBe (slice bs 0 4)
      burnToken := slice bs 4 32
      mintRecipient := slice bs 36 32
      amount := natOfBe (slice bs 68 32)
      messageSender := slice bs 100 32
      maxFee := natOfBe (slice bs 132 32)
      feeExecuted := natOfBe (slice bs 164 32)
      expirationBlock := natOfBe (slice bs 196 32)
      hookData := bs.drop minSize
    }
  else none

/-- True when the body carries hook data. Mirrors `BurnMessageV2::has_hooks`. -/
def hasHooks (b : BurnBody) : Bool :=
  !b.hookData.isEmpty

/-- True when the body is configured for fast transfer (`max_fee > 0`).
Mirrors `BurnMessageV2::is_fast_transfer`. -/
def isFastTransfer (b : BurnBody) : Bool :=
  b.maxFee != 0

theorem length_encode (b : BurnBody) (wf : b.WellFormed) :
    b.encode.length = minSize + b.hookData.length := by
  obtain ⟨-, hbt, hmr, -, hms, -, -, -⟩ := wf
  simp [encode, minSize, hbt, hmr, hms]
  omega

/-- Round-trip: every well-formed body decodes from its own encoding. -/
theorem decode_encode (b : BurnBody) (wf : b.WellFormed) :
    decode b.encode = some b := by
  obtain ⟨hv, hbt, hmr, ha, hms, hmf, hfe, hex⟩ := wf
  have hlen : b.encode.length = minSize + b.hookData.length :=
    length_encode b ⟨hv, hbt, hmr, ha, hms, hmf, hfe, hex⟩
  unfold decode
  rw [if_pos (by omega)]
  simp [encode, minSize, slice_append_right, slice_append_left,
    drop_append_of_le, hbt, hmr, hms,
    natOfBe_beBytes 4 b.version (by simpa using hv),
    natOfBe_beBytes 32 b.amount (by simpa using ha),
    natOfBe_beBytes 32 b.maxFee (by simpa using hmf),
    natOfBe_beBytes 32 b.feeExecuted (by simpa using hfe),
    natOfBe_beBytes 32 b.expirationBlock (by simpa using hex)]

/-- Canonicality: an accepted byte string is exactly the encoding of the
body it decodes to, and that body is well-formed. -/
theorem encode_of_decode {bs : List UInt8} {b : BurnBody}
    (hdec : decode bs = some b) : b.encode = bs ∧ b.WellFormed := by
  unfold decode at hdec
  split at hdec
  · rename_i hlen
    have hlen' : 228 ≤ bs.length := hlen
    cases hdec
    have hbe : ∀ start len, start + len ≤ bs.length →
        beBytes len (natOfBe (slice bs start len)) = slice bs start len := by
      intro start len hle
      have hb := beBytes_natOfBe (slice bs start len)
      rwa [length_slice bs start len hle] at hb
    have hlt : ∀ len start, start + len ≤ bs.length →
        natOfBe (slice bs start len) < 256 ^ len := by
      intro len start hle
      have hl := natOfBe_lt (slice bs start len)
      rwa [length_slice bs start len hle] at hl
    constructor
    · dsimp only [encode]
      rw [hbe 0 4 (by omega), hbe 68 32 (by omega), hbe 132 32 (by omega),
        hbe 164 32 (by omega), hbe 196 32 (by omega)]
      simp only [List.append_assoc]
      conv =>
        rhs
        rw [show bs = bs.drop 0 from rfl,
          drop_eq_slice_append bs 0 4 4 rfl,
          drop_eq_slice_append bs 4 32 36 rfl,
          drop_eq_slice_append bs 36 32 68 rfl,
          drop_eq_slice_append bs 68 32 100 rfl,
          drop_eq_slice_append bs 100 32 132 rfl,
          drop_eq_slice_append bs 132 32 164 rfl,
          drop_eq_slice_append bs 164 32 196 rfl,
          drop_eq_slice_append bs 196 32 228 rfl]
      rfl
    · refine ⟨?_, length_slice bs 4 32 (by omega),
        length_slice bs 36 32 (by omega), ?_,
        length_slice bs 100 32 (by omega), ?_, ?_, ?_⟩
      · have := hlt 4 0 (by omega)
        simpa using this
      · have := hlt 32 68 (by omega)
        simpa using this
      · have := hlt 32 132 (by omega)
        simpa using this
      · have := hlt 32 164 (by omega)
        simpa using this
      · have := hlt 32 196 (by omega)
        simpa using this
  · cases hdec

end BurnBody

/-- A full CCTP v2 burn-transfer message: header plus body. Mirrors
`ParsedV2Message` in `src/protocol/message.rs`. -/
structure Message where
  header : MessageHeader
  body : BurnBody
deriving DecidableEq, Repr

namespace Message

/-- Minimum total message size: 148-byte header + 228-byte body. -/
def minSize : Nat := MessageHeader.size + BurnBody.minSize

/-- Domain-aware validity for a body word. EVM domains require the 12-byte
zero-padding convention; non-EVM domains preserve the raw 32-byte word. -/
def addressWordValidForDomain (d : DomainId) (w : List UInt8) : Bool :=
  !d.isEvm || w.take 12 == addressPadding

/-- The three body words whose EVM-ness is determined by the header domains:
`burnToken` and `messageSender` are source-domain words, while
`mintRecipient` is a destination-domain word. -/
def bodyWordsValid (h : MessageHeader) (b : BurnBody) : Bool :=
  addressWordValidForDomain h.sourceDomain b.burnToken &&
  addressWordValidForDomain h.destinationDomain b.mintRecipient &&
  addressWordValidForDomain h.sourceDomain b.messageSender

def WellFormed (m : Message) : Prop :=
  m.header.WellFormed ∧ m.body.WellFormed ∧ bodyWordsValid m.header m.body = true

/-- Encodes the full message. Mirrors `ParsedV2Message::encode`. -/
def encode (m : Message) : List UInt8 :=
  m.header.encode ++ m.body.encode

/-- Decodes a canonical CCTP v2 burn-transfer message: a valid header on the
first 148 bytes, a structurally valid burn body on the rest, and EVM padding
only where the header's source/destination domains require it. Mirrors
`ParsedV2Message::decode` / `ParsedV2Message::parse`. -/
def decode (bs : List UInt8) : Option Message :=
  match MessageHeader.decode (bs.take MessageHeader.size),
        BurnBody.decode (bs.drop MessageHeader.size) with
  | some h, some b => if bodyWordsValid h b then some ⟨h, b⟩ else none
  | _, _ => none

/-- Round-trip: every well-formed message decodes from its own encoding. -/
theorem decode_encode (m : Message) (wf : m.WellFormed) :
    decode m.encode = some m := by
  obtain ⟨hh, hb, hw⟩ := wf
  have hlen := MessageHeader.length_encode m.header hh
  simp only [decode, encode]
  rw [List.take_left' hlen, List.drop_left' hlen,
    MessageHeader.decode_encode m.header hh, BurnBody.decode_encode m.body hb]
  simp [hw]

/-- Canonicality: an accepted byte string is exactly the encoding of the
message it decodes to. Distinct raw inputs never alias to one parsed value,
so the parsed message determines the on-chain `keccak256` message hash. -/
theorem encode_of_decode {bs : List UInt8} {m : Message}
    (hdec : decode bs = some m) : m.encode = bs ∧ m.WellFormed := by
  unfold decode at hdec
  split at hdec
  case h_1 h b hh hb =>
    split at hdec
    · rename_i hvalid
      cases hdec
      obtain ⟨hhenc, hhwf⟩ := MessageHeader.encode_of_decode hh
      obtain ⟨hbenc, hbwf⟩ := BurnBody.encode_of_decode hb
      refine ⟨?_, hhwf, hbwf, hvalid⟩
      rw [encode, hhenc, hbenc, List.take_append_drop]
    · cases hdec
  case h_2 => exact absurd hdec (by simp)

/-- Strictness corollary: the parser is injective on accepted inputs — two
different raw byte strings can never decode to the same message. -/
theorem decode_injective {bs₁ bs₂ : List UInt8} {m : Message}
    (h₁ : decode bs₁ = some m) (h₂ : decode bs₂ = some m) : bs₁ = bs₂ := by
  have e₁ := (encode_of_decode h₁).1
  have e₂ := (encode_of_decode h₂).1
  rw [← e₁, e₂]

end Message

end CctpSpec
