-- SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
--
-- SPDX-License-Identifier: Apache-2.0

import CctpSpec.Domain

/-!
# CCTP v2 bridge route validity

Models the bridge SDK chain classification implemented by `CctpV2` for
`alloy_chains::NamedChain` and the acceptance rules implemented by
`CctpV2Route::new`. The model pins this crate's interpretation of Alloy chain
values; Alloy itself, including `NamedChain::is_testnet`, remains in the trusted
computing base and is checked by generated Rust correspondence tests.
-/

namespace CctpSpec

/-- Chain values needed to exhaustively model the SDK's current CCTP v2 bridge
boundary, plus representative unsupported values. -/
inductive Chain where
  | ethereumMainnet
  | ethereumSepolia
  | arbitrumMainnet
  | arbitrumSepolia
  | baseMainnet
  | baseSepolia
  | optimismMainnet
  | optimismSepolia
  | avalancheMainnet
  | avalancheFuji
  | polygonMainnet
  | polygonAmoy
  | unichainMainnet
  | unichainSepolia
  | lineaMainnet
  | lineaSepolia
  | sonicMainnet
  | sonicTestnet
  | seiMainnet
  | seiTestnet
  | hyperEvmMainnet
  | hyperEvmTestnet
  | worldMainnet
  | worldSepolia
  | monadMainnet
  | monadTestnet
  | xdcMainnet
  | xdcTestnet
  | inkMainnet
  | inkSepolia
  | plumeMainnet
  | plumeTestnet
  | arcMainnet
  | arcTestnet
  | injectiveMainnet
  | injectiveTestnet
  | pharosMainnet
  | pharosTestnet
  | cronosMainnet
  | cronosTestnet
  | plasmaMainnet
  | plasmaTestnet
  | bnbSmartChainMainnet
  | moonbeamMainnet
deriving DecidableEq, Repr

namespace Chain

/-- Stable fixture name mapped explicitly to `NamedChain` on the Rust side. -/
def jsonName : Chain → String
  | .ethereumMainnet => "ethereum_mainnet"
  | .ethereumSepolia => "ethereum_sepolia"
  | .arbitrumMainnet => "arbitrum_mainnet"
  | .arbitrumSepolia => "arbitrum_sepolia"
  | .baseMainnet => "base_mainnet"
  | .baseSepolia => "base_sepolia"
  | .optimismMainnet => "optimism_mainnet"
  | .optimismSepolia => "optimism_sepolia"
  | .avalancheMainnet => "avalanche_mainnet"
  | .avalancheFuji => "avalanche_fuji"
  | .polygonMainnet => "polygon_mainnet"
  | .polygonAmoy => "polygon_amoy"
  | .unichainMainnet => "unichain_mainnet"
  | .unichainSepolia => "unichain_sepolia"
  | .lineaMainnet => "linea_mainnet"
  | .lineaSepolia => "linea_sepolia"
  | .sonicMainnet => "sonic_mainnet"
  | .sonicTestnet => "sonic_testnet"
  | .seiMainnet => "sei_mainnet"
  | .seiTestnet => "sei_testnet"
  | .hyperEvmMainnet => "hyper_evm_mainnet"
  | .hyperEvmTestnet => "hyper_evm_testnet"
  | .worldMainnet => "world_mainnet"
  | .worldSepolia => "world_sepolia"
  | .monadMainnet => "monad_mainnet"
  | .monadTestnet => "monad_testnet"
  | .xdcMainnet => "xdc_mainnet"
  | .xdcTestnet => "xdc_testnet"
  | .inkMainnet => "ink_mainnet"
  | .inkSepolia => "ink_sepolia"
  | .plumeMainnet => "plume_mainnet"
  | .plumeTestnet => "plume_testnet"
  | .arcMainnet => "arc_mainnet"
  | .arcTestnet => "arc_testnet"
  | .injectiveMainnet => "injective_mainnet"
  | .injectiveTestnet => "injective_testnet"
  | .pharosMainnet => "pharos_mainnet"
  | .pharosTestnet => "pharos_testnet"
  | .cronosMainnet => "cronos_mainnet"
  | .cronosTestnet => "cronos_testnet"
  | .plasmaMainnet => "plasma_mainnet"
  | .plasmaTestnet => "plasma_testnet"
  | .bnbSmartChainMainnet => "bnb_smart_chain_mainnet"
  | .moonbeamMainnet => "moonbeam_mainnet"

/-- Every modeled chain. Completeness is proved by `mem_all`. -/
def all : List Chain :=
  [.ethereumMainnet, .ethereumSepolia, .arbitrumMainnet, .arbitrumSepolia,
   .baseMainnet, .baseSepolia, .optimismMainnet, .optimismSepolia,
   .avalancheMainnet, .avalancheFuji, .polygonMainnet, .polygonAmoy,
   .unichainMainnet, .unichainSepolia, .lineaMainnet, .lineaSepolia,
   .sonicMainnet, .sonicTestnet, .seiMainnet, .seiTestnet,
   .hyperEvmMainnet, .hyperEvmTestnet, .worldMainnet, .worldSepolia, .monadMainnet,
   .monadTestnet, .xdcMainnet, .xdcTestnet, .inkMainnet, .inkSepolia,
   .plumeMainnet, .plumeTestnet, .arcMainnet, .arcTestnet,
   .injectiveMainnet, .injectiveTestnet, .pharosMainnet, .pharosTestnet,
   .cronosMainnet, .cronosTestnet, .plasmaMainnet, .plasmaTestnet,
   .bnbSmartChainMainnet, .moonbeamMainnet]

theorem mem_all (chain : Chain) : chain ∈ all := by
  cases chain <;> decide

