// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0
//! Contract addresses for CCTP contracts across all supported chains
//!
//! This module centralizes all contract address constants for both `MessageTransmitter`
//! and `TokenMessenger` contracts across mainnet and testnet chains.

use alloy_primitives::{address, Address};

// Native stablecoin token addresses
//
// Reference: <https://developers.circle.com/stablecoins/usdc-contract-addresses>

/// Ethereum mainnet native USDC token address.
pub const ETHEREUM_USDC_ADDRESS: Address = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

/// Ethereum Sepolia native USDC token address.
pub const ETHEREUM_SEPOLIA_USDC_ADDRESS: Address =
    address!("1c7D4B196Cb0C7B01d743Fbc6116a902379C7238");

/// Arbitrum mainnet native USDC token address.
pub const ARBITRUM_USDC_ADDRESS: Address = address!("af88d065e77c8cC2239327C5EDb3A432268e5831");

/// Arbitrum Sepolia native USDC token address.
pub const ARBITRUM_SEPOLIA_USDC_ADDRESS: Address =
    address!("75faf114eafb1BDbe2F0316DF893fd58CE46AA4d");

/// Avalanche C-Chain native USDC token address.
pub const AVALANCHE_USDC_ADDRESS: Address = address!("B97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E");

/// Avalanche Fuji native USDC token address.
pub const AVALANCHE_FUJI_USDC_ADDRESS: Address =
    address!("5425890298aed601595a70AB815c96711a31Bc65");

/// Base mainnet native USDC token address.
pub const BASE_USDC_ADDRESS: Address = address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");

/// Base Sepolia native USDC token address.
pub const BASE_SEPOLIA_USDC_ADDRESS: Address = address!("036CbD53842c5426634e7929541eC2318f3dCF7e");

/// Optimism mainnet native USDC token address.
pub const OPTIMISM_USDC_ADDRESS: Address = address!("0b2C639c533813f4Aa9D7837CAf62653d097Ff85");

/// Optimism Sepolia native USDC token address.
pub const OPTIMISM_SEPOLIA_USDC_ADDRESS: Address =
    address!("5fd84259d66Cd46123540766Be93DFE6D43130D7");

/// Polygon PoS native USDC token address.
pub const POLYGON_USDC_ADDRESS: Address = address!("3c499c542cEF5E3811e1192ce70d8cC03d5c3359");

/// Polygon Amoy native USDC token address.
pub const POLYGON_AMOY_USDC_ADDRESS: Address = address!("41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582");

/// Unichain mainnet native USDC token address.
pub const UNICHAIN_USDC_ADDRESS: Address = address!("078D782b760474a361dDA0AF3839290b0EF57AD6");

/// Unichain Sepolia native test USDC token address.
pub const UNICHAIN_SEPOLIA_USDC_ADDRESS: Address =
    address!("31d0220469e10c4E71834a79b1f276d740d3768F");

/// Linea mainnet native USDC token address.
pub const LINEA_USDC_ADDRESS: Address = address!("176211869cA2b568f2A7D4EE941E073a821EE1ff");

/// Linea Sepolia native test USDC token address.
pub const LINEA_SEPOLIA_USDC_ADDRESS: Address =
    address!("FEce4462D57bD51A6A552365A011b95f0E16d9B7");

/// Sonic mainnet native USDC token address.
pub const SONIC_USDC_ADDRESS: Address = address!("29219dd400f2Bf60E5a23d13Be72B486D4038894");

/// Sonic testnet native test USDC token address.
pub const SONIC_TESTNET_USDC_ADDRESS: Address =
    address!("0BA304580ee7c9a980CF72e55f5Ed2E9fd30Bc51");

/// Sei mainnet native USDC token address.
pub const SEI_USDC_ADDRESS: Address = address!("e15fC38F6D8c56aF07bbCBe3BAf5708A2Bf42392");

/// Sei testnet native test USDC token address.
pub const SEI_TESTNET_USDC_ADDRESS: Address = address!("4fCF1784B31630811181f670Aea7A7bEF803eaED");

