//! Secret-free account-configuration backup and import preview.
//!
//! Export writes only what the user typed and the manifest declared: provider
//! id, label, base URL, wire format, and endpoint paths. It never writes an
//! API key, a credential reference, account ids, metadata, headers, cookies,
//! or keyring material. Import is a preview only — the UI then creates each
//! entry through the normal `accounts_create` path after the user re-enters a
//! key, so first-connect verification and LAN consent still apply.

use super::{AppRuntime, Arc, CommandError, State, unix_seconds};
use serde::{Deserialize, Serialize};

/// Version of the backup document written by [`accounts_export_config`].
pub const ACCOUNT_EXPORT_SCHEMA_VERSION: u32 = 1;
/// Import payloads larger than this are refused before JSON parsing.
const MAX_IMPORT_BYTES: usize = 256 * 1024;
/// One backup may carry at most this many accounts; a config file is not a
/// bulk provisioning channel.
const MAX_IMPORT_ENTRIES: usize = 50;

fn default_api_format() -> String {
    "openai_compatible".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountConfigFile {
    pub schema_version: u32,
    pub exported_at: u64,
    pub accounts: Vec<AccountConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountConfigEntry {
    /// Manifest/profile id (`deepseek`, `litellm`) or
    /// `generic.openai_compatible`. Never the account-scoped provider id.
    pub provider_id: String,
    pub label: String,
    pub base_url: String,
    #[serde(default = "default_api_format")]
    pub api_format: String,
    #[serde(default)]
    pub quota_endpoint: Option<String>,
    #[serde(default)]
    pub usage_endpoint: Option<String>,
    #[serde(default)]
    pub balance_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfigExportResult {
    pub path: String,
    pub accounts: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImportEntryView {
    pub index: usize,
    pub provider_id: String,
    pub provider_name: String,
    pub label: String,
    pub base_url: String,
    pub api_format: String,
    pub quota_endpoint: Option<String>,
    pub usage_endpoint: Option<String>,
    pub balance_endpoint: Option<String>,
    /// False when the entry cannot be re-created as written; `reason` says
    /// which field failed. An importable entry still needs a new API key.
    pub importable: bool,
    pub reason: Option<String>,
    /// The same provider/label/base URL already exists; importing again is
    /// allowed but the UI warns.
    pub already_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImportPreview {
    pub schema_version: u32,
    pub exported_at: u64,
    pub entries: Vec<AccountImportEntryView>,
}

/// The config vocabulary's wire format normalized to the account-input
/// vocabulary (`anthropic`, `gemini`, `openai_compatible`).
fn input_api_format(config_format: Option<&str>) -> String {
    match config_format {
        Some("anthropic") => "anthropic".to_owned(),
        Some("google-ai") => "gemini".to_owned(),
        _ => "openai_compatible".to_owned(),
    }
}

/// Provider id prefix for an account: the manifest id, or the generic
/// custom-provider id. The account-scoped suffix is never exported.
fn provider_prefix(provider_id: &str) -> &str {
    provider_id.split(':').next().unwrap_or(provider_id)
}

fn export_config(
    runtime: &AppRuntime,
    account_ids: Option<&[String]>,
) -> Result<AccountConfigExportResult, CommandError> {
    let repository = runtime.repository()?;
    let wanted = account_ids.map(|ids| {
        ids.iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>()
    });
    let mut entries = Vec::new();
    for account in repository.accounts()? {
        if let Some(wanted) = &wanted
            && !wanted.contains(account.id.as_str())
        {
            continue;
        }
        let Some(provider) = repository.provider(&account.provider_id)? else {
            continue;
        };
        entries.push(AccountConfigEntry {
            provider_id: provider_prefix(&account.provider_id).to_owned(),
            label: account.label,
            base_url: provider.base_url,
            api_format: input_api_format(provider.api_format.as_deref()),
            quota_endpoint: provider.quota_endpoint,
            usage_endpoint: provider.usage_endpoint,
            balance_endpoint: provider.balance_endpoint,
        });
    }
    entries.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });
    let file = AccountConfigFile {
        schema_version: ACCOUNT_EXPORT_SCHEMA_VERSION,
        exported_at: unix_seconds(),
        accounts: entries,
    };
    let body = serde_json::to_string_pretty(&file).map_err(|error| {
        CommandError::InvalidInput(format!("cannot encode the backup: {error}"))
    })?;
    let path = super::exports::write_export(&runtime.data_dir, "accounts", "json", &body)?;
    Ok(AccountConfigExportResult {
        path: path.to_string_lossy().into_owned(),
        accounts: file.accounts.len(),
    })
}

#[tauri::command]
pub fn accounts_export_config(
    state: State<'_, Arc<AppRuntime>>,
    account_ids: Option<Vec<String>>,
) -> Result<AccountConfigExportResult, CommandError> {
    export_config(&state, account_ids.as_deref())
}

fn validate_import_entry(
    entry: &AccountConfigEntry,
    entry_index: usize,
    data_dir: &std::path::Path,
    existing: &[crate::domain::state::AccountMetadata],
    repository: &crate::storage::LocalRepository,
) -> AccountImportEntryView {
    let mut reason: Option<String> = None;
    let mut provider_name = entry.provider_id.clone();
    let mut importable = true;
    // What the create path will actually persist. For a built-in provider
    // the manifest is authoritative for the base URL and every endpoint —
    // client-supplied values are ignored there — so the preview shows the
    // effective values instead of ones that would be silently dropped.
    let mut base_url = entry.base_url.clone();
    let mut quota = entry.quota_endpoint.clone();
    let mut usage = entry.usage_endpoint.clone();
    let mut balance = entry.balance_endpoint.clone();
    let label = entry.label.trim();
    if label.is_empty() || label.chars().count() > 64 {
        importable = false;
        reason = Some("label must be 1-64 characters".to_owned());
    }
    if entry.provider_id == "generic.openai_compatible" {
        if let Err(error) = super::accounts::validate_user_base_url(&entry.base_url) {
            importable = false;
            reason = Some(error.to_string());
        }
        if !matches!(
            entry.api_format.as_str(),
            "openai_compatible" | "anthropic" | "gemini"
        ) {
            importable = false;
            reason = Some(format!(
                "unsupported apiFormat `{}` (openai_compatible, anthropic, gemini)",
                entry.api_format
            ));
        }
    } else {
        let (profiles, _) = crate::providers::builtin::all_manifest_profiles(Some(data_dir));
        if let Some(profile) = profiles
            .iter()
            .find(|profile| profile.id == entry.provider_id)
        {
            provider_name.clone_from(&profile.name);
            quota.clone_from(&profile.endpoints.quota);
            usage.clone_from(&profile.endpoints.usage);
            balance.clone_from(&profile.endpoints.balance);
            if profile.custom_base_url {
                // A gateway has no fixed host: the entry's URL is the only
                // real host and is what create will use.
                if let Err(error) = super::accounts::validate_user_base_url(&entry.base_url) {
                    importable = false;
                    reason = Some(error.to_string());
                }
            } else {
                base_url.clone_from(&profile.base_url);
            }
        } else {
            importable = false;
            reason = Some(format!("unknown provider id `{}`", entry.provider_id));
        }
    }
    let already_exists = existing.iter().any(|account| {
        provider_prefix(&account.provider_id) == entry.provider_id
            && account.label == label
            && repository
                .provider(&account.provider_id)
                .ok()
                .flatten()
                .is_some_and(|provider| provider.base_url == entry.base_url)
    });
    AccountImportEntryView {
        index: entry_index,
        provider_id: entry.provider_id.clone(),
        provider_name,
        label: label.to_owned(),
        base_url,
        api_format: entry.api_format.clone(),
        quota_endpoint: quota,
        usage_endpoint: usage,
        balance_endpoint: balance,
        importable,
        reason,
        already_exists,
    }
}

/// Validates a pasted backup document and returns one preview row per entry.
/// No account is created here: the UI asks for a fresh API key per row and
/// calls the normal create command, which validates the connection.
fn import_preview(
    runtime: &AppRuntime,
    payload: &str,
) -> Result<AccountImportPreview, CommandError> {
    if payload.len() > MAX_IMPORT_BYTES {
        return Err(CommandError::InvalidInput(
            "backup exceeds the 256 KiB import limit".to_owned(),
        ));
    }
    let file: AccountConfigFile = serde_json::from_str(payload).map_err(|error| {
        CommandError::InvalidInput(format!("backup is not a valid account export: {error}"))
    })?;
    if file.schema_version != ACCOUNT_EXPORT_SCHEMA_VERSION {
        return Err(CommandError::InvalidInput(format!(
            "unsupported backup schemaVersion {} (this build supports {ACCOUNT_EXPORT_SCHEMA_VERSION})",
            file.schema_version
        )));
    }
    if file.accounts.len() > MAX_IMPORT_ENTRIES {
        return Err(CommandError::InvalidInput(format!(
            "backup contains more than {MAX_IMPORT_ENTRIES} accounts"
        )));
    }
    let repository = runtime.repository()?;
    let existing = repository.accounts()?;
    let entries = file
        .accounts
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            validate_import_entry(entry, index, &runtime.data_dir, &existing, &repository)
        })
        .collect();
    Ok(AccountImportPreview {
        schema_version: file.schema_version,
        exported_at: file.exported_at,
        entries,
    })
}

#[tauri::command]
pub fn accounts_import_preview(
    payload: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<AccountImportPreview, CommandError> {
    import_preview(&state, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::{AuthMethod, OpenAiCompatibleConfig};
    use crate::domain::state::AccountMetadata;
    use std::collections::HashMap;

    fn runtime(name: &str) -> (AppRuntime, std::path::PathBuf) {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "tokenowl-backup-{name}-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let runtime = AppRuntime::new(dir.join("tokenowl.sqlite3")).expect("runtime");
        (runtime, dir)
    }

    fn seed_account(
        runtime: &AppRuntime,
        provider_id: &str,
        label: &str,
        base_url: &str,
        api_format: Option<&str>,
        credential_ref: Option<&str>,
    ) {
        let repository = runtime.repository().expect("db");
        let mut config = OpenAiCompatibleConfig::new(provider_id, "Test Provider", base_url);
        config.auth_method = AuthMethod::ApiKey;
        config.api_format = api_format.map(str::to_owned);
        config.credential_ref = credential_ref.map(str::to_owned);
        config.quota_endpoint = Some("/usage".to_owned());
        repository.upsert_provider(&config, 1).expect("provider");
        let account = AccountMetadata::from_auth_input(
            "account-1",
            provider_id,
            label,
            "api_key",
            credential_ref.map(str::to_owned),
            HashMap::new(),
            true,
            None,
        );
        repository.upsert_account(&account, 1).expect("account");
    }

    fn preview(runtime: &AppRuntime, payload: &str) -> AccountImportPreview {
        import_preview(runtime, payload).expect("preview")
    }

    fn export(runtime: &AppRuntime) -> AccountConfigExportResult {
        export_config(runtime, None).expect("export")
    }

    #[test]
    fn export_omits_every_secret_and_credential_reference() {
        let (runtime, dir) = runtime("export");
        seed_account(
            &runtime,
            "deepseek:account-1",
            "Work",
            "https://api.deepseek.com/v1",
            None,
            Some("account/account-1"),
        );
        let result = export(&runtime);
        assert_eq!(result.accounts, 1);
        let body = std::fs::read_to_string(&result.path).expect("read export");
        assert!(!body.contains("account/account-1"), "credential ref leaked");
        assert!(!body.contains("credential"), "credential field leaked");
        assert!(!body.contains("account-1"), "account id leaked");
        assert!(!body.to_lowercase().contains("apikey"));
        let file: AccountConfigFile = serde_json::from_str(&body).expect("parse");
        assert_eq!(file.accounts[0].provider_id, "deepseek");
        assert_eq!(file.accounts[0].label, "Work");
        assert_eq!(file.accounts[0].base_url, "https://api.deepseek.com/v1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_maps_the_native_wire_format_to_the_input_vocabulary() {
        let (runtime, dir) = runtime("format");
        seed_account(
            &runtime,
            "gemini:account-1",
            "Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
            Some("google-ai"),
            None,
        );
        let result = export(&runtime);
        let body = std::fs::read_to_string(&result.path).expect("read");
        assert!(body.contains("\"apiFormat\": \"gemini\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_validates_custom_and_unknown_entries_without_creating() {
        let (runtime, dir) = runtime("preview");
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 1_800_000_000u64,
            "accounts": [
                {
                    "providerId": "generic.openai_compatible",
                    "label": "Custom",
                    "baseUrl": "https://relay.example.test/v1",
                    "apiFormat": "openai_compatible",
                    "usageEndpoint": "/usage"
                },
                {
                    "providerId": "does-not-exist",
                    "label": "Unknown",
                    "baseUrl": "https://unknown.test/v1"
                },
                {
                    "providerId": "generic.openai_compatible",
                    "label": "Bad URL",
                    "baseUrl": "ftp://bad.test/v1"
                }
            ]
        })
        .to_string();
        let view = preview(&runtime, &payload);
        assert_eq!(view.entries.len(), 3);
        assert!(view.entries[0].importable);
        assert!(!view.entries[1].importable);
        assert!(
            view.entries[1]
                .reason
                .as_deref()
                .unwrap()
                .contains("unknown provider")
        );
        assert!(!view.entries[2].importable);
        assert_eq!(
            runtime
                .repository()
                .expect("db")
                .accounts()
                .expect("accounts")
                .len(),
            0,
            "preview never creates accounts"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_rejects_future_versions_duplicates_and_extra_fields() {
        let (runtime, dir) = runtime("reject");
        let future = serde_json::json!({
            "schemaVersion": 2,
            "exportedAt": 1,
            "accounts": []
        })
        .to_string();
        assert!(import_preview(&runtime, &future).is_err());

        let extra = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 1,
            "accounts": [{
                "providerId": "generic.openai_compatible",
                "label": "X",
                "baseUrl": "https://x.test/v1",
                "apiKey": "must-not-be-here"
            }]
        })
        .to_string();
        assert!(
            import_preview(&runtime, &extra).is_err(),
            "unknown fields are rejected, not ignored"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_marks_an_existing_provider_label_and_url() {
        let (runtime, dir) = runtime("existing");
        seed_account(
            &runtime,
            "deepseek:account-1",
            "Work",
            "https://api.deepseek.com/v1",
            None,
            None,
        );
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 1,
            "accounts": [{
                "providerId": "deepseek",
                "label": "Work",
                "baseUrl": "https://api.deepseek.com/v1"
            }]
        })
        .to_string();
        let view = preview(&runtime, &payload);
        assert!(view.entries[0].already_exists);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A built-in entry's host and endpoints come from its manifest, so the
    /// preview shows the values create will persist instead of payload
    /// values that would be silently ignored. A gateway entry keeps its own
    /// URL: that host is the only real one.
    #[test]
    fn preview_shows_the_effective_builtin_host_and_endpoints() {
        let (runtime, dir) = runtime("effective");
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": 1,
            "accounts": [
                {
                    "providerId": "mistral",
                    "label": "Mistral copy",
                    "baseUrl": "https://attacker.test/v1",
                    "usageEndpoint": "/usage"
                },
                {
                    "providerId": "one-api",
                    "label": "Gateway copy",
                    "baseUrl": "https://my-relay.test/v1"
                }
            ]
        })
        .to_string();
        let view = preview(&runtime, &payload);
        assert!(view.entries[0].importable);
        assert_eq!(
            view.entries[0].base_url, "https://api.mistral.ai/v1",
            "the manifest host is authoritative for a fixed-host provider"
        );
        assert_eq!(
            view.entries[0].usage_endpoint, None,
            "endpoints a builtin ignores are not shown as if they would apply"
        );
        assert_eq!(view.entries[0].quota_endpoint, None);
        assert!(view.entries[1].importable);
        assert_eq!(
            view.entries[1].base_url, "https://my-relay.test/v1",
            "a gateway keeps the user URL create will use"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
