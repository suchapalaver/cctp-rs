// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use std::{env, fmt::Display, str::FromStr};

use alloy_chains::NamedChain;
use alloy_primitives::{address, Address};
use cctp_rs::{CctpError, CctpV2Route};
use thiserror::Error;
use url::Url;

pub const DEFAULT_SOURCE_CHAIN: NamedChain = NamedChain::ArbitrumSepolia;
pub const DEFAULT_DESTINATION_CHAIN: NamedChain = NamedChain::BaseSepolia;
pub const DEFAULT_TRANSFER_AMOUNT: u64 = 1_000_000;
pub const MIN_NATIVE_BALANCE_WEI: u64 = 1_000_000_000_000_000;
pub const MIN_USDC_BALANCE: u64 = 1_000_000;

#[derive(Debug, Error)]
pub enum FundedV2Error {
    #[error(transparent)]
    Cctp(#[from] CctpError),
    #[error("funded examples refuse mainnet routes; use testnet source and destination chains")]
    MainnetRoute,
    #[error(
        "{env_var}={value:?} is not a recognized chain; supported funded testnet aliases: {supported_aliases}"
    )]
    UnrecognizedChain {
        env_var: String,
        value: String,
        supported_aliases: &'static str,
    },
    #[error("set {generic_env_var}, {chain_env_var}, or TESTNET_API_KEY for {chain}")]
    MissingRpcConfig {
        generic_env_var: String,
        chain_env_var: &'static str,
        chain: NamedChain,
    },
    #[error("{env_var} must be a valid HTTP RPC URL: {source}")]
    InvalidRpcUrl {
        env_var: String,
        #[source]
        source: url::ParseError,
    },
    #[error("{env_var} must be an HTTP RPC URL with a host")]
    InvalidRpcEndpoint { env_var: String },
    #[error("{env_var} must be a valid EVM address: {source}")]
    InvalidUsdcAddress {
        env_var: String,
        #[source]
        source: alloy_primitives::hex::FromHexError,
    },
    #[error("no built-in testnet USDC address for {chain}; set {override_env_var}")]
    MissingUsdcAddress {
        chain: NamedChain,
        override_env_var: String,
    },
    #[error(
        "{chain} is not in this funded example's built-in testnet route table; supported routes use: {supported_aliases}"
    )]
    UnsupportedChain {
        chain: NamedChain,
        supported_aliases: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct FundedV2Route {
    pub route: CctpV2Route,
    pub source_rpc_url: Url,
    pub destination_rpc_url: Url,
    pub source_usdc_address: Address,
    pub destination_usdc_address: Address,
}

impl FundedV2Route {
    pub fn from_env() -> Result<Self, FundedV2Error> {
        let source_chain = chain_from_env("SOURCE_CHAIN", DEFAULT_SOURCE_CHAIN)?;
        let destination_chain = chain_from_env("DESTINATION_CHAIN", DEFAULT_DESTINATION_CHAIN)?;
        let route = CctpV2Route::new(source_chain, destination_chain)?;

        if !source_chain.is_testnet() || !destination_chain.is_testnet() {
            return Err(FundedV2Error::MainnetRoute);
        }

        Ok(Self {
            route,
            source_rpc_url: rpc_url_for(source_chain, "SOURCE_RPC_URL")?,
            destination_rpc_url: rpc_url_for(destination_chain, "DESTINATION_RPC_URL")?,
            source_usdc_address: usdc_address_for(source_chain, "SOURCE_USDC_ADDRESS")?,
            destination_usdc_address: usdc_address_for(
                destination_chain,
                "DESTINATION_USDC_ADDRESS",
            )?,
        })
    }

    pub fn source_chain(&self) -> NamedChain {
        self.route.source_chain()
    }

    pub fn destination_chain(&self) -> NamedChain {
        self.route.destination_chain()
    }

    pub fn source_tx_url(&self, tx_hash: impl Display) -> String {
        tx_url(self.source_chain(), tx_hash)
    }

    pub fn destination_tx_url(&self, tx_hash: impl Display) -> String {
        tx_url(self.destination_chain(), tx_hash)
    }
}

pub fn format_eth_balance(balance: alloy_primitives::U256) -> String {
    let eth = balance.to::<u128>() as f64 / 1e18;
    format!("{eth:.6}")
}

pub fn format_usdc_balance(balance: alloy_primitives::U256) -> String {
    let usdc = balance.to::<u128>() as f64 / 1e6;
    format!("{usdc:.6}")
}

pub fn redacted_rpc_url(url: &Url) -> String {
    let mut authority = url
        .host_str()
        .map_or_else(|| "<unknown-host>".to_string(), ToString::to_string);
    if let Some(port) = url.port() {
        authority.push(':');
        authority.push_str(&port.to_string());
    }
    format!("{}://{authority}/<redacted>", url.scheme())
}

pub fn supported_testnet_routes() -> &'static str {
    "sepolia, arbitrum-sepolia, base-sepolia, optimism-sepolia, avalanche-fuji, polygon-amoy"
}

