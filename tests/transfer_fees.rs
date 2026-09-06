// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
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
use cctp_rs::{CctpError, CctpTransferAsset, CctpV2Bridge, FeeBps};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dummy_provider() -> impl Provider<Ethereum> + Clone {
    ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap())
}

fn bridge(api_override: Url) -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
    bridge_for_route(api_override, NamedChain::Mainnet, NamedChain::Linea)
}

fn bridge_for_route(
    api_override: Url,
    source_chain: NamedChain,
    destination_chain: NamedChain,
) -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
    let provider = dummy_provider();
    CctpV2Bridge::builder()
        .source_chain(source_chain)
        .destination_chain(destination_chain)
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

#[test]
fn usdc_fee_url_is_built_from_asset_segment() {
    let bridge = bridge(Url::parse("https://iris-api.circle.com").expect("valid URL"));
    let url = bridge
        .create_transfer_fees_url_for_asset(CctpTransferAsset::Usdc)
        .expect("USDC fee URL should build");

    assert_eq!(
        url.as_str(),
        "https://iris-api.circle.com/v2/burn/USDC/fees/0/11"
    );
}

#[tokio::test]
async fn eurc_fee_lookup_reports_unpublished_endpoint_for_supported_route() {
    let server = MockServer::start().await;
    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let bridge = bridge_for_route(api_override, NamedChain::Mainnet, NamedChain::Base);

    let err = bridge
        .get_transfer_fees_for_asset(CctpTransferAsset::Eurc)
        .await
        .expect_err("Circle has not published an EURC fee endpoint yet");

    assert!(matches!(
        err,
        CctpError::TransferFeeEndpointUnavailable {
            asset: CctpTransferAsset::Eurc,
            source_domain: 0,
            destination_domain: 6,
            ..
        }
    ));
}

#[test]
fn eurc_fee_url_rejects_unannounced_asset_route() {
    let bridge = bridge(Url::parse("https://iris-api.circle.com").expect("valid URL"));
    let err = bridge
        .create_transfer_fees_url_for_asset(CctpTransferAsset::Eurc)
        .expect_err("Ethereum -> Linea is not a modeled EURC route");

    assert!(matches!(
        err,
        CctpError::UnsupportedAssetRoute {
            asset: CctpTransferAsset::Eurc,
            source_chain: NamedChain::Mainnet,
            destination_chain: NamedChain::Linea,
            ..
        }
    ));
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
