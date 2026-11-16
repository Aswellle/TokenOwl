//! Key rotation must never destroy the working key. These tests drive the
//! real rotation flow (`rotate_account_credential`) against loopback servers
//! and the OS credential store: a verified candidate replaces the stable
//! reference, and every failure path — rejection, rate limit, outage, store
//! failure — leaves the previous key exactly in place.
//!
//! Every test runs under its own keyring service namespace and its own
//! database so parallel tests cannot observe each other's secrets.

#![cfg(any(target_os = "windows", target_os = "macos"))]

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{json_response, spawn_server};
use tokenowl_lib::commands::{
    AppRuntime, rotate_account_credential, rotate_account_credential_with,
};
use tokenowl_lib::credentials::{CredentialError, CredentialStore};
use tokenowl_lib::domain::provider::OpenAiCompatibleConfig;
use tokenowl_lib::domain::state::AccountMetadata;
use tokenowl_lib::storage::LocalRepository;

/// One test's isolated fixture: unique keyring service, unique database,
/// and the shared-store lock held for the test's whole body.
struct Fixture {
    runtime: Arc<AppRuntime>,
    store: CredentialStore,
    reference: String,
    db: PathBuf,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// Serializes every keyring-touching test. The Windows credential store is
/// a shared OS resource: rapid CredWrite/CredRead from parallel test threads
/// transiently reports fresh entries as missing, so each test holds this
/// lock for its whole body — the same way it would serialize on a database.
static CREDENTIAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Fixture {
    fn build(tag: &str) -> Self {
        let guard = CREDENTIAL_LOCK.lock().expect("credential lock");
        let db = temp_db(tag);
        let store = CredentialStore::new(format!(
            "com.tokenowl.rotation.{tag}.{}",
            std::process::id()
        ));
        let runtime = Arc::new(AppRuntime::new_with_service(&db, store.clone()).expect("runtime"));
        Self {
            runtime,
            store,
            reference: format!("account/rotation-{tag}"),
            db,
            _guard: guard,
        }
    }
}

/// A per-test database path the previous run's sidecars cannot survive.
fn temp_db(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tokenowl-rotation-{tag}-{}.sqlite3",
        std::process::id()
    ));
    for suffix in ["", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
    path
}

fn model_only_provider(base_url: &str, credential_ref: &str) -> OpenAiCompatibleConfig {
    let mut config =
        OpenAiCompatibleConfig::new("rotation-fixture:account-1", "Rotation Fixture", base_url);
    config.models_endpoint = Some("/models".to_owned());
    config.quota_endpoint = None;
    config.usage_endpoint = None;
    config.balance_endpoint = None;
    config.credential_ref = Some(credential_ref.to_owned());
    config
}

fn seed_account(repository: &LocalRepository, config: &OpenAiCompatibleConfig, label: &str) {
    repository.upsert_provider(config, 1).expect("provider row");
    let account = AccountMetadata::from_auth_input(
        "account-1",
        config.id.clone(),
        label,
        "api_key",
        config.credential_ref.clone(),
        std::collections::HashMap::new(),
        true,
        None,
    );
    repository.upsert_account(&account, 1).expect("account row");
}

fn resolved(store: &CredentialStore, reference: &str) -> Result<String, CredentialError> {
    store
        .resolve(reference)
        .map(|value| value.as_str().to_owned())
}