fn chain_from_env(env_var: &str, default: NamedChain) -> Result<NamedChain, FundedV2Error> {
    match env::var(env_var) {
        Ok(raw) if !raw.trim().is_empty() => parse_chain(env_var, &raw),
        _ => Ok(default),
    }
}

fn parse_chain(env_var: &str, raw: &str) -> Result<NamedChain, FundedV2Error> {
    let normalized = raw.trim().to_ascii_lowercase().replace(['_', ' '], "-");

    let chain = match normalized.as_str() {
        "ethereum-sepolia" | "eth-sepolia" | "sepolia" => NamedChain::Sepolia,
        "arbitrum-sepolia" | "arb-sepolia" => NamedChain::ArbitrumSepolia,
        "base-sepolia" => NamedChain::BaseSepolia,
        "optimism-sepolia" | "op-sepolia" => NamedChain::OptimismSepolia,
        "avalanche-fuji" | "fuji" => NamedChain::AvalancheFuji,
        "polygon-amoy" | "amoy" => NamedChain::PolygonAmoy,
        _ => NamedChain::from_str(&normalized)
            .or_else(|_| NamedChain::from_str(raw.trim()))
            .map_err(|_| FundedV2Error::UnrecognizedChain {
                env_var: env_var.to_string(),
                value: raw.to_string(),
                supported_aliases: supported_testnet_routes(),
            })?,
    };

    Ok(chain)
}

fn rpc_url_for(chain: NamedChain, generic_env_var: &str) -> Result<Url, FundedV2Error> {
    if let Ok(raw) = env::var(generic_env_var) {
        if !raw.trim().is_empty() {
            return parse_rpc_url(generic_env_var, &raw);
        }
    }

    let chain_env_var = rpc_env_var(chain).ok_or_else(|| unsupported_chain_env(chain))?;
    if let Ok(raw) = env::var(chain_env_var) {
        if !raw.trim().is_empty() {
            return parse_rpc_url(chain_env_var, &raw);
        }
    }

    let api_key = env::var("TESTNET_API_KEY").map_err(|_| FundedV2Error::MissingRpcConfig {
        generic_env_var: generic_env_var.to_string(),
        chain_env_var,
        chain,
    })?;

    let alchemy_host = alchemy_host(chain).ok_or_else(|| unsupported_chain_env(chain))?;
    parse_rpc_url(
        "TESTNET_API_KEY",
        &format!("https://{alchemy_host}.g.alchemy.com/v2/{api_key}"),
    )
}

fn parse_rpc_url(env_var: &str, rpc_url: &str) -> Result<Url, FundedV2Error> {
    let url: Url = rpc_url
        .parse()
        .map_err(|source| FundedV2Error::InvalidRpcUrl {
            env_var: env_var.to_string(),
            source,
        })?;

    match (url.scheme(), url.host_str()) {
        ("http" | "https", Some(_)) => Ok(url),
        _ => Err(FundedV2Error::InvalidRpcEndpoint {
            env_var: env_var.to_string(),
        }),
    }
}

fn usdc_address_for(chain: NamedChain, override_env_var: &str) -> Result<Address, FundedV2Error> {
    if let Ok(raw) = env::var(override_env_var) {
        if !raw.trim().is_empty() {
            return raw
                .trim()
                .parse()
                .map_err(|source: alloy_primitives::hex::FromHexError| {
                    FundedV2Error::InvalidUsdcAddress {
                        env_var: override_env_var.to_string(),
                        source,
                    }
                });
        }
    }

    default_testnet_usdc(chain).ok_or_else(|| FundedV2Error::MissingUsdcAddress {
        chain,
        override_env_var: override_env_var.to_string(),
    })
}

