// SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

//! End-to-end coverage for normalizing untrusted Iris status/payload
//! combinations before bridge polling decisions.

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use cctp_rs::{AttestationFailureKind, Cctp, CctpError, CctpV2Bridge, PollingConfig};
use url::Url;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dummy_provider() -> impl Provider<Ethereum> + Clone {
    ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap())
}

fn v1_bridge(api_override: Url) -> Cctp<impl Provider<Ethereum> + Clone> {
    let provider = dummy_provider();
    Cctp::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Arbitrum)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build()
}

fn v2_bridge(api_override: Url) -> CctpV2Bridge<impl Provider<Ethereum> + Clone> {
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

fn assert_failure_kind(error: CctpError, expected: AttestationFailureKind) {
    assert!(
        matches!(error, CctpError::AttestationFailed(actual) if actual == expected),
        "expected AttestationFailed({expected:?}), got {error:?}"
    );
}

#[tokio::test]
async fn v1_rejects_pending_response_with_attestation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "pending",
            "attestation": "0xaa"
        })))
        .mount(&server)
        .await;

    let error = v1_bridge(Url::parse(&server.uri()).expect("wiremock URI parses"))
        .get_attestation(
            FixedBytes::from([0x12; 32]),
            PollingConfig::default()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect_err("status-inconsistent attestation must fail");

    assert_failure_kind(error, AttestationFailureKind::UnexpectedAttestation);
}

#[tokio::test]
async fn v1_rejects_present_but_empty_attestation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "complete",
            "attestation": ""
        })))
        .mount(&server)
        .await;

    let error = v1_bridge(Url::parse(&server.uri()).expect("wiremock URI parses"))
        .get_attestation(
            FixedBytes::from([0x12; 32]),
            PollingConfig::default()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect_err("empty attestation must fail");

    assert_failure_kind(error, AttestationFailureKind::AttestationEmpty);
}

#[tokio::test]
async fn v2_rejects_pending_response_with_attestation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [{
                "status": "pending",
                "message": "0xaa",
                "attestation": "0xbb"
            }]
        })))
        .mount(&server)
        .await;

    let error = v2_bridge(Url::parse(&server.uri()).expect("wiremock URI parses"))
        .get_attestation(
            FixedBytes::from([0xab; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect_err("status-inconsistent attestation must fail");

    assert_failure_kind(error, AttestationFailureKind::UnexpectedAttestation);
}

#[tokio::test]
async fn v2_rejects_present_but_empty_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [{
                "status": "complete",
                "message": "",
                "attestation": "0xbb"
            }]
        })))
        .mount(&server)
        .await;

    let error = v2_bridge(Url::parse(&server.uri()).expect("wiremock URI parses"))
        .get_attestation(
            FixedBytes::from([0xab; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect_err("empty message must fail");

    assert_failure_kind(error, AttestationFailureKind::MessageEmpty);
}