/// A verified candidate is committed under the stable reference and the old
/// value is gone. The one-time candidate reference leaves nothing behind and
/// queues no cleanup entry.
#[test]
fn a_verified_candidate_replaces_the_stable_reference() {
    let fixture = Fixture::build("verified");
    let server = spawn_server(json_response("200 OK", r#"{"data":[{"id":"m1"}]}"#));
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    fixture
        .store
        .set(&fixture.reference, "sk-old")
        .expect("old key");

    let result = tauri::async_runtime::block_on(rotate_account_credential(
        &fixture.runtime,
        None,
        "account-1",
        "sk-new-valid",
    ))
    .expect("rotation completes");

    assert_eq!(result.outcome, "validated_and_saved");
    assert!(result.account.is_some(), "the refreshed view rides along");
    assert_eq!(
        resolved(&fixture.store, &fixture.reference).expect("stable key"),
        "sk-new-valid",
        "the stable reference must carry the candidate after verification"
    );
    // The cleanup queue holds no entries: the candidate's pending record was
    // cleared when its deletion succeeded.
    let repository = LocalRepository::open(&fixture.db).expect("database");
    let drained = match repository
        .get_setting(tokenowl_lib::commands::CLEANUP_SETTING_KEY)
        .expect("queue setting")
    {
        None => true,
        Some(value) => value.as_array().is_some_and(std::vec::Vec::is_empty),
    };
    assert!(
        drained,
        "a successful rotation must leave no cleanup entries"
    );
}

/// A refused candidate never touches the stable reference.
#[test]
fn a_rejected_candidate_leaves_the_old_key_in_place() {
    let fixture = Fixture::build("rejected");
    let server = spawn_server(json_response(
        "401 Unauthorized",
        r#"{"error":"invalid api key"}"#,
    ));
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    fixture
        .store
        .set(&fixture.reference, "sk-old-good")
        .expect("old key");

    let result = tauri::async_runtime::block_on(rotate_account_credential(
        &fixture.runtime,
        None,
        "account-1",
        "sk-revoked",
    ))
    .expect("rotation completes with a verdict");

    assert_eq!(result.outcome, "rejected");
    assert_eq!(result.error_code.as_deref(), Some("AuthRequired"));
    assert_eq!(
        resolved(&fixture.store, &fixture.reference).expect("old key survives"),
        "sk-old-good",
        "a rejected candidate must not overwrite the working key"
    );
    // The candidate was deleted after the probe, so its pending cleanup
    // record is gone with it.
    let repository = LocalRepository::open(&fixture.db).expect("database");
    let drained = match repository
        .get_setting(tokenowl_lib::commands::CLEANUP_SETTING_KEY)
        .expect("queue setting")
    {
        None => true,
        Some(value) => value.as_array().is_some_and(std::vec::Vec::is_empty),
    };
    assert!(drained, "a rejected rotation must leave no cleanup entries");
}

/// A rotation interrupted after the candidate was written leaves an orphaned
/// keyring entry behind. The startup sweep deletes it and drains the queued
/// cleanup: this simulates the crash window between the candidate write and
/// its deletion, where no in-process code ever runs again.
#[test]
fn an_interrupted_rotation_cleans_up_at_the_next_startup() {
    let fixture = Fixture::build("crashed");
    let candidate_ref = format!("{}/candidate/{}", fixture.reference, "crashnonce");
    // The crash left the candidate in the keyring and its deletion queued.
    fixture
        .store
        .set(&candidate_ref, "sk-crashed-candidate")
        .expect("candidate entry");
    let repository = LocalRepository::open(&fixture.db).expect("database");
    repository
        .set_setting(
            tokenowl_lib::commands::CLEANUP_SETTING_KEY,
            &serde_json::json!([{
                "credentialRef": candidate_ref,
                "errorCode": "CandidateProbe",
                "recordedAt": 1_800_000_000u64,
            }]),
        )
        .expect("queue entry");

    fixture.runtime.run_startup_cleanup();

    assert!(
        resolved(&fixture.store, &candidate_ref).is_err(),
        "the orphaned candidate entry must be deleted by the sweep"
    );
    let drained = match repository
        .get_setting(tokenowl_lib::commands::CLEANUP_SETTING_KEY)
        .expect("queue setting")
    {
        None => true,
        Some(value) => value.as_array().is_some_and(std::vec::Vec::is_empty),
    };
    assert!(drained, "the queue must drain once the reference is gone");
}

/// A rate-limited probe is indeterminate, not a rejection: the old key
/// stays and the verdict names the retry hint.
#[test]
fn a_rate_limited_probe_is_indeterminate_and_keeps_the_old_key() {
    let fixture = Fixture::build("ratelimited");
    let server = spawn_server(
        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_owned(),
    );
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    fixture
        .store
        .set(&fixture.reference, "sk-old-good")
        .expect("old key");

    let result = tauri::async_runtime::block_on(rotate_account_credential(
        &fixture.runtime,
        None,
        "account-1",
        "sk-candidate",
    ))
    .expect("rotation completes with a verdict");

    assert_eq!(result.outcome, "validation_indeterminate");
    assert_eq!(result.error_code.as_deref(), Some("RateLimited"));
    assert_eq!(
        result.retry_at.as_deref(),
        Some("30"),
        "Retry-After rides along"
    );
    assert_eq!(
        resolved(&fixture.store, &fixture.reference).expect("old key survives"),
        "sk-old-good"
    );
}

/// A server outage is indeterminate too: nothing about the key was proven.
#[test]
fn a_server_outage_is_indeterminate_and_keeps_the_old_key() {
    let fixture = Fixture::build("outage");
    let server = spawn_server(json_response("503 Service Unavailable", "{}"));
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    fixture
        .store
        .set(&fixture.reference, "sk-old-good")
        .expect("old key");

    let result = tauri::async_runtime::block_on(rotate_account_credential(
        &fixture.runtime,
        None,
        "account-1",
        "sk-candidate",
    ))
    .expect("rotation completes with a verdict");

    assert_eq!(result.outcome, "validation_indeterminate");
    assert_eq!(result.error_code.as_deref(), Some("ProviderUnavailable"));
    assert_eq!(
        resolved(&fixture.store, &fixture.reference).expect("old key survives"),
        "sk-old-good"
    );
}

/// A credential-store failure while writing the candidate ends the rotation
/// before any request is spent, and the old key is untouched.
#[test]
fn a_candidate_store_failure_never_reaches_the_network() {
    let fixture = Fixture::build("storefail");
    let server = spawn_server(json_response("200 OK", r#"{"data":[]}"#));
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    // A candidate write that fails (any keyring outage behaves the same)
    // must end the rotation before a request is spent.
    let result = tauri::async_runtime::block_on(rotate_account_credential_with(
        &fixture.runtime,
        None,
        "account-1",
        "sk-candidate",
        |_reference, _secret| Err(CredentialError::StoreUnavailable),
    ))
    .expect("rotation completes with a verdict");

    assert_eq!(result.outcome, "storage_failed");
    assert!(
        result.error_code.is_some(),
        "the store failure carries its stable code"
    );
    assert_eq!(
        server.requested_paths().len(),
        0,
        "no network request may follow a candidate store failure"
    );
}

/// The rotation refuses to run at all for a blank key: nothing is written,
/// nothing is probed.
#[test]
fn a_blank_key_is_refused_before_anything_is_written() {
    let fixture = Fixture::build("blank");
    let server = spawn_server(json_response("200 OK", r#"{"data":[]}"#));
    let config = model_only_provider(&server.base_url, &fixture.reference);
    {
        let repository = LocalRepository::open(&fixture.db).expect("database");
        seed_account(&repository, &config, "Rotation");
    }
    let error = tauri::async_runtime::block_on(rotate_account_credential(
        &fixture.runtime,
        None,
        "account-1",
        "   ",
    ))
    .expect_err("blank keys are command errors");
    assert!(matches!(
        error,
        tokenowl_lib::commands::CommandError::InvalidInput(_)
    ));
    assert_eq!(
        server.requested_paths().len(),
        0,
        "no probe for a blank key"
    );
}