/// HyperEVM mainnet native USDC token address.
pub const HYPEREVM_USDC_ADDRESS: Address = address!("b88339CB7199b77E23DB6E890353E22632Ba630f");

/// HyperEVM testnet native test USDC token address.
pub const HYPEREVM_TESTNET_USDC_ADDRESS: Address =
    address!("2B3370eE501B4a559b57D449569354196457D8Ab");

/// World Chain mainnet native USDC token address.
pub const WORLD_CHAIN_USDC_ADDRESS: Address = address!("79A02482A880bCe3F13E09da970dC34dB4cD24D1");

/// World Chain Sepolia native USDC token address.
pub const WORLD_CHAIN_SEPOLIA_USDC_ADDRESS: Address =
    address!("66145f38cBAC35Ca6F1Dfb4914dF98F1614aeA88");

/// Monad mainnet native USDC token address.
pub const MONAD_USDC_ADDRESS: Address = address!("754704Bc059F8C67012fEd69BC8A327a5aafb603");

/// Monad testnet native USDC token address.
pub const MONAD_TESTNET_USDC_ADDRESS: Address =
    address!("534b2f3A21130d7a60830c2Df862319e593943A3");

/// XDC mainnet native USDC token address.
pub const XDC_USDC_ADDRESS: Address = address!("fA2958CB79b0491CC627c1557F441eF849Ca8eb1");

/// XDC Apothem native test USDC token address.
pub const XDC_TESTNET_USDC_ADDRESS: Address = address!("b5AB69F7bBada22B28e79C8FFAECe55eF1c771D4");

/// Ink mainnet native USDC token address.
pub const INK_USDC_ADDRESS: Address = address!("2D270e6886d130D724215A266106e6832161EAEd");

/// Ink Sepolia native test USDC token address.
pub const INK_SEPOLIA_USDC_ADDRESS: Address = address!("Fabab97dCE620294D2B0b0e46C68964e326300Ac");

/// Plume mainnet native USDC token address.
pub const PLUME_USDC_ADDRESS: Address = address!("222365EF19F7947e5484218551B56bb3965Aa7aF");

/// Plume testnet native test USDC token address.
pub const PLUME_TESTNET_USDC_ADDRESS: Address =
    address!("cB5f30e335672893c7eb944B374c196392C19D18");

/// Arc mainnet native USDC token address.
pub const ARC_USDC_ADDRESS: Address = address!("3600000000000000000000000000000000000000");

/// Arc testnet native test USDC token address.
pub const ARC_TESTNET_USDC_ADDRESS: Address = address!("3600000000000000000000000000000000000000");

/// Injective EVM mainnet native USDC token address.
pub const INJECTIVE_USDC_ADDRESS: Address = address!("a00C59fF5a080D2b954d0c75e46E22a0c371235a");

/// Injective EVM testnet native test USDC token address.
pub const INJECTIVE_TESTNET_USDC_ADDRESS: Address =
    address!("0C382e685bbeeFE5d3d9C29e29E341fEE8E84C5d");

/// Pharos mainnet native USDC token address.
pub const PHAROS_USDC_ADDRESS: Address = address!("C879C018dB60520F4355C26eD1a6D572cdAC1815");

/// Pharos testnet native test USDC token address.
pub const PHAROS_TESTNET_USDC_ADDRESS: Address =
    address!("cfC8330f4BCAB529c625D12781b1C19466A9Fc8B");

/// Cronos mainnet native USDC token address.
pub const CRONOS_USDC_ADDRESS: Address = address!("3D7F2C478aAfdB65542BCB44bCeeC05849999d2D");

/// Cronos testnet native test USDC token address.
pub const CRONOS_TESTNET_USDC_ADDRESS: Address =
    address!("Eb33dc5fac03833e132593659e1dE7256aB59794");

/// Plasma mainnet native USDC token address.
pub const PLASMA_USDC_ADDRESS: Address = address!("2d661C89D812261039AF9764eceaAee884f5F67F");

