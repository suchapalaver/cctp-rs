-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec
import Lean.Data.Json

/-!
# Correspondence fixture generation

Builds the JSON test vectors consumed by the Rust correspondence test
(`tests/lean_model_correspondence.rs`). Every message vector is checked
against the Lean model during generation: accept vectors must round-trip
through `Message.decode`/`Message.encode`, reject vectors must decode to
`none`. A vector that disagrees with the model aborts generation, so the
committed fixture file can only ever contain the model's own verdicts.

Regenerate with `lake exe gen_vectors > ../tests/fixtures/lean/cctp_v2_vectors.json`
from the `verification/` directory.
-/

namespace CctpSpec.Fixtures

open Lean (Json ToJson toJson)

/-! ## Hex helpers -/

private def hexDigit (n : Nat) : Char :=
  if n < 10 then Char.ofNat ('0'.toNat + n) else Char.ofNat ('a'.toNat + n - 10)

def hexOfBytes (bs : List UInt8) : String :=
  bs.foldl (fun s b => s.push (hexDigit (b.toNat / 16)) |>.push (hexDigit (b.toNat % 16))) "0x"

private def hexVal (c : Char) : Nat :=
  if '0' ≤ c ∧ c ≤ '9' then c.toNat - '0'.toNat
  else if 'a' ≤ c ∧ c ≤ 'f' then c.toNat - 'a'.toNat + 10
  else if 'A' ≤ c ∧ c ≤ 'F' then c.toNat - 'A'.toNat + 10
  else 0

private def ofHexChars : List Char → List UInt8
  | hi :: lo :: rest => UInt8.ofNat (hexVal hi * 16 + hexVal lo) :: ofHexChars rest
  | _ => []

/-- Parses a (known-good) hex literal, with or without a `0x` prefix. -/
def ofHex (s : String) : List UInt8 :=
  let cs := s.toList
  ofHexChars (if s.startsWith "0x" then cs.drop 2 else cs)

/-! ## Byte-string construction helpers -/

/-- A canonical EVM `bytes32` word: 12 zero bytes then the 20-byte address. -/
def evmWord (addr20 : List UInt8) : List UInt8 :=
  List.replicate 12 0 ++ addr20

def zeroWord : List UInt8 := List.replicate 32 0

def zeroAddr : List UInt8 := List.replicate 20 0

def repeatByte (b : UInt8) (n : Nat) : List UInt8 := List.replicate n b

/-- Overwrites `patch` into `bs` starting at byte `start`. -/
def setSlice (bs patch : List UInt8) (start : Nat) : List UInt8 :=
  bs.take start ++ patch ++ bs.drop (start + patch.length)

/-! ## Shared addresses (mirrors the Rust unit-test fixtures) -/

def usdcAddr : List UInt8 := ofHex "75faf114eafb1bdbe2f0316df893fd58ce46aa4d"
def usdcAddr2 : List UInt8 := ofHex "a2d2a41577ce14e20a6c2de999a8ec2bd9fe34af"
def addrA : List UInt8 := ofHex "8fe6b999dc680ccfdd5bf7eb0974218be2542daa"
def addrB : List UInt8 := ofHex "7f7d081724f0240c64c9e01cde4626602f9a0192"
def addrC : List UInt8 := ofHex "1234567890abcdef1234567890abcdef12345678"

/-- A real canonical CCTP v2 message (Arbitrum → Base, 1 USDC) captured from
Circle Iris; also used in the Rust unit tests in `src/protocol/message.rs`. -/
def realCircleRaw : List UInt8 := ofHex
  "0000000100000003000000062f3cb13cf4a6103f9e3b256495b08c4e05630fcba639565d199ed420a5f2be010000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000008fe6b999dc680ccfdd5bf7eb0974218be2542daa0000000000000000000000000000000000000000000000000000000000000000000007d0000007d00000000100000000000000000000000075faf114eafb1bdbe2f0316df893fd58ce46aa4d0000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a019200000000000000000000000000000000000000000000000000000000000f42400000000000000000000000007f7d081724f0240c64c9e01cde4626602f9a0192000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"

