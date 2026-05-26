// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! End-to-end coverage for v2 attestation polling over multi-message
//! responses (issue #213).
//!
//! A v2 query is keyed by transaction hash, and one transaction can emit
//! several `MessageSent` events, so Circle's Iris API may return an array
//! mixing pending, complete, and failed entries. The polling loop used to
//! read only `messages.first()`, so a usable attestation sitting behind a
//! pending or failed sibling was ignored or treated as a whole-transaction
//! failure. These tests drive the real `get_attestation` HTTP/JSON path
//! against a `wiremock` server to prove the array-wide selection wires
//! through end to end.

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use cctp_rs::{AttestationFailureKind, CctpError, CctpV2Bridge, PollingConfig};
use url::Url;
use wiremock::matchers::method;
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

#[tokio::test]
async fn returns_complete_message_behind_failed_sibling() {
    let server = MockServer::start().await;
    let message_hex = format!("0x{}", "cc".repeat(228));
    let attestation_hex = format!("0x{}", "ab".repeat(65));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "failed", "message": null, "attestation": null },
                {
                    "status": "complete",
                    "message": message_hex,
                    "attestation": attestation_hex,
                }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let (message, attestation) = bridge(api_override)
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect("a complete message must be returned despite a failed sibling");

    assert_eq!(message.len(), 228);
    assert_eq!(attestation.len(), 65);
}

#[tokio::test]
async fn polls_until_a_pending_sibling_completes() {
    // First poll: one failed, one still pending — must keep polling rather
    // than failing on the failed entry. Second poll: the pending sibling
    // resolves into a usable attestation.
    let server = MockServer::start().await;
    let message_hex = format!("0x{}", "cc".repeat(228));
    let attestation_hex = format!("0x{}", "ab".repeat(65));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "failed", "message": null, "attestation": null },
                { "status": "pending", "message": null, "attestation": null }
            ]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "failed", "message": null, "attestation": null },
                {
                    "status": "complete",
                    "message": message_hex,
                    "attestation": attestation_hex,
                }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let (message, attestation) = bridge(api_override)
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(3)
                .with_poll_interval_secs(1),
        )
        .await
        .expect("polling must continue past a failed sibling until the pending one completes");

    assert_eq!(message.len(), 228);
    assert_eq!(attestation.len(), 65);
}

#[tokio::test]
async fn fails_when_every_message_failed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "failed", "message": null, "attestation": null },
                { "status": "failed", "message": null, "attestation": null }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let err = bridge(api_override)
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await
        .expect_err("an all-failed response must surface as a failure, not succeed");

    // The failure must be the API-reported-failed variant, not a timeout:
    // with nothing pending or usable, the loop should fail on the spot
    // rather than burn its attempt budget.
    assert!(
        matches!(
            err,
            CctpError::AttestationFailed(AttestationFailureKind::ApiReportedFailed)
        ),
        "expected AttestationFailed(ApiReportedFailed), got {err:?}"
    );
}