/-- Mirrors `NamedChain::supports_cctp_v2()` for the modeled values. -/
def supportsCctpV2 : Chain → Bool
  | .bnbSmartChainMainnet | .moonbeamMainnet => false
  | _ => true

/-- Mirrors `NamedChain::is_testnet()` for the modeled values. -/
def isTestnet : Chain → Bool
  | .ethereumSepolia | .arbitrumSepolia | .baseSepolia | .optimismSepolia
  | .avalancheFuji | .polygonAmoy | .unichainSepolia | .lineaSepolia
  | .sonicTestnet | .seiTestnet | .hyperEvmTestnet | .worldSepolia | .monadTestnet
  | .xdcTestnet | .inkSepolia | .plumeTestnet | .arcTestnet
  | .injectiveTestnet | .pharosTestnet | .cronosTestnet | .plasmaTestnet => true
  | _ => false

/-- Mirrors `NamedChain::cctp_v2_domain_id()`. Unsupported bridge chains have
no route-domain mapping even if the protocol parser recognizes their domain. -/
def domainId? : Chain → Option DomainId
  | .ethereumMainnet | .ethereumSepolia => some .ethereum
  | .arbitrumMainnet | .arbitrumSepolia => some .arbitrum
  | .baseMainnet | .baseSepolia => some .base
  | .optimismMainnet | .optimismSepolia => some .optimism
  | .avalancheMainnet | .avalancheFuji => some .avalanche
  | .polygonMainnet | .polygonAmoy => some .polygon
  | .unichainMainnet | .unichainSepolia => some .unichain
  | .lineaMainnet | .lineaSepolia => some .linea
  | .sonicMainnet | .sonicTestnet => some .sonic
  | .seiMainnet | .seiTestnet => some .sei
  | .hyperEvmMainnet | .hyperEvmTestnet => some .hyperEvm
  | .worldMainnet | .worldSepolia => some .worldChain
  | .monadMainnet | .monadTestnet => some .monad
  | .xdcMainnet | .xdcTestnet => some .xdc
  | .inkMainnet | .inkSepolia => some .ink
  | .plumeMainnet | .plumeTestnet => some .plume
  | .arcMainnet | .arcTestnet => some .arcTestnet
  | .injectiveMainnet | .injectiveTestnet => some .injective
  | .pharosMainnet | .pharosTestnet => some .pharos
  | .cronosMainnet | .cronosTestnet => some .cronos
  | .plasmaMainnet | .plasmaTestnet => some .plasma
  | .bnbSmartChainMainnet | .moonbeamMainnet => none

/-- The support table and domain table have exactly the same domain. -/
theorem domainId_isSome_eq_supportsCctpV2 (chain : Chain) :
    chain.domainId?.isSome = chain.supportsCctpV2 := by
  cases chain <;> rfl

/-- Every supported bridge chain maps to a parser-modeled domain. -/
theorem domainId_exists_of_supported {chain : Chain}
    (h : chain.supportsCctpV2 = true) :
    ∃ domain, chain.domainId? = some domain := by
  cases chain <;> simp_all [supportsCctpV2, domainId?]

/-- Whether a parser-modeled domain is the image of a supported bridge chain. -/
def isBridgeDomain (domain : DomainId) : Bool :=
  all.any (fun chain => decide (chain.domainId? = some domain))

end Chain

/-- The route predicate mirrored by `CctpV2Route::new`. -/
def ValidRoute (source destination : Chain) : Prop :=
  source.supportsCctpV2 = true ∧
  destination.supportsCctpV2 = true ∧
  source ≠ destination ∧
  source.isTestnet = destination.isTestnet

instance (source destination : Chain) : Decidable (ValidRoute source destination) := by
  unfold ValidRoute
  infer_instance

/-- Executable route verdict used by fixture generation. -/
def isValidRoute (source destination : Chain) : Bool :=
  decide (ValidRoute source destination)

theorem isValidRoute_eq_true_iff {source destination : Chain} :
    isValidRoute source destination = true ↔ ValidRoute source destination := by
  simp [isValidRoute]

/-- Every valid route endpoint maps into the typed `DomainId` table. -/
theorem valid_route_domains {source destination : Chain}
    (h : isValidRoute source destination = true) :
    ∃ sourceDomain destinationDomain,
      source.domainId? = some sourceDomain ∧
      destination.domainId? = some destinationDomain := by
  have valid := isValidRoute_eq_true_iff.mp h
  obtain ⟨sourceDomain, sourceMapped⟩ :=
    Chain.domainId_exists_of_supported valid.1
  obtain ⟨destinationDomain, destinationMapped⟩ :=
    Chain.domainId_exists_of_supported valid.2.1
  exact ⟨sourceDomain, destinationDomain, sourceMapped, destinationMapped⟩

/-- Valid routes never have the same source and destination chain. -/
theorem valid_route_source_ne_destination {source destination : Chain}
    (h : isValidRoute source destination = true) : source ≠ destination :=
  (isValidRoute_eq_true_iff.mp h).2.2.1

/-- Valid routes stay within one network environment. -/
theorem valid_route_same_environment {source destination : Chain}
    (h : isValidRoute source destination = true) :
    source.isTestnet = destination.isTestnet :=
  (isValidRoute_eq_true_iff.mp h).2.2.2

/-- Route validity is symmetric in source and destination. -/
theorem valid_route_symmetric {source destination : Chain}
    (h : isValidRoute source destination = true) :
    isValidRoute destination source = true := by
  rw [isValidRoute_eq_true_iff]
  have valid := isValidRoute_eq_true_iff.mp h
  exact ⟨valid.2.1, valid.1, valid.2.2.1.symm, valid.2.2.2.symm⟩

end CctpSpec
