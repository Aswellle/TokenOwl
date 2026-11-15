//! Model-only L2 providers must answer a refresh with a health verdict, not
//! a fetch failure. These tests drive the real adapter against loopback
//! servers covering the acceptance matrix for the models probe: success,
//! rejected key, rate limit, server outage, malformed JSON, and oversized
//! response bodies.

mod common;

use common::{json_response, spawn_router, spawn_server};
use tokenowl_lib::credentials::CredentialStore;
use tokenowl_lib::domain::provider::{
    DataSource, OpenAiCompatibleConfig, ProviderErrorKind, SnapshotStatus,
};
use tokenowl_lib::providers::builtin::openai::{
    AdapterLimits, MODELS_PROBE_SNAPSHOT_ID, OpenAiCompatibleAdapter, ProbeOutcome,
};

fn model_only_config(base_url: &str) -> OpenAiCompatibleConfig {
    let mut config = OpenAiCompatibleConfig::new("fixture-l2", "Fixture L2", base_url);
    // The default config already points at /models; make the intent explicit.
    config.models_endpoint = Some("/models".to_owned());
    config.quota_endpoint = None;
    config.usage_endpoint = None;
    config.balance_endpoint = None;
    config.credential_ref = Some("account/probe-fixture".to_owned());
    config
}

fn store_with_key(key: &str) -> CredentialStore {
    let store = CredentialStore::default();
    store.set("account/probe-fixture", key).expect("keyring");
    store
}

/// The builtin registry's own model-only accounts ride the official-API
/// source; a custom id stays custom. Both must produce one explicit
/// unavailable snapshot with no invented quota values.
#[test]
fn a_verified_models_probe_answers_with_an_unknown_snapshot() {
    let server = spawn_server(json_response(
        "200 OK",
        r#"{"object":"list","data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#,
    ));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store));
    let result = result.expect("verified probe");
    assert_eq!(result.snapshots.len(), 1);
    let snapshot = &result.snapshots.snapshots[0];
    assert_eq!(snapshot.id.as_deref(), Some(MODELS_PROBE_SNAPSHOT_ID));
    assert_eq!(snapshot.status, SnapshotStatus::Unavailable);
    assert_eq!(snapshot.used, None);
    assert_eq!(snapshot.limit, None);
    assert_eq!(snapshot.remaining, None);
    assert_eq!(snapshot.reset_at, None);
    assert_eq!(snapshot.account_id.as_deref(), Some("account-1"));
    assert_eq!(snapshot.source, DataSource::CustomEndpoint);
    assert!(
        server
            .requested_paths()
            .iter()
            .all(|path| path == "/models"),
        "the probe must hit the models endpoint only, got {:?}",
        server.requested_paths()
    );
}

