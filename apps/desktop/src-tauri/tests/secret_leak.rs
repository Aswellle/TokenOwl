//! Secret-leak regression sweep (docs/06, gap matrix: OS credential store).
//!
//! The OS credential store itself is deliberately absent here — these tests
//! lock the boundary around it: a raw secret must never surface in
//! `SQLite` bytes, serialized DTOs, account metadata, or telemetry output,
//! whatever the credential-ref plumbing does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tokenowl_lib::domain::provider::{AuthMethod, OpenAiCompatibleConfig};
use tokenowl_lib::domain::state::AccountMetadata;
use tokenowl_lib::storage::LocalRepository;
use tokenowl_lib::telemetry::redact_value;

const SECRET: &str = "sk-tokenowl-regression-9f4e2d1c7b5aSECRET";
const CREDENTIAL_REF: &str = "account/leak-probe";

fn temp_db_path(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tokenowl-secret-sweep-{tag}-{}.sqlite3",
        std::process::id()
    ));
    for suffix in ["", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
    path
}

fn db_artifacts(path: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| {
            let mut os = path.as_os_str().to_os_string();
            os.push(suffix);
            PathBuf::from(os)
        })
        .filter(|candidate| candidate.exists())
        .collect()
}

fn probe_config() -> OpenAiCompatibleConfig {
    let mut config = OpenAiCompatibleConfig::new(
        "openai:account-leak-probe",
        "Leak Probe",
        "https://api.openai.com/v1",
    );
    config.credential_ref = Some(CREDENTIAL_REF.to_owned());
    config.auth_method = AuthMethod::ApiKey;
    config
}

/// Builds the account the way `persist_new_account` does — the secret is
/// handed to `from_auth_input` (which must drop it) and to the metadata map
/// (which the sanitizer must strip), while only the ref is retained.
fn probe_account() -> AccountMetadata {
    AccountMetadata::from_auth_input(
        "account-leak-probe",
        "openai:account-leak-probe",
        "Leak Probe",
        "api_key",
        Some(CREDENTIAL_REF.to_owned()),
        HashMap::from([
            ("api_key".to_owned(), SECRET.to_owned()),
            ("note".to_owned(), "safe note".to_owned()),
        ]),
        true,
        Some(SECRET),
    )
}

#[test]
fn sqlite_never_persists_the_raw_secret() {
    let path = temp_db_path("main");
    {
        let repo = LocalRepository::open(&path).expect("open repository");
        repo.upsert_provider(&probe_config(), 1_000)
            .expect("provider upsert");
        repo.upsert_account(&probe_account(), 1_000)
            .expect("account upsert");
    }

    let mut saw_credential_ref = false;
    for file in db_artifacts(&path) {
        let bytes = std::fs::read(&file).expect("read db artifact");
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SECRET),
            "raw secret leaked into {}",
            file.display()
        );
        saw_credential_ref |= text.contains(CREDENTIAL_REF);
    }
    // Positive control: the ref must be present, otherwise this sweep would
    // prove nothing about what the database actually stores.
    assert!(
        saw_credential_ref,
        "credential ref missing from the database artifacts"
    );

    for file in db_artifacts(&path) {
        let _ = std::fs::remove_file(file);
    }
}

#[test]
fn metadata_sanitizer_drops_secret_shaped_keys() {
    let metadata = probe_account().metadata;
    assert!(
        !metadata.contains_key("api_key"),
        "api_key survived sanitizing"
    );
    assert_eq!(metadata.get("note").map(String::as_str), Some("safe note"));
}

#[test]
fn dto_serialization_never_carries_the_secret() {
    let metadata = probe_account();
    let account_json = serde_json::to_string(&metadata).expect("account json");
    assert!(!account_json.contains(SECRET));
    assert!(!format!("{metadata:?}").contains(SECRET));

    let config = probe_config();
    let config_json = serde_json::to_string(&config).expect("config json");
    assert!(!config_json.contains(SECRET));
    assert!(!format!("{config:?}").contains(SECRET));
}

#[test]
fn read_back_account_keeps_only_the_ref() {
    let repo = LocalRepository::open_in_memory().expect("open in-memory repository");
    repo.upsert_provider(&probe_config(), 1_000)
        .expect("provider upsert");
    repo.upsert_account(&probe_account(), 1_000)
        .expect("account upsert");

    let stored = repo
        .account("account-leak-probe")
        .expect("account query")
        .expect("account present");
    assert_eq!(stored.credential_ref.as_deref(), Some(CREDENTIAL_REF));
    assert!(!stored.metadata.contains_key("api_key"));
    let json = serde_json::to_string(&stored).expect("stored account json");
    assert!(!json.contains(SECRET));
}

#[test]
fn telemetry_masks_key_shaped_secrets() {
    assert_eq!(redact_value("api_key", SECRET), "***");
    assert_eq!(redact_value("apiKey", SECRET), "***");
    assert_eq!(
        redact_value("authorization", &format!("Bearer {SECRET}")),
        "***"
    );
    assert_eq!(redact_value("token", SECRET), "***");
    // Canonical-form variants the normalizer must collapse (R-23): a key
    // spelled `auth_token`, `api_token`, `client_secret`, or `cookie` is the
    // same secret-bearing field after punctuation is stripped.
    assert_eq!(redact_value("auth_token", SECRET), "***");
    assert_eq!(redact_value("api-token", SECRET), "***");
    assert_eq!(redact_value("sessionCookie", SECRET), "***");
    assert_eq!(redact_value("client_secret", SECRET), "***");
    assert_eq!(redact_value("cookie", SECRET), "***");
    // Non-sensitive fields pass through untouched.
    assert_eq!(redact_value("provider", "openai"), "openai");
    assert_eq!(redact_value("token_type", "bearer"), "bearer");
}

/// The IPC input DTO carries the raw API key; its `Debug` output (error
/// context, logs) must never print it (R-22).
#[test]
fn account_input_debug_never_prints_the_api_key() {
    let input = tokenowl_lib::commands::GenericOpenAiAccountInput {
        provider_id: "generic.openai_compatible".to_owned(),
        label: "Leak Probe".to_owned(),
        base_url: "https://api.openai.com/v1".to_owned(),
        api_key: SECRET.to_owned(),
        quota_endpoint: None,
        usage_endpoint: None,
        balance_endpoint: None,
        lan_consent_token: Some("consent-token".to_owned()),
    };
    let rendered = format!("{input:?}");
    assert!(
        !rendered.contains(SECRET),
        "Debug leaked the key: {rendered}"
    );
    assert!(
        rendered.contains("redacted"),
        "api_key must be visibly redacted"
    );
    assert!(
        !rendered.contains("consent-token"),
        "token must not print either"
    );
    // Positive control: non-secret fields stay visible so the impl is a
    // redaction, not a blank.
    assert!(rendered.contains("generic.openai_compatible"));
}

/// The transport response body is raw provider payload; its `Debug` output
/// must carry only status and size metadata (R-22).
#[test]
fn provider_response_debug_never_prints_the_body() {
    let response = tokenowl_lib::domain::provider::ProviderResponse::new(
        200,
        format!(r#"{{"api_key": "{SECRET}"}}"#),
    );
    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains(SECRET),
        "Debug leaked the body: {rendered}"
    );
    assert!(rendered.contains("200"), "status remains visible");
}
