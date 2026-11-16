//! Endpoint diagnostics probe every declared endpoint through the real
//! adapter and loopback servers, proving the panel reports per-endpoint
//! outcomes without exposing bodies and requests a shared path only once.

mod common;

use common::{json_response, spawn_router};
use tokenowl_lib::credentials::CredentialStore;
use tokenowl_lib::domain::provider::{EndpointProbe, OpenAiCompatibleConfig};
use tokenowl_lib::providers::builtin::openai::OpenAiCompatibleAdapter;

fn config(base_url: &str) -> OpenAiCompatibleConfig {
    let mut config = OpenAiCompatibleConfig::new("diagnostics", "Diagnostics", base_url);
    config.models_endpoint = Some("/models".to_owned());
    config.quota_endpoint = Some("/usage".to_owned());
    config.usage_endpoint = Some("/usage".to_owned());
    config.balance_endpoint = Some("/balance".to_owned());
    config.reset_endpoint = Some("/reset".to_owned());
    config.credential_ref = Some("account/diagnostics".to_owned());
    config
}

fn probe<'a>(probes: &'a [EndpointProbe], kind: &str) -> &'a EndpointProbe {
    probes
        .iter()
        .find(|probe| probe.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind} probe: {probes:?}"))
}

#[test]
fn every_declared_endpoint_is_classified_and_shared_paths_are_requested_once() {
    let server = spawn_router(|path| match path {
        "/models" => (
            json_response("200 OK", r#"{"data":[{"id":"m"}]}"#),
            path.to_owned(),
        ),
        "/usage" => (
            json_response("401 Unauthorized", r#"{"error":"bad key"}"#),
            path.to_owned(),
        ),
        "/balance" => (json_response("200 OK", "not json"), path.to_owned()),
        "/reset" => (
            json_response("429 Too Many Requests", r#"{"error":"slow down"}"#),
            path.to_owned(),
        ),
        _ => (json_response("404 Not Found", "{}"), path.to_owned()),
    });
    let config = config(&server.base_url);
    config.validate().expect("config must be valid");
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    // Isolate this binary's keyring namespace: integration test processes
    // run in parallel, and the shared default service has shown interference.
    let store = CredentialStore::new(format!(
        "com.tokenowl.test.endpoint-diagnostics.{}",
        std::process::id()
    ));
    store
        .set("account/diagnostics", "sk-fixture")
        .expect("keyring");
    let probes = tauri::async_runtime::block_on(adapter.probe_endpoints(&config, &store))
        .expect("diagnostics");

    assert_eq!(probes.len(), 5, "one row per declared kind");
    assert_eq!(
        probe(&probes, "models").outcome,
        "ok",
        "models probe: {probes:?}"
    );
    assert_eq!(probe(&probes, "models").http_status, Some(200));
    assert_eq!(probe(&probes, "quota").outcome, "auth");
    assert_eq!(probe(&probes, "quota").error_code, None);
    assert_eq!(probe(&probes, "usage").outcome, "auth");
    assert_eq!(probe(&probes, "balance").outcome, "malformed");
    assert_eq!(probe(&probes, "reset").outcome, "rate_limited");
    assert!(
        probes.iter().all(|probe| probe.latency_ms < 10_000),
        "latency is measured, not a sentinel: {probes:?}"
    );

    let mut paths = server.requested_paths();
    paths.sort();
    assert_eq!(
        paths,
        vec!["/balance", "/models", "/reset", "/usage"],
        "the shared /usage path is requested once"
    );
}

#[test]
fn an_unreachable_endpoint_reports_a_network_outcome_instead_of_an_error() {
    let mut config = config("http://127.0.0.1:9");
    config.quota_endpoint = None;
    config.usage_endpoint = None;
    config.balance_endpoint = None;
    config.reset_endpoint = None;
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    // Isolate this binary's keyring namespace: integration test processes
    // run in parallel, and the shared default service has shown interference.
    let store = CredentialStore::new(format!(
        "com.tokenowl.test.endpoint-diagnostics.{}",
        std::process::id()
    ));
    store
        .set("account/diagnostics", "sk-fixture")
        .expect("keyring");
    let probes = tauri::async_runtime::block_on(adapter.probe_endpoints(&config, &store))
        .expect("diagnostics");
    assert_eq!(probes.len(), 1);
    assert!(
        matches!(
            probes[0].outcome.as_str(),
            "network" | "timeout" | "config_error"
        ),
        "unexpected outcome: {:?}",
        probes[0]
    );
}