/// Plasma testnet native test USDC token address.
pub const PLASMA_TESTNET_USDC_ADDRESS: Address =
    address!("E67Fb267022cBA8064Dd388CC2FED724F3120D9D");

// Native EURC token addresses for Circle's initial CCTP EURC rollout.
//
// Reference: <https://developers.circle.com/stablecoins/eurc-contract-addresses>

/// Ethereum mainnet native EURC token address.
pub const ETHEREUM_EURC_ADDRESS: Address = address!("1aBaEA1f7C830bD89Acc67eC4af516284b1bC33c");

/// Ethereum Sepolia native EURC token address.
pub const ETHEREUM_SEPOLIA_EURC_ADDRESS: Address =
    address!("08210F9170F89Ab7658F0B5E3fF39b0E03C594D4");

/// Base mainnet native EURC token address.
pub const BASE_EURC_ADDRESS: Address = address!("60a3E35Cc302bFA44Cb288Bc5a4F316Fdb1adb42");

/// Base Sepolia native EURC token address.
pub const BASE_SEPOLIA_EURC_ADDRESS: Address = address!("808456652fdb597867f38412077A9182bf77359F");

// MessageTransmitter Addresses

/// <https://arbiscan.io/address/0xC30362313FBBA5cf9163F0bb16a0e01f01a896ca>
pub const ARBITRUM_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("c30362313fbba5cf9163f0bb16a0e01f01a896ca");

/// <https://sepolia.arbiscan.io/address/0xacf1ceef35caac005e15888ddb8a3515c41b4872>
pub const ARBITRUM_SEPOLIA_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("acf1ceef35caac005e15888ddb8a3515c41b4872");

/// <https://snowtrace.io/address/0x8186359af5f57fbb40c6b14a588d2a59c0c29880>
pub const AVALANCHE_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("8186359af5f57fbb40c6b14a588d2a59c0c29880");

/// <https://basescan.org/address/0xAD09780d193884d503182aD4588450C416D6F9D4>
pub const BASE_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("ad09780d193884d503182ad4588450c416d6f9d4");

/// <https://base-sepolia.blockscout.com/address/0x7865fAfC2db2093669d92c0F33AeEF291086BEFD>
pub const BASE_SEPOLIA_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("7865fAfC2db2093669d92c0F33AeEF291086BEFD");

/// <https://etherscan.io/address/0x0a992d191DEeC32aFe36203Ad87D7d289a738F81>
pub const ETHEREUM_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("0a992d191DEeC32aFe36203Ad87D7d289a738F81");

/// <https://sepolia.etherscan.io/address/0x7865fAfC2db2093669d92c0F33AeEF291086BEFD>
pub const ETHEREUM_SEPOLIA_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("7865fAfC2db2093669d92c0F33AeEF291086BEFD");

/// <https://optimistic.etherscan.io/address/0x4D41f22c5a0e5c74090899E5a8Fb597a8842b3e8>
pub const OPTIMISM_MESSAGE_TRANSMITTER_ADDRESS: Address =
    address!("4D41f22c5a0e5c74090899E5a8Fb597a8842b3e8");

/// <https://polygonscan.com/address/0x9daF8c91AEFAE50b9c0E69629D3F6Ca40cA3B3FE>
pub const POLYGON_CCTP_V1_MESSAGE_TRANSMITTER: Address =
    address!("F3be9355363857F3e001be68856A2f96b4C39Ba9");

/// <https://uniscan.xyz/address/0x353bE9E2E38AB1D19104534e4edC21c643Df86f4>
pub const UNICHAIN_CCTP_V1_MESSAGE_TRANSMITTER: Address =
    address!("353bE9E2E38AB1D19104534e4edC21c643Df86f4");

