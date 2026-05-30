// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! End-to-end coverage for live CCTP v2 transfer fee lookup (issues #4-#7).
//!
//! These tests drive the real HTTP/JSON path against a `wiremock` server so the
//! SDK-level helpers cover URL construction, response decoding, finality
//! selection, and buffered `maxFee` calculation without hitting Circle Iris.
//!
//! The ignored live smoke tests are opt-in drift checks against Iris:
//!
//! ```text
//! cargo test --test transfer_fees live_ --all-features -- --ignored --nocapture
//! ```

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use cctp_rs::{CctpError, CctpV2Bridge, FeeBps};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dummy_provider() -> impl Provider<Ethereum> + Clone {
    ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap())
}

fn bridge(api_override: Url) -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
    let provider = dummy_provider();
    CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build()
}

fn live_sandbox_bridge() -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
    let provider = dummy_provider();
    CctpV2Bridge::builder()
        .source_chain(NamedChain::Sepolia)
        .destination_chain(NamedChain::BaseSepolia)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .build()
}

fn live_mainnet_bridge() -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
    let provider = dummy_provider();
    CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Base)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .build()
}

#[tokio::test]
async fn fetches_route_fees_and_calculates_buffered_fast_max_fee() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/burn/USDC/fees/0/11"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "finalityThreshold": 1000, "minimumFee": 1.3 },
            { "finalityThreshold": 2000, "minimumFee": 0 }
        ])))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let bridge = bridge(api_override);

    let fees = bridge
        .get_transfer_fees()
        .await
        .expect("fee response should decode");
    assert_eq!(fees.len(), 2);
    assert_eq!(fees[0].minimum_fee, FeeBps::from_hundredths(130));

    let fast_fee = bridge
        .get_fast_transfer_fee()
        .await
        .expect("fee response should decode")
        .expect("fast fee entry should be selected");
    assert_eq!(fast_fee.minimum_fee, FeeBps::from_hundredths(130));

    let standard_fee = bridge
        .get_standard_transfer_fee()
        .await
        .expect("fee response should decode")
        .expect("standard fee entry should be selected");
    assert_eq!(standard_fee.minimum_fee, FeeBps::from_hundredths(0));
    assert!(standard_fee.is_standard_transfer());

    let max_fee = bridge
        .calculate_fast_transfer_max_fee(U256::from(10_500_000u64), 20)
        .await
        .expect("fast max fee should calculate");
    assert_eq!(max_fee, U256::from(1638u64));
}

#[tokio::test]
async fn missing_fast_fee_returns_route_context_for_max_fee_calculation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/burn/USDC/fees/0/11"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "finalityThreshold": 2000, "minimumFee": 0 }
        ])))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let err = bridge(api_override)
        .calculate_fast_transfer_max_fee(U256::from(10_500_000u64), 20)
        .await
        .expect_err("missing fast fee should be explicit");

    assert!(matches!(
        err,
        CctpError::TransferFeeUnavailable {
            source_domain: 0,
            destination_domain: 11,
            finality_threshold: 1000,
        }
    ));
}

#[tokio::test]
async fn non_success_fee_response_returns_network_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/burn/USDC/fees/0/11"))
        .respond_with(ResponseTemplate::new(500).set_body_string("iris unavailable"))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let err = bridge(api_override)
        .get_transfer_fees()
        .await
        .expect_err("HTTP status failures should be surfaced");

    assert!(matches!(
        err,
        CctpError::Network(ref network_error)
            if network_error.status().is_some_and(|status| status.as_u16() == 500)
    ));
}

#[tokio::test]
async fn malformed_fee_response_returns_json_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/burn/USDC/fees/0/11"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let err = bridge(api_override)
        .get_transfer_fees()
        .await
        .expect_err("malformed JSON should be surfaced");

    assert!(matches!(err, CctpError::Json(_)));
}

#[tokio::test]
#[ignore]
async fn live_sandbox_fee_lookup_smoke_test() {
    let bridge = live_sandbox_bridge();
    let url = bridge
        .create_transfer_fees_url()
        .expect("testnet route should construct a fee URL");
    assert_eq!(
        url.as_str(),
        "https://iris-api-sandbox.circle.com/v2/burn/USDC/fees/0/6"
    );

    let fees = bridge
        .get_transfer_fees()
        .await
        .expect("Iris sandbox fee response should decode");

    assert!(!fees.is_empty(), "Iris sandbox should return fee entries");
    assert!(
        fees.iter().any(|fee| fee.finality_threshold == 1000),
        "Iris sandbox route should include a Fast Transfer fee"
    );
}

#[tokio::test]
#[ignore]
async fn live_mainnet_fee_lookup_smoke_test() {
    let bridge = live_mainnet_bridge();
    let url = bridge
        .create_transfer_fees_url()
        .expect("mainnet route should construct a fee URL");
    assert_eq!(
        url.as_str(),
        "https://iris-api.circle.com/v2/burn/USDC/fees/0/6"
    );

    let fees = bridge
        .get_transfer_fees()
        .await
        .expect("Iris mainnet fee response should decode");

    assert!(!fees.is_empty(), "Iris mainnet should return fee entries");
    assert!(
        fees.iter().any(|fee| fee.finality_threshold == 1000),
        "Iris mainnet route should include a Fast Transfer fee"
    );
}