/-! ## Message vectors -/

structure AcceptVector where
  name : String
  comment : String
  message : Message

structure RejectVector where
  name : String
  comment : String
  rejectKind : String
  bytes : List UInt8

private def mkHeader (version : Nat) (src dst : DomainId)
    (nonce sender recipient caller : List UInt8) (minFin finExec : Nat) :
    MessageHeader :=
  { version, sourceDomain := src, destinationDomain := dst, nonce, sender,
    recipient, destinationCaller := caller, minFinalityThreshold := minFin,
    finalityThresholdExecuted := finExec }

private def mkBody (version : Nat) (burnToken mintRecipient : List UInt8)
    (amount : Nat) (messageSender : List UInt8)
    (maxFee feeExecuted expirationBlock : Nat) (hookData : List UInt8) :
    BurnBody :=
  { version, burnToken, mintRecipient, amount, messageSender, maxFee,
    feeExecuted, expirationBlock, hookData }

def fastWithHookMessage : Message :=
  { header := mkHeader 1 .ethereum .linea (repeatByte 0x11 32) (evmWord addrA)
      (evmWord addrB) (evmWord addrC) 1000 1000
    body := mkBody 1 usdcAddr2 addrB 2500000 addrA 150 25 12345678
      (ofHex "deadbeefcafe") }

def solanaSourceMessage : Message :=
  { header := mkHeader 1 .solana .base zeroWord (repeatByte 0xab 32)
      (evmWord addrB) zeroWord 2000 2000
    body := mkBody 1 usdcAddr2 addrB 1000000 addrC 0 0 0 [] }

def starknetDestinationMessage : Message :=
  { header := mkHeader 1 .ethereum .starknetTestnet (repeatByte 0x22 32)
      (evmWord addrA) (repeatByte 0x42 32) (repeatByte 0x77 32) 1000 2000
    body := mkBody 1 usdcAddr2 addrB 1 addrA 0 0 0 [] }

def maxValuesMessage : Message :=
  { header := mkHeader 7 .base .avalanche (repeatByte 0xff 32) (evmWord addrC)
      (evmWord addrA) zeroWord 0 1500
    body := mkBody 9 usdcAddr2 addrA (2 ^ 256 - 1) addrC (2 ^ 256 - 1)
      (2 ^ 256 - 1) (2 ^ 256 - 1) [0x00] }

def minimalMessage : Message :=
  { header := mkHeader 0 .arbitrum .unichain zeroWord zeroWord zeroWord
      zeroWord 2000 2000
    body := mkBody 0 zeroAddr zeroAddr 0 zeroAddr 0 0 0 [] }

def acceptVectors : Except String (List AcceptVector) := do
  let realCircle ←
    match Message.decode realCircleRaw with
    | some m =>
      if m.encode == realCircleRaw then
        pure (AcceptVector.mk "real_circle_arbitrum_to_base"
          "Captured from Circle Iris: 1 USDC, Arbitrum to Base, standard finality, permissionless."
          m)
      else
        throw "real Circle message did not re-encode to its raw bytes"
    | none => throw "real Circle message failed to decode in the Lean model"
  pure [
    realCircle,
    ⟨"fast_with_hook_ethereum_to_linea",
      "Fast finality requested and executed, destination caller set, fee fields populated, hook data present.",
      fastWithHookMessage⟩,
    ⟨"solana_source_placeholder_nonce",
      "Non-EVM source domain: raw 32-byte sender word without EVM padding, all-zero placeholder nonce, permissionless.",
      solanaSourceMessage⟩,
    ⟨"starknet_destination_caller_set",
      "Non-EVM destination domain: raw recipient and caller words, fast requested but standard executed.",
      starknetDestinationMessage⟩,
    ⟨"max_values_unvalidated_fields",
      "Maximum uint256 amounts and fees, header/body versions the parser carries without validating, finality values matching no known mode.",
      maxValuesMessage⟩,
    ⟨"minimal_all_zero",
      "Exactly 376 bytes: zero addresses, zero amount, no hook data.",
      minimalMessage⟩
  ]