// TokenMessenger Addresses

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const ARBITRUM_TOKEN_MESSENGER_ADDRESS: Address =
    address!("19330d10D9Cc8751218eaf51E8885D058642E08A");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const ARBITRUM_SEPOLIA_TOKEN_MESSENGER_ADDRESS: Address =
    address!("9f3B8679c73C2Fef8b59B4f3444d4e156fb70AA5");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const AVALANCHE_TOKEN_MESSENGER_ADDRESS: Address =
    address!("6b25532e1060ce10cc3b0a99e5683b91bfde6982");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const BASE_TOKEN_MESSENGER_ADDRESS: Address =
    address!("1682ae6375c4e4a97e4b583bc394c861a46d8962");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const BASE_SEPOLIA_TOKEN_MESSENGER_ADDRESS: Address =
    address!("9f3B8679c73C2Fef8b59B4f3444d4e156fb70AA5");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const ETHEREUM_TOKEN_MESSENGER_ADDRESS: Address =
    address!("bd3fa81b58ba92a82136038b25adec7066af3155");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const ETHEREUM_SEPOLIA_TOKEN_MESSENGER_ADDRESS: Address =
    address!("9f3B8679c73C2Fef8b59B4f3444d4e156fb70AA5");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const OPTIMISM_TOKEN_MESSENGER_ADDRESS: Address =
    address!("2B4069517957735bE00ceE0fadAE88a26365528f");

/// <https://developers.circle.com/stablecoins/evm-smart-contracts>
pub const POLYGON_CCTP_V1_TOKEN_MESSENGER: Address =
    address!("9daF8c91AEFAE50b9c0E69629D3F6Ca40cA3B3FE");

/// <https://uniscan.xyz/address/0x4e744b28E787c3aD0e810eD65A24461D4ac5a762>
pub const UNICHAIN_CCTP_V1_TOKEN_MESSENGER: Address =
    address!("4e744b28E787c3aD0e810eD65A24461D4ac5a762");

// =============================================================================
// CCTP V2 Contract Addresses
// =============================================================================
//
// V2 uses unified contract addresses across all chains within each environment.
// This is a major improvement over V1, simplifying integration and reducing
// configuration complexity.
//
// Reference: <https://developers.circle.com/cctp/evm-smart-contracts>

/// CCTP V2 `MessageTransmitter` address (Mainnet)
///
/// Used across ALL v2 mainnet chains including:
/// - Linea (Domain 11)
/// - Sonic (Domain 13)
/// - And other v2-supported mainnets
///
/// <https://developers.circle.com/cctp/evm-smart-contracts>
pub const CCTP_V2_MESSAGE_TRANSMITTER_MAINNET: Address =
    address!("81D40F21F12A8F0E3252Bccb954D722d4c464B64");

/// CCTP V2 `TokenMessenger` address (Mainnet)
///
/// Used across ALL v2 mainnet chains including:
/// - Linea (Domain 11)
/// - Sonic (Domain 13)
/// - And other v2-supported mainnets
///
/// <https://developers.circle.com/cctp/evm-smart-contracts>
pub const CCTP_V2_TOKEN_MESSENGER_MAINNET: Address =
    address!("28b5a0e9C621a5BadaA536219b3a228C8168cf5d");

/// CCTP V2 `MessageTransmitter` address (Testnet)
///
/// Used across ALL v2 testnet chains including:
/// - Linea Sepolia (Domain 11)
/// - Sonic Testnet (Domain 13)
/// - And other v2-supported testnets
///
/// <https://developers.circle.com/cctp/evm-smart-contracts>
pub const CCTP_V2_MESSAGE_TRANSMITTER_TESTNET: Address =
    address!("E737e5cEBEEBa77EFE34D4aa090756590b1CE275");

/// CCTP V2 `TokenMessenger` address (Testnet)
///
/// Used across ALL v2 testnet chains including:
/// - Linea Sepolia (Domain 11)
/// - Sonic Testnet (Domain 13)
/// - And other v2-supported testnets
///
/// <https://developers.circle.com/cctp/evm-smart-contracts>
pub const CCTP_V2_TOKEN_MESSENGER_TESTNET: Address =
    address!("8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA");
