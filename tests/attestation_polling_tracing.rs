// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Tracing-correctness regression tests for the v1 and v2 attestation
//! polling loops.
//!
//! The polling loops must hold their spans across `.await` via
//! `Future::instrument`, never via a `Span::enter()` guard parked in
//! the future's state across a suspension point. Holding a guard
//! across `.await` leaks the entered span onto unrelated tasks once
//! the executor schedules another future on the same thread, producing
//! misleading production traces (see #200 / #202).
//!
//! Each test drives `get_attestation` against a `wiremock` server that
//! returns Pending then Complete, and emits a probe `tracing` event
//! from a sibling `tokio::spawn`-ed task scheduled to fire during the
//! bridge's inter-attempt sleep. With `Future::instrument`, the
//! polling spans are exited when the future yields, so the probe sees
//! none of them as ancestors. With a leaked `Span::enter()` guard, the
//! probe event would be parented to the polling span and these tests
//! fail.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_primitives::{Address, FixedBytes};
use alloy_provider::{Provider, ProviderBuilder};
use cctp_rs::{Cctp, CctpV2Bridge, PollingConfig};
use tracing::Event;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{Layer, Registry};
use url::Url;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const PROBE_TARGET: &str = "cctp_rs_tracing_probe";

#[derive(Default, Debug)]
struct CapturedProbe {
    ancestor_span_names: Vec<String>,
}

struct ProbeCaptureLayer {
    captures: Arc<Mutex<Vec<CapturedProbe>>>,
}

impl<S> Layer<S> for ProbeCaptureLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != PROBE_TARGET {
            return;
        }
        let mut ancestor_span_names = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                ancestor_span_names.push(span.name().to_string());
            }
        }
        self.captures.lock().unwrap().push(CapturedProbe {
            ancestor_span_names,
        });
    }
}

fn install_probe_subscriber() -> (
    Arc<Mutex<Vec<CapturedProbe>>>,
    tracing::subscriber::DefaultGuard,
) {
    let captures = Arc::new(Mutex::new(Vec::new()));
    let layer = ProbeCaptureLayer {
        captures: captures.clone(),
    };
    let subscriber = Registry::default().with(layer);
    let guard = tracing::subscriber::set_default(subscriber);
    (captures, guard)
}

fn dummy_provider() -> impl Provider<Ethereum> + Clone {
    ProviderBuilder::new().connect_http("http://127.0.0.1:1/".parse().unwrap())
}

fn assert_no_leaked_attestation_span(captures: &[CapturedProbe]) {
    assert!(
        !captures.is_empty(),
        "expected at least one probe event to be captured during polling"
    );
    for capture in captures {
        for ancestor in &capture.ancestor_span_names {
            assert!(
                !ancestor.starts_with("cctp_rs."),
                "probe event was parented to span `{ancestor}`; the \
                 attestation polling loop is holding a tracing span \
                 across `.await`. This regresses #200 / #202."
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn v1_attestation_polling_does_not_leak_span_across_await() {
    let (captures, _guard) = install_probe_subscriber();

    let server = MockServer::start().await;
    let attestation_hex = format!("0x{}", "ab".repeat(65));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "pending_confirmations"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "complete",
            "attestation": attestation_hex,
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

    let probe = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        tracing::info!(target: PROBE_TARGET, "probe_during_polling_sleep");
    });

    let result = bridge
        .get_attestation(
            FixedBytes::from([0x12u8; 32]),
            PollingConfig::default()
                .with_max_attempts(3)
                .with_poll_interval_secs(1),
        )
        .await;
    probe.await.expect("probe task panicked");

    let attestation = result.expect("polling should succeed once Complete is returned");
    assert_eq!(attestation.len(), 65);

    assert_no_leaked_attestation_span(&captures.lock().unwrap());
}

#[tokio::test(flavor = "current_thread")]
async fn v2_attestation_polling_does_not_leak_span_across_await() {
    let (captures, _guard) = install_probe_subscriber();

    let server = MockServer::start().await;
    let message_hex = format!("0x{}", "cc".repeat(228));
    let attestation_hex = format!("0x{}", "ab".repeat(65));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
                { "status": "pending_confirmations" }
            ]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messages": [
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
    let provider = dummy_provider();
    let bridge = CctpV2Bridge::builder()
        .source_chain(NamedChain::Mainnet)
        .destination_chain(NamedChain::Linea)
        .source_provider(provider.clone())
        .destination_provider(provider)
        .recipient(Address::ZERO)
        .api_url_override(api_override)
        .build();

    let probe = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        tracing::info!(target: PROBE_TARGET, "probe_during_polling_sleep");
    });

    let result = bridge
        .get_attestation(
            FixedBytes::from([0xabu8; 32]),
            PollingConfig::fast_transfer()
                .with_max_attempts(3)
                .with_poll_interval_secs(1),
        )
        .await;
    probe.await.expect("probe task panicked");

    let (message, attestation) = result.expect("polling should succeed once Complete is returned");
    assert_eq!(message.len(), 228);
    assert_eq!(attestation.len(), 65);

    assert_no_leaked_attestation_span(&captures.lock().unwrap());
}
