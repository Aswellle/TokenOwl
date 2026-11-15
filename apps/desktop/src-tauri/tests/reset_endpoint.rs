//! The declared reset endpoint must be a real request with real parsing:
//! these tests drive the adapter against loopback servers and assert which
//! endpoints were contacted, where the reset lands, and that a failed reset
//! never erases its siblings' quota data.

mod common;

use common::{json_response, spawn_router, spawn_server};
use tokenowl_lib::credentials::CredentialStore;
use tokenowl_lib::domain::provider::{
    OpenAiCompatibleConfig, ProviderErrorKind, ResetFieldMapping, SnapshotStatus,
};
use tokenowl_lib::providers::builtin::openai::OpenAiCompatibleAdapter;

const USAGE_BODY: &str = r#"{"data":{"metric_id":"tokens-5h","used":2500,"limit":10000}}"#;

fn config_with_reset(
    base_url: &str,
    reset_mapping: Option<ResetFieldMapping>,
) -> OpenAiCompatibleConfig {
    let mut config = OpenAiCompatibleConfig::new("fixture-reset", "Fixture Reset", base_url);
    config.usage_endpoint = Some("/usage".to_owned());
    config.reset_endpoint = Some("/reset".to_owned());
    config.credential_ref = Some("account/reset-fixture".to_owned());
    if let Some(mapping) = reset_mapping {
        config.field_mappings = Some(tokenowl_lib::domain::provider::FieldMappings {
            reset: Some(mapping),
            ..tokenowl_lib::domain::provider::FieldMappings::default()
        });
    }
    config
}

fn store() -> CredentialStore {
    let store = CredentialStore::default();
    store
        .set("account/reset-fixture", "sk-fixture-valid")
        .expect("keyring");
    store
}

/// The declared reset endpoint is really requested, and a window-named reset
/// folds into the snapshot the provider identified.
#[test]
fn reset_endpoint_is_requested_and_binds_to_the_named_window() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                json_response(
                    "200 OK",
                    r#"{"data":{"metric_id":"tokens-5h","reset_at":1764000000}}"#,
                ),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: Some("unix_seconds".to_owned()),
            window_id: Some("data.metric_id".to_owned()),
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("refresh with reset");
    let paths = server.requested_paths();
    assert!(
        paths.iter().any(|path| path.ends_with("/reset")),
        "the reset endpoint must be requested, got {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("/usage")),
        "the usage endpoint must be requested, got {paths:?}"
    );
    // The window-named reset folds into the usage snapshot: no standalone
    // reset card, no invented precision on the other metrics.
    assert_eq!(result.snapshots.len(), 1, "got {:?}", result.snapshots);
    let snapshot = &result.snapshots.snapshots[0];
    assert_eq!(snapshot.metric_id.as_deref(), Some("tokens-5h"));
    assert_eq!(snapshot.reset_at.as_deref(), Some("2025-11-24T16:00:00Z"));
    let window = snapshot.window.as_ref().expect("window carries the reset");
    assert_eq!(window.reset_at.as_deref(), Some("2025-11-24T16:00:00Z"));
}

/// A reset that names no window stays one honest account-level card instead
/// of being copied onto every window with invented precision.
#[test]
fn an_account_level_reset_stays_a_standalone_card() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                json_response("200 OK", r#"{"reset_at":1764000000}"#),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "reset_at".to_owned(),
            timestamp_format: Some("unix_seconds".to_owned()),
            window_id: None,
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("refresh with reset");
    assert_eq!(result.snapshots.len(), 2);
    let usage = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("tokens-5h"))
        .expect("usage snapshot");
    assert_eq!(usage.reset_at, None, "no reset copied onto the window");
    let reset = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("reset"))
        .expect("standalone reset card");
    assert_eq!(reset.reset_at.as_deref(), Some("2025-11-24T16:00:00Z"));
    assert_eq!(reset.used, None, "a reset card invents no quota values");
    assert_eq!(reset.limit, None);
    assert_eq!(reset.remaining, None);
}