def rejectVectors : List RejectVector :=
  let minimal := minimalMessage.encode
  let fwh := fastWithHookMessage.encode
  [
    ⟨"empty", "Zero-length input.", "too_short_header", []⟩,
    ⟨"header_minus_one_byte", "147 bytes: one short of a full header.",
      "too_short_header", List.replicate 147 0⟩,
    ⟨"header_only", "A valid 148-byte header with no body.",
      "too_short_body", minimalMessage.header.encode⟩,
    ⟨"message_minus_one_byte", "375 bytes: one short of the minimum message.",
      "too_short_body", minimal.take 375⟩,
    ⟨"unknown_source_domain_4", "Domain 4 has never been announced by Circle.",
      "unknown_source_domain", setSlice minimal (beBytes 4 4) 4⟩,
    ⟨"unknown_source_domain_8", "Domain 8 is a gap in Circle's table.",
      "unknown_source_domain", setSlice minimal (beBytes 4 8) 4⟩,
    ⟨"unknown_destination_domain_999", "Domain 999 is far beyond the announced table.",
      "unknown_destination_domain", setSlice minimal (beBytes 4 999) 8⟩,
    ⟨"unknown_destination_domain_20", "Domain 20 is a gap in Circle's table.",
      "unknown_destination_domain", setSlice minimal (beBytes 4 20) 8⟩,
    ⟨"non_canonical_burn_token_first_pad_byte",
      "First padding byte of the burn_token word set to 0xff.",
      "non_canonical_burn_token", setSlice fwh [0xff] (148 + 4)⟩,
    ⟨"non_canonical_burn_token_last_pad_byte",
      "Last (12th) padding byte of the burn_token word set to 0x01.",
      "non_canonical_burn_token", setSlice fwh [0x01] (148 + 4 + 11)⟩,
    ⟨"non_canonical_mint_recipient_pad_byte",
      "First padding byte of the mint_recipient word set to 0xff.",
      "non_canonical_mint_recipient", setSlice fwh [0xff] (148 + 36)⟩,
    ⟨"non_canonical_message_sender_pad_byte",
      "First padding byte of the message_sender word set to 0xff.",
      "non_canonical_message_sender", setSlice fwh [0xff] (148 + 100)⟩
  ]

/-! ## Constant-conversion vectors -/

/-- Two past the largest announced domain ID, derived from the model's own
table (completeness enforced by `DomainId.mem_all`) so the sweep cannot go
stale when a domain is added. -/
def domainSweepBound : Nat :=
  (DomainId.all.map DomainId.toU32).foldl Nat.max 0 + 2

def domainSweep : List Nat :=
  List.range domainSweepBound ++ [100, 999, 4294967295]

def finalitySweep : List Nat :=
  [0, 500, 999, 1000, 1001, 1500, 2000, 2001, 3000, 4294967295]

def transferModes : List TransferMode := [
  .standard,
  .fast 150,
  .standardWithHook (ofHex "deadbeef"),
  .fastWithHook 98765432109876543210 [0x00]
]

/-! ## JSON encoding -/

private def str (s : String) : Json := Json.str s

private def optFinalityJson : Option FinalityThreshold → Json
  | some t => str t.jsonName
  | none => Json.null

def domainVectorJson (n : Nat) : Json :=
  match DomainId.fromU32 n with
  | some d => Json.mkObj [("u32", toJson n), ("valid", toJson true),
      ("name", str d.jsonName), ("is_evm", toJson d.isEvm)]
  | none => Json.mkObj [("u32", toJson n), ("valid", toJson false)]

def finalityVectorJson (n : Nat) : Json :=
  match FinalityThreshold.fromU32 n with
  | some t => Json.mkObj [("u32", toJson n), ("valid", toJson true),
      ("name", str t.jsonName)]
  | none => Json.mkObj [("u32", toJson n), ("valid", toJson false)]

