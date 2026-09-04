-- SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
-- SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
--
-- SPDX-License-Identifier: Apache-2.0

/-!
# CCTP domain identifiers

Models `DomainId` from `src/protocol/domain_id.rs`: the 30 current CCTP domain
IDs modeled by the parser, their `u32` wire values, and the
EVM-address-convention flag. The theorems pin the conversion table: `fromU32`
and `toU32` are mutually inverse on exactly the modeled IDs, so legacy-only or
gap values (4, 8, 20, 23, 24, ...) can never decode.
-/

namespace CctpSpec

/-- CCTP domain identifier. One constructor per parser-modeled domain ID;
mirrors `DomainId` in `src/protocol/domain_id.rs`. -/
inductive DomainId where
  | ethereum
  | avalanche
  | optimism
  | arbitrum
  | solana
  | base
  | polygon
  | aptos
  | unichain
  | linea
  | codex
  | sonic
  | worldChain
  | monad
  | sei
  | bnbSmartChain
  | xdc
  | hyperEvm
  | ink
  | plume
  | starknetTestnet
  | arcTestnet
  | stellar
  | edge
  | injective
  | morph
  | pharos
  | cronos
  | plasma
  | xLayer
deriving DecidableEq, Repr

namespace DomainId

/-- The `u32` wire value of a domain, as in Circle's protocol tables. -/
def toU32 : DomainId → Nat
  | .ethereum => 0
  | .avalanche => 1
  | .optimism => 2
  | .arbitrum => 3
  | .solana => 5
  | .base => 6
  | .polygon => 7
  | .aptos => 9
  | .unichain => 10
  | .linea => 11
  | .codex => 12
  | .sonic => 13
  | .worldChain => 14
  | .monad => 15
  | .sei => 16
  | .bnbSmartChain => 17
  | .xdc => 18
  | .hyperEvm => 19
  | .ink => 21
  | .plume => 22
  | .starknetTestnet => 25
  | .arcTestnet => 26
  | .stellar => 27
  | .edge => 28
  | .injective => 29
  | .morph => 30
  | .pharos => 31
  | .cronos => 32
  | .plasma => 33
  | .xLayer => 37

/-- Typed view of a `u32` wire value. Mirrors `DomainId::from_u32`. -/
def fromU32 : Nat → Option DomainId
  | 0 => some .ethereum
  | 1 => some .avalanche
  | 2 => some .optimism
  | 3 => some .arbitrum
  | 5 => some .solana
  | 6 => some .base
  | 7 => some .polygon
  | 9 => some .aptos
  | 10 => some .unichain
  | 11 => some .linea
  | 12 => some .codex
  | 13 => some .sonic
  | 14 => some .worldChain
  | 15 => some .monad
  | 16 => some .sei
  | 17 => some .bnbSmartChain
  | 18 => some .xdc
  | 19 => some .hyperEvm
  | 21 => some .ink
  | 22 => some .plume
  | 25 => some .starknetTestnet
  | 26 => some .arcTestnet
  | 27 => some .stellar
  | 28 => some .edge
  | 29 => some .injective
  | 30 => some .morph
  | 31 => some .pharos
  | 32 => some .cronos
  | 33 => some .plasma
  | 37 => some .xLayer
  | _ => none

/-- Whether the domain uses the SDK's EVM `bytes32` address conventions.
Mirrors `DomainId::is_evm`. -/
def isEvm : DomainId → Bool
  | .solana => false
  | .aptos => false
  | .starknetTestnet => false
  | .stellar => false
  | _ => true

/-- The `snake_case` serde name used by the Rust SDK's JSON serialization. -/
def jsonName : DomainId → String
  | .ethereum => "ethereum"
  | .avalanche => "avalanche"
  | .optimism => "optimism"
  | .arbitrum => "arbitrum"
  | .solana => "solana"
  | .base => "base"
  | .polygon => "polygon"
  | .aptos => "aptos"
  | .unichain => "unichain"
  | .linea => "linea"
  | .codex => "codex"
  | .sonic => "sonic"
  | .worldChain => "world_chain"
  | .monad => "monad"
  | .sei => "sei"
  | .bnbSmartChain => "bnb_smart_chain"
  | .xdc => "xdc"
  | .hyperEvm => "hyper_evm"
  | .ink => "ink"
  | .plume => "plume"
  | .starknetTestnet => "starknet"
  | .arcTestnet => "arc_testnet"
  | .stellar => "stellar"
  | .edge => "edge"
  | .injective => "injective"
  | .morph => "morph"
  | .pharos => "pharos"
  | .cronos => "cronos"
  | .plasma => "plasma"
  | .xLayer => "x_layer"

/-- Every domain constructor, for exhaustive sweeps in fixture generation.
`mem_all` proves completeness, so adding a constructor without extending
this list fails the build. -/
def all : List DomainId :=
  [.ethereum, .avalanche, .optimism, .arbitrum, .solana, .base, .polygon,
   .aptos, .unichain, .linea, .codex, .sonic, .worldChain, .monad, .sei,
   .bnbSmartChain, .xdc, .hyperEvm, .ink, .plume, .starknetTestnet,
   .arcTestnet, .stellar, .edge, .injective, .morph, .pharos, .cronos,
   .plasma, .xLayer]

theorem mem_all (d : DomainId) : d ∈ all := by
  cases d <;> decide

/-- Round-trip: every domain's wire value decodes back to the same domain. -/
theorem fromU32_toU32 (d : DomainId) : fromU32 (toU32 d) = some d := by
  cases d <;> rfl

/-- Canonicality: an accepted wire value is exactly the encoding of the
domain it decodes to. Together with `fromU32_toU32` this makes the
conversion a bijection between the 30 modeled IDs and the typed values. -/
theorem toU32_of_fromU32 {n : Nat} {d : DomainId} (h : fromU32 n = some d) :
    toU32 d = n := by
  unfold fromU32 at h
  split at h <;> cases h <;> rfl

/-- `toU32` is injective (no two domains share a wire value). -/
theorem toU32_injective {d₁ d₂ : DomainId} (h : toU32 d₁ = toU32 d₂) :
    d₁ = d₂ := by
  have h₁ := fromU32_toU32 d₁
  rw [h, fromU32_toU32 d₂] at h₁
  exact (Option.some.inj h₁).symm

/-- Every domain wire value fits a `u32`. -/
theorem toU32_lt (d : DomainId) : toU32 d < 2 ^ 32 := by
  cases d <;> decide

/-- Exactly Solana, Aptos, Starknet, and Stellar are non-EVM. -/
theorem isEvm_eq_false_iff (d : DomainId) :
    d.isEvm = false ↔ d = .solana ∨ d = .aptos ∨
      d = .starknetTestnet ∨ d = .stellar := by
  cases d <;> simp [isEvm]

end DomainId

end CctpSpec