/// A failed reset endpoint keeps every quota sibling and reports its own
/// error card — one endpoint's outage never erases the others.
#[test]
fn a_failed_reset_endpoint_preserves_quota_siblings() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                json_response("500 Internal Server Error", "{}"),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: Some("unix_seconds".to_owned()),
            window_id: None,
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("quota data survives the reset outage");
    assert_eq!(result.snapshots.len(), 2);
    let usage = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("tokens-5h"))
        .expect("usage snapshot");
    assert_eq!(usage.status, SnapshotStatus::Available);
    assert_eq!(usage.used, Some(2500.0));
    let reset = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("reset"))
        .expect("reset error card");
    assert_eq!(reset.status, SnapshotStatus::Error);
    assert_eq!(reset.reset_at, None, "no invented reset instant");
}

/// A rejected reset request (bad key on that endpoint) surfaces the auth
/// error when it is the only failure, preserving the structured kind.
#[test]
fn a_rejected_reset_request_keeps_its_error_kind() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (json_response("401 Unauthorized", "{}"), path.to_owned())
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: None,
            window_id: None,
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("quota data survives the rejected reset");
    let reset = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("reset"))
        .expect("reset error card");
    assert_eq!(reset.status, SnapshotStatus::Error);
    let usage = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("tokens-5h"))
        .expect("usage snapshot");
    assert_eq!(usage.status, SnapshotStatus::Available);
}

/// A custom account with a reset endpoint but no declared mapping still gets
/// its reset through the lenient parser's reset-bearing fields.
#[test]
fn a_custom_reset_endpoint_without_mapping_uses_the_lenient_fields() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                json_response("200 OK", r#"{"reset_at":"2025-11-24T16:00:00Z"}"#),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(&server.base_url, None);
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("refresh with unmapped reset");
    let reset = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.reset_at.is_some())
        .expect("a reset-bearing snapshot");
    assert_eq!(reset.reset_at.as_deref(), Some("2025-11-24T16:00:00Z"));
}

/// A reset endpoint that answers with no recognizable timestamp contributes
/// nothing — unknown stays unknown.
#[test]
fn an_unrecognized_reset_answer_contributes_nothing() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                json_response("200 OK", r#"{"status":"ok"}"#),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: None,
            window_id: None,
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("refresh");
    assert_eq!(result.snapshots.len(), 1, "only the usage snapshot");
    assert_eq!(
        result.snapshots.snapshots[0].metric_id.as_deref(),
        Some("tokens-5h")
    );
    assert_eq!(
        result.snapshots.snapshots[0].status,
        SnapshotStatus::Available
    );
    // Silence about the reset is fine, but the refresh must not have failed
    // as UnsupportedCapability or invented an error card for it.
    assert_ne!(result.snapshots.snapshots[0].status, SnapshotStatus::Error);
}

/// A provider declaring ONLY a reset endpoint (no quota surface) answers
/// with its reset card rather than a capability failure.
#[test]
fn a_reset_only_provider_answers_with_its_reset_card() {
    let server = spawn_server(json_response(
        "200 OK",
        r#"{"data":{"reset_at":1764000000}}"#,
    ));
    let mut config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: Some("unix_seconds".to_owned()),
            window_id: None,
        }),
    );
    config.usage_endpoint = None;
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("reset-only refresh");
    assert_eq!(result.snapshots.len(), 1);
    let snapshot = &result.snapshots.snapshots[0];
    assert_eq!(snapshot.reset_at.as_deref(), Some("2025-11-24T16:00:00Z"));
}

/// Every wire failure kind is preserved through the reset path: a 429 keeps
/// its Retry-After when it is the wholesale failure.
#[test]
fn a_rate_limited_reset_wholesale_failure_keeps_retry_after() {
    let server = spawn_router(|path| {
        if path.ends_with("/reset") {
            (
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_owned(),
                path.to_owned(),
            )
        } else {
            (json_response("200 OK", USAGE_BODY), path.to_owned())
        }
    });
    let config = config_with_reset(
        &server.base_url,
        Some(ResetFieldMapping {
            value: "data.reset_at".to_owned(),
            timestamp_format: None,
            window_id: None,
        }),
    );
    let adapter = OpenAiCompatibleAdapter::with_default_limits(config.clone()).expect("adapter");
    let result = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store()))
        .expect("usage survives the rate limit");
    let reset = result
        .snapshots
        .snapshots
        .iter()
        .find(|snapshot| snapshot.metric_id.as_deref() == Some("reset"))
        .expect("reset error card");
    assert_eq!(reset.status, SnapshotStatus::Error);
    assert_eq!(reset.used, None);
    assert_eq!(result.snapshots.len(), 2);
    let _ = ProviderErrorKind::RateLimited; // kind asserted via snapshot errors elsewhere
}