/// A builtin provider id labels the probe traffic as the official API.
#[test]
fn builtin_ids_label_the_probe_as_official_api() {
    let server = spawn_server(json_response("200 OK", r#"{"data":[]}"#));
    let mut config = model_only_config(&server.base_url);
    config.id = "openai".to_owned();
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect("verified probe");
    assert_eq!(
        result.snapshots.snapshots[0].source,
        DataSource::OfficialApi
    );
}

/// A refused key is a structured auth error, not a generic failure.
#[test]
fn a_rejected_key_maps_to_auth_required() {
    let server = spawn_server(json_response(
        "401 Unauthorized",
        r#"{"error":{"message":"Incorrect API key"}}"#,
    ));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-revoked");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("rejected key");
    assert_eq!(error.code, ProviderErrorKind::AuthRequired);
    assert_eq!(error.http_status, Some(401));
}

/// 403 is a rejection too (some gateways answer forbidden for revoked keys).
#[test]
fn a_forbidden_key_is_rejected() {
    let server = spawn_server(json_response("403 Forbidden", r#"{"error":"forbidden"}"#));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-revoked");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("forbidden key");
    assert_eq!(error.code, ProviderErrorKind::AuthRequired);
}

/// Rate limiting is indeterminate, never a key rejection, and carries
/// Retry-After.
#[test]
fn a_rate_limited_probe_is_indeterminate_with_retry_after() {
    let server = spawn_server(json_response(
        "429 Too Many Requests",
        r#"{"error":"rate limited"}"#,
    ));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("rate limited");
    assert_eq!(error.code, ProviderErrorKind::RateLimited);
    assert!(error.retryable);
}

/// A 5xx outage is indeterminate: the refresh fails with the provider
/// unavailable code so the pipeline keeps the last known good values.
#[test]
fn a_server_outage_maps_to_provider_unavailable() {
    let server = spawn_server(json_response("503 Service Unavailable", "{}"));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("server outage");
    assert_eq!(error.code, ProviderErrorKind::ProviderUnavailable);
}

/// A 2xx whose body is not JSON is a structure error, not a healthy probe.
#[test]
fn a_non_json_2xx_body_fails_as_a_parser_error() {
    let server = spawn_server(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 15\r\nConnection: close\r\n\r\n<html>gateway</".to_owned(),
    );
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("malformed body");
    assert_eq!(error.code, ProviderErrorKind::ParserFailed);
}

/// The response-size cap applies to the probe: a body larger than the limit
/// must fail instead of being silently accepted.
#[test]
fn an_oversized_probe_body_is_rejected() {
    // One byte over the 64-byte cap the adapter below is built with.
    let body = format!("\"{}\"", "x".repeat(64));
    let server = spawn_server(json_response("200 OK", &body));
    let config = model_only_config(&server.base_url);
    let adapter = OpenAiCompatibleAdapter::new(
        AdapterLimits {
            timeout: std::time::Duration::from_secs(5),
            max_response_bytes: 64,
        },
        config.clone(),
    )
    .expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("oversized body");
    assert_eq!(error.code, ProviderErrorKind::InvalidResponse);
}

/// A provider with neither quota endpoints nor a models endpoint still
/// refuses to fetch — the probe cannot replace a missing endpoint.
#[test]
fn a_provider_without_any_endpoint_stays_unsupported() {
    let server = spawn_server(json_response("200 OK", "{}"));
    let mut config = model_only_config(&server.base_url);
    config.models_endpoint = None;
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let error = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect_err("no endpoints at all");
    assert_eq!(error.code, ProviderErrorKind::UnsupportedCapability);
}

/// A provider with quota endpoints AND a models endpoint keeps fetching
/// quota surfaces; the models endpoint is not requested on that path.
#[test]
fn quota_bearing_providers_do_not_spend_a_models_request() {
    let server = spawn_router(|path| {
        if path.ends_with("/usage") {
            (
                json_response("200 OK", r#"{"data":{"used":5,"limit":10}}"#),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", r#"{"data":[]}"#), path.to_owned())
        }
    });
    let mut config = model_only_config(&server.base_url);
    config.usage_endpoint = Some("/usage".to_owned());
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect("quota fetch");
    assert!(!result.snapshots.is_empty());
    assert!(
        server
            .requested_paths()
            .iter()
            .all(|path| path.ends_with("/usage")),
        "the models endpoint must not be requested when quota endpoints exist, got {:?}",
        server.requested_paths()
    );
}

/// The probe verdict is directly observable for credential-validation
/// callers: verified / rejected / indeterminate are distinguishable.
#[test]
fn probe_models_exposes_the_verdict_directly() {
    let ok_server = spawn_server(json_response("200 OK", r#"{"data":[{"id":"m1"}]}"#));
    let config = model_only_config(&ok_server.base_url);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let store = store_with_key("sk-fixture-valid");
    let outcome = tauri::async_runtime::block_on(adapter.probe_models(&config, &store))
        .expect("probe request built");
    assert_eq!(outcome, ProbeOutcome::Verified);

    let bad_server = spawn_server(json_response("401 Unauthorized", "{}"));
    let mut bad_config = model_only_config(&bad_server.base_url);
    bad_config.credential_ref = Some("account/probe-fixture-2".to_owned());
    let store = {
        let store = CredentialStore::default();
        store
            .set("account/probe-fixture-2", "sk-revoked")
            .expect("keyring");
        store
    };
    let outcome = tauri::async_runtime::block_on(adapter.probe_models(&bad_config, &store))
        .expect("probe request built");
    assert!(matches!(outcome, ProbeOutcome::Rejected(_)));
}