fn default_testnet_usdc(chain: NamedChain) -> Option<Address> {
    Some(match chain {
        NamedChain::Sepolia => address!("1c7D4B196Cb0C7B01d743Fbc6116a902379C7238"),
        NamedChain::AvalancheFuji => address!("5425890298aed601595a70AB815c96711a31Bc65"),
        NamedChain::OptimismSepolia => address!("5fd84259d66Cd46123540766Be93DFE6D43130D7"),
        NamedChain::ArbitrumSepolia => address!("75faf114eafb1BDbe2F0316DF893fd58CE46AA4d"),
        NamedChain::BaseSepolia => address!("036CbD53842c5426634e7929541eC2318f3dCF7e"),
        NamedChain::PolygonAmoy => address!("41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582"),
        _ => return None,
    })
}

fn rpc_env_var(chain: NamedChain) -> Option<&'static str> {
    Some(match chain {
        NamedChain::Sepolia => "SEPOLIA_RPC_URL",
        NamedChain::AvalancheFuji => "AVALANCHE_FUJI_RPC_URL",
        NamedChain::OptimismSepolia => "OPTIMISM_SEPOLIA_RPC_URL",
        NamedChain::ArbitrumSepolia => "ARBITRUM_SEPOLIA_RPC_URL",
        NamedChain::BaseSepolia => "BASE_SEPOLIA_RPC_URL",
        NamedChain::PolygonAmoy => "POLYGON_AMOY_RPC_URL",
        _ => return None,
    })
}

fn alchemy_host(chain: NamedChain) -> Option<&'static str> {
    Some(match chain {
        NamedChain::Sepolia => "eth-sepolia",
        NamedChain::AvalancheFuji => "avax-fuji",
        NamedChain::OptimismSepolia => "opt-sepolia",
        NamedChain::ArbitrumSepolia => "arb-sepolia",
        NamedChain::BaseSepolia => "base-sepolia",
        NamedChain::PolygonAmoy => "polygon-amoy",
        _ => return None,
    })
}

fn tx_url(chain: NamedChain, tx_hash: impl Display) -> String {
    match tx_explorer_base(chain) {
        Some(base_url) => format!("{base_url}/tx/{tx_hash}"),
        None => tx_hash.to_string(),
    }
}

fn tx_explorer_base(chain: NamedChain) -> Option<&'static str> {
    Some(match chain {
        NamedChain::Sepolia => "https://sepolia.etherscan.io",
        NamedChain::AvalancheFuji => "https://testnet.snowtrace.io",
        NamedChain::OptimismSepolia => "https://sepolia-optimism.etherscan.io",
        NamedChain::ArbitrumSepolia => "https://sepolia.arbiscan.io",
        NamedChain::BaseSepolia => "https://base-sepolia.blockscout.com",
        NamedChain::PolygonAmoy => "https://amoy.polygonscan.com",
        _ => return None,
    })
}

fn unsupported_chain_env(chain: NamedChain) -> FundedV2Error {
    FundedV2Error::UnsupportedChain {
        chain,
        supported_aliases: supported_testnet_routes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_chain_alias_preserves_typed_context() {
        let error = parse_chain("SOURCE_CHAIN", "definitely-not-a-chain").unwrap_err();
        assert!(matches!(
            error,
            FundedV2Error::UnrecognizedChain {
                ref env_var,
                ref value,
                ..
            } if env_var == "SOURCE_CHAIN" && value == "definitely-not-a-chain"
        ));
    }

    #[test]
    fn malformed_rpc_url_preserves_parse_error() {
        let error = parse_rpc_url("SOURCE_RPC_URL", "://bad").unwrap_err();
        assert!(matches!(
            error,
            FundedV2Error::InvalidRpcUrl { ref env_var, .. }
                if env_var == "SOURCE_RPC_URL"
        ));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn non_http_rpc_url_has_distinct_category() {
        let error = parse_rpc_url("SOURCE_RPC_URL", "ws://localhost:8545").unwrap_err();
        assert!(matches!(
            error,
            FundedV2Error::InvalidRpcEndpoint { ref env_var }
                if env_var == "SOURCE_RPC_URL"
        ));
    }
}