def transferModeJson (m : TransferMode) : Json :=
  Json.mkObj [
    ("mode", str m.jsonName),
    ("min_finality_threshold", toJson m.finalityThreshold.toU32),
    ("is_fast", toJson m.isFast),
    ("max_fee", str (toString m.maxFee)),
    ("hook_data", match m.hookData? with
      | some h => str (hexOfBytes h)
      | none => Json.null)
  ]

def headerJson (h : MessageHeader) : Json :=
  Json.mkObj [
    ("version", toJson h.version),
    ("source_domain", toJson h.sourceDomain.toU32),
    ("source_domain_name", str h.sourceDomain.jsonName),
    ("destination_domain", toJson h.destinationDomain.toU32),
    ("destination_domain_name", str h.destinationDomain.jsonName),
    ("nonce", str (hexOfBytes h.nonce)),
    ("sender", str (hexOfBytes h.sender)),
    ("recipient", str (hexOfBytes h.recipient)),
    ("destination_caller", str (hexOfBytes h.destinationCaller)),
    ("min_finality_threshold", toJson h.minFinalityThreshold),
    ("finality_threshold_executed", toJson h.finalityThresholdExecuted)
  ]

def bodyJson (b : BurnBody) : Json :=
  Json.mkObj [
    ("version", toJson b.version),
    ("burn_token", str (hexOfBytes b.burnToken)),
    ("mint_recipient", str (hexOfBytes b.mintRecipient)),
    ("amount", str (toString b.amount)),
    ("message_sender", str (hexOfBytes b.messageSender)),
    ("max_fee", str (toString b.maxFee)),
    ("fee_executed", str (toString b.feeExecuted)),
    ("expiration_block", str (toString b.expirationBlock)),
    ("hook_data", str (hexOfBytes b.hookData))
  ]

def derivedJson (m : Message) : Json :=
  Json.mkObj [
    ("message_len_bytes", toJson m.encode.length),
    ("has_placeholder_nonce", toJson m.header.hasPlaceholderNonce),
    ("is_permissionless", toJson m.header.isPermissionless),
    ("requested_finality", optFinalityJson m.header.requestedFinality),
    ("attested_finality", optFinalityJson m.header.attestedFinality),
    ("is_fast_transfer", toJson m.body.isFastTransfer),
    ("has_hooks", toJson m.body.hasHooks)
  ]

def acceptVectorJson (v : AcceptVector) : Json :=
  Json.mkObj [
    ("name", str v.name),
    ("comment", str v.comment),
    ("raw", str (hexOfBytes v.message.encode)),
    ("header", headerJson v.message.header),
    ("body", bodyJson v.message.body),
    ("derived", derivedJson v.message)
  ]

def rejectVectorJson (v : RejectVector) : Json :=
  Json.mkObj [
    ("name", str v.name),
    ("comment", str v.comment),
    ("raw", str (hexOfBytes v.bytes)),
    ("reject_kind", str v.rejectKind)
  ]

/-- Builds the full fixture document, self-checking every message vector
against the Lean model's verdict first. -/
def fixturesJson : Except String Json := do
  let accepts ← acceptVectors
  for v in accepts do
    unless Message.decode v.message.encode == some v.message do
      throw s!"accept vector '{v.name}' failed the model round-trip check"
  for v in rejectVectors do
    unless Message.decode v.bytes == none do
      throw s!"reject vector '{v.name}' unexpectedly decodes in the model"
  pure <| Json.mkObj [
    ("_meta", Json.mkObj [
      ("description", str "Lean-model-generated CCTP v2 correspondence vectors. DO NOT EDIT BY HAND."),
      ("generator", str "verification/: lake exe gen_vectors > ../tests/fixtures/lean/cctp_v2_vectors.json")
    ]),
    ("domain_vectors", Json.arr (domainSweep.map domainVectorJson).toArray),
    ("finality_vectors", Json.arr (finalitySweep.map finalityVectorJson).toArray),
    ("transfer_mode_vectors", Json.arr (transferModes.map transferModeJson).toArray),
    ("accept_message_vectors", Json.arr (accepts.map acceptVectorJson).toArray),
    ("reject_message_vectors", Json.arr (rejectVectors.map rejectVectorJson).toArray)
  ]

end CctpSpec.Fixtures
