// SPDX-FileCopyrightText: 2026 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Regression tests for #201: the per-attempt and per-response spans
//! emitted by the v1 and v2 attestation polling loops must declare
//! `error.type`, `error.message`, `error.context`, and
//! `otel.status_code` so that `spans::record_error_with_context` writes
//! land on the innermost span.
//!
//! `tracing::Span::record` silently drops writes for fields that were
//! not declared at span construction. If the inner spans omit these
//! field declarations, the error attributes the SDK appears to emit
//! for `HttpRequestFailed`, `AttestationFailed`,
//! `AttestationDataMissing`, and `MessageDataMissing` never actually
//! materialize on any span, breaking Tempo/Jaeger queries like
//! `{ span.error.type = "HttpRequestFailed" }`.
//!
//! Each test installs a `tracing_subscriber::Layer` that captures
//! `on_record` calls, drives `get_attestation` against a failure
//! scenario (an unreachable HTTP host for the attempt-span path; a
//! `failed` Circle API status for the process-response-span path),
//! and asserts that the expected field landed on the expected span.

use std::sync::{Arc, Mutex};

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use cctp_rs::{Cctp, CctpV2Bridge, PollingConfig};
use tracing::field::{Field, Visit};
use tracing::span::Record;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};
use url::Url;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Clone)]
struct CapturedField {
    span_name: String,
    field: String,
    value: String,
}

struct FieldRecordCaptureLayer {
    records: Arc<Mutex<Vec<CapturedField>>>,
}

impl<S> Layer<S> for FieldRecordCaptureLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_record(&self, id: &tracing::span::Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let span_name = span.name().to_string();
        let mut sink = self.records.lock().unwrap();
        let mut visitor = FieldVisitor {
            records: &mut sink,
            span_name,
        };
        values.record(&mut visitor);
    }
}

struct FieldVisitor<'a> {
    records: &'a mut Vec<CapturedField>,
    span_name: String,
}

impl Visit for FieldVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.records.push(CapturedField {
            span_name: self.span_name.clone(),
            field: field.name().to_string(),
            value: value.to_string(),
        });
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.records.push(CapturedField {
            span_name: self.span_name.clone(),
            field: field.name().to_string(),
            value: format!("{value:?}"),
        });
    }
}

fn install_record_subscriber() -> (
    Arc<Mutex<Vec<CapturedField>>>,
    tracing::subscriber::DefaultGuard,
) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let layer = FieldRecordCaptureLayer {
        records: records.clone(),
    };
    let subscriber = Registry::default().with(layer);
    let guard = tracing::subscriber::set_default(subscriber);
    (records, guard)
}

fn dummy_provider() -> impl Provider<Ethereum> + Clone {
    ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap())
}

fn assert_field_recorded(
    records: &[CapturedField],
    expected_span: &str,
    expected_field: &str,
    expected_value: &str,
) {
    let matched = records.iter().any(|r| {
        r.span_name == expected_span && r.field == expected_field && r.value == expected_value
    });
    assert!(
        matched,
        "expected span `{expected_span}` to record field `{expected_field}={expected_value}`, \
         but no such record was observed. Captured records: {records:#?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v1_process_response_span_records_attestation_failed() {
    let (records, _guard) = install_record_subscriber();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "failed"
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let provider = dummy_provider();
    let bridge = Cctp::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Arbitrum)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0x12u8; 32]),
            PollingConfig::default()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(result.is_err(), "expected polling to fail on failed status");

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "error.type",
        "AttestationFailed",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v1_attempt_span_records_http_request_failed() {
    let (records, _guard) = install_record_subscriber();

    let unreachable = Url::parse("http://127.0.0.1:1/").expect("static url");
    let provider = dummy_provider();
    let bridge = Cctp::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Arbitrum)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(unreachable)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0x12u8; 32]),
            PollingConfig::default()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(
        result.is_err(),
        "expected polling to fail against unreachable host"
    );

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.get_attestation",
        "error.type",
        "HttpRequestFailed",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.get_attestation",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v1_process_response_span_records_attestation_data_missing() {
    let (records, _guard) = install_record_subscriber();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "complete",
            "attestation": null
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let provider = dummy_provider();
    let bridge = Cctp::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Arbitrum)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0x12u8; 32]),
            PollingConfig::default()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(
        result.is_err(),
        "expected polling to fail on complete status with null attestation"
    );

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "error.type",
        "AttestationDataMissing",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v2_process_response_span_records_attestation_failed() {
    let (records, _guard) = install_record_subscriber();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "failed" }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let provider = dummy_provider();
    let bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(result.is_err(), "expected polling to fail on failed status");

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "error.type",
        "AttestationFailed",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v2_attempt_span_records_http_request_failed() {
    let (records, _guard) = install_record_subscriber();

    let unreachable = Url::parse("http://127.0.0.1:1/").expect("static url");
    let provider = dummy_provider();
    let bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(unreachable)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(
        result.is_err(),
        "expected polling to fail against unreachable host"
    );

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.get_attestation",
        "error.type",
        "HttpRequestFailed",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.get_attestation",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v2_process_response_span_records_attestation_data_missing() {
    let (records, _guard) = install_record_subscriber();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                {
                    "status": "complete",
                    "message": "0xdeadbeef",
                    "attestation": null
                }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let provider = dummy_provider();
    let bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(
        result.is_err(),
        "expected polling to fail on complete status with null attestation"
    );

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "error.type",
        "AttestationDataMissing",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "otel.status_code",
        "ERROR",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn v2_process_response_span_records_message_data_missing() {
    let (records, _guard) = install_record_subscriber();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                {
                    "status": "complete",
                    "message": null,
                    "attestation": "0x1234abcd"
                }
            ]
        })))
        .mount(&server)
        .await;

    let api_override = Url::parse(&server.uri()).expect("wiremock URI parses as Url");
    let provider = dummy_provider();
    let bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let result = bridge
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(1)
                .with_poll_interval_secs(1),
        )
        .await;
    assert!(
        result.is_err(),
        "expected polling to fail on complete status with null message"
    );

    let captured = records.lock().unwrap();
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "error.type",
        "MessageDataMissing",
    );
    assert_field_recorded(
        &captured,
        "cctp_rs.process_attestation_response",
        "otel.status_code",
        "ERROR",
    );
}
