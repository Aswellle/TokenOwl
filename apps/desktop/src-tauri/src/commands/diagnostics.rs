//! Local, redacted support diagnostics.
//!
//! The bundle contains app/platform identity, the storage schema version,
//! non-secret refresh preferences, provider ids, wire formats, endpoint
//! *kinds*, refresh states, HTTP statuses, and stable error codes. It never
//! contains API keys, credential references, cookies, request/response
//! bodies, headers, proxy URLs, account labels, filesystem paths, or a custom
//! base URL's host/path/query. Export requires the preview token, so the user
//! always confirms the exact document that is written.

use super::{AppRuntime, Arc, CommandError, State, unix_seconds};
use crate::domain::provider::{AuthMethod, EndpointProbe, OpenAiCompatibleConfig, WireFormat};
use crate::scheduler::CredentialVerdict;
use serde::Serialize;
use std::time::Instant;

/// Version of the diagnostic document schema.
pub const DIAGNOSTICS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPreviewView {
    /// FNV-1a of the timestamp-free document. Export accepts only this token,
    /// so a changed bundle cannot be written without a fresh review; the
    /// exported copy carries its own `generatedAt`.
    pub token: String,
    pub generated_at: u64,
    pub json: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportResult {
    pub path: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointTestView {
    pub account_id: String,
    pub provider_id: String,
    /// Present only for built-in manifests: their hosts are public. A custom
    /// endpoint's host is omitted so a private gateway address never leaves
    /// the machine through an exported bundle or a bug report.
    pub base_host: Option<String>,
    pub custom_endpoint: bool,
    pub tested_at: u64,
    pub probes: Vec<EndpointProbe>,
}

fn declared_endpoint_kinds(config: &OpenAiCompatibleConfig) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    for (kind, endpoint) in [
        ("models", &config.models_endpoint),
        ("quota", &config.quota_endpoint),
        ("usage", &config.usage_endpoint),
        ("balance", &config.balance_endpoint),
        ("reset", &config.reset_endpoint),
    ] {
        if endpoint.is_some() {
            kinds.push(kind);
        }
    }
    kinds
}

fn wire_format_name(config: &OpenAiCompatibleConfig) -> &'static str {
    match config.effective_wire_format() {
        WireFormat::OpenAiCompatible => "openai-compatible",
        WireFormat::Anthropic => "anthropic",
        WireFormat::GoogleAi => "google-ai",
    }
}

fn auth_method_name(method: &AuthMethod) -> &'static str {
    match method {
        AuthMethod::ApiKey => "api_key",
        AuthMethod::Bearer => "bearer",
        AuthMethod::OAuth => "oauth",
        AuthMethod::Basic => "basic",
        AuthMethod::BrowserSession => "browser_session",
        AuthMethod::Custom(_) => "custom",
        AuthMethod::None => "none",
    }
}

fn refresh_status_name(status: crate::domain::state::RefreshStatus) -> &'static str {
    match status {
        crate::domain::state::RefreshStatus::Idle => "idle",
        crate::domain::state::RefreshStatus::Refreshing => "refreshing",
        crate::domain::state::RefreshStatus::Succeeded => "healthy",
        crate::domain::state::RefreshStatus::Failed => "failed",
    }
}

fn base_host(config: &OpenAiCompatibleConfig) -> Option<String> {
    url::Url::parse(config.base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

/// Builds the exact JSON document the preview shows and the export writes.
/// Everything here is read-only; a missing optional facet (settings, local
/// usage counts) is reported as empty rather than failing the bundle.
fn build_bundle(runtime: &AppRuntime) -> Result<(String, String), CommandError> {
    let repository = runtime.repository()?;
    let schema_version = repository.schema_version()?;
    let refresh = super::refresh::refresh_settings(&repository);
    let local_sources = match repository.get_setting(super::usage::LOCAL_SOURCES_KEY) {
        Ok(Some(serde_json::Value::Array(values))) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let local_usage_counts = repository.local_usage_counts().unwrap_or_default();
    let proxy_configured = runtime
        .credentials
        .resolve(super::settings::PROXY_CREDENTIAL_REF)
        .is_ok();

    let accounts = repository.accounts()?;
    let mut account_views = Vec::with_capacity(accounts.len());
    for account in &accounts {
        let Some(config) = repository.provider(&account.provider_id)? else {
            continue;
        };
        let state = repository.refresh_state(&account.id)?;
        let builtin = crate::providers::builtin::is_builtin_provider_id(&account.provider_id);
        account_views.push(serde_json::json!({
            "providerId": account.provider_id.split(':').next().unwrap_or(&account.provider_id),
            "providerName": config.display_name,
            "accountType": account.account_type,
            "wireFormat": wire_format_name(&config),
            "authMethod": auth_method_name(&config.auth_method),
            "capabilityLevel": crate::providers::builtin::builtin_capability_level(&account.provider_id),
            "endpointKinds": declared_endpoint_kinds(&config),
            "baseHost": if builtin { base_host(&config) } else { None },
            "customEndpoint": !builtin,
            "refresh": {
                "status": refresh_status_name(state.status),
                "errorCode": state.error_code,
                "httpStatus": state.http_status,
                "endpointKind": state.endpoint_kind,
                "lastAttemptAt": state.last_started_at,
                "lastSuccessAt": state.last_success_at,
                "consecutiveFailures": state.consecutive_failures,
            },
        }));
    }

    let mut bundle = serde_json::json!({
        "schemaVersion": DIAGNOSTICS_SCHEMA_VERSION,
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "storage": {
            "schemaVersion": schema_version,
            "accountCount": accounts.len(),
            "localUsageEventCounts": local_usage_counts
                .iter()
                .map(|(source, count)| serde_json::json!({ "source": source, "events": count }))
                .collect::<Vec<_>>(),
        },
        "settings": {
            "adaptiveRefresh": refresh.adaptive,
            "cadenceMinutes": refresh.cadence_minutes,
            "proxyConfigured": proxy_configured,
            "localUsageSources": local_sources,
        },
        "accounts": account_views,
    });
    // The token covers the stable document only: `generatedAt` changes on
    // every build, so hashing it would make every export fail the preview
    // check one second later. Serialize the timestamp-free value, hash it,
    // then add the timestamp for the displayed/exported document.
    let stable = serde_json::to_string(&bundle).map_err(|error| {
        CommandError::InvalidInput(format!("cannot encode diagnostics: {error}"))
    })?;
    // The token is a staleness guard, not an authentication primitive: FNV-1a
    // is deterministic and forgeable by design, and the document it covers is
    // shown to the user in full before export. Never reuse it to authorize
    // anything beyond "export what was just reviewed".
    let token = format!("{:016x}", crate::usage::fnv1a64(stable.as_bytes()));
    bundle["generatedAt"] = serde_json::json!(unix_seconds());
    let json = serde_json::to_string_pretty(&bundle).map_err(|error| {
        CommandError::InvalidInput(format!("cannot encode diagnostics: {error}"))
    })?;
    Ok((json, token))
}

#[tauri::command]
pub fn diagnostics_preview(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<DiagnosticsPreviewView, CommandError> {
    let (json, token) = build_bundle(&state)?;
    Ok(DiagnosticsPreviewView {
        token,
        generated_at: unix_seconds(),
        json,
    })
}

/// Writes the reviewed bundle. A mismatched token means the underlying state
/// changed since the preview; the user must review the new document first.
/// The written document carries a fresh `generatedAt`; everything the token
/// covers is identical.
#[tauri::command]
pub fn diagnostics_export(
    expected_token: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<DiagnosticsExportResult, CommandError> {
    let (json, token) = build_bundle(&state)?;
    if token != expected_token {
        return Err(CommandError::InvalidInput(
            "diagnostics changed since the preview; review it again before exporting".to_owned(),
        ));
    }
    let path = super::exports::write_export(&state.data_dir, "diagnostics", "json", &json)?;
    Ok(DiagnosticsExportResult {
        path: path.to_string_lossy().into_owned(),
        bytes: json.len(),
    })
}

/// Probes one account's declared endpoints. A failure is per endpoint and
/// never fails the whole command; only a missing account/provider is an error.
#[tauri::command]
pub async fn account_endpoint_test(
    account_id: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<EndpointTestView, CommandError> {
    let runtime = Arc::clone(state.inner());
    let (account, config) = {
        let repository = runtime.repository()?;
        let account = repository
            .account(&account_id)?
            .ok_or_else(|| CommandError::InvalidInput("account not found".to_owned()))?;
        let config = repository
            .provider(&account.provider_id)?
            .ok_or_else(|| CommandError::InvalidInput("provider not found".to_owned()))?;
        (account, config)
    };
    let started = Instant::now();
    let probes = match config.effective_wire_format() {
        WireFormat::OpenAiCompatible => {
            runtime
                .adapter
                .probe_endpoints(&config, &runtime.credentials)
                .await?
        }
        WireFormat::Anthropic => {
            let verdict = runtime
                .anthropic
                .probe_credential(&config, &runtime.credentials)
                .await;
            let mut probe = match verdict {
                Ok(CredentialVerdict::Verified) => EndpointProbe::new("models", "ok"),
                Ok(
                    CredentialVerdict::Rejected(error) | CredentialVerdict::Indeterminate(error),
                )
                | Err(error) => EndpointProbe::from_error("models", &error),
            };
            probe.latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            vec![probe]
        }
        WireFormat::GoogleAi => {
            let verdict = runtime
                .google
                .probe_credential(&config, &runtime.credentials)
                .await;
            let mut probe = match verdict {
                Ok(CredentialVerdict::Verified) => EndpointProbe::new("models", "ok"),
                Ok(
                    CredentialVerdict::Rejected(error) | CredentialVerdict::Indeterminate(error),
                )
                | Err(error) => EndpointProbe::from_error("models", &error),
            };
            probe.latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            vec![probe]
        }
    };
    let builtin = crate::providers::builtin::is_builtin_provider_id(&account.provider_id);
    Ok(EndpointTestView {
        account_id: account.id,
        provider_id: account
            .provider_id
            .split(':')
            .next()
            .unwrap_or("")
            .to_owned(),
        base_host: if builtin { base_host(&config) } else { None },
        custom_endpoint: !builtin,
        tested_at: unix_seconds(),
        probes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::state::AccountMetadata;
    use std::collections::HashMap;

    fn runtime(name: &str) -> (AppRuntime, std::path::PathBuf) {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "tokenowl-diagnostics-{name}-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        let runtime = AppRuntime::new(dir.join("tokenowl.sqlite3")).expect("runtime");
        (runtime, dir)
    }

    fn seed(runtime: &AppRuntime, provider_id: &str, label: &str, base_url: &str) {
        let repository = runtime.repository().expect("db");
        let mut config = OpenAiCompatibleConfig::new(provider_id, "Diagnosed Provider", base_url);
        config.quota_endpoint = Some("/usage".to_owned());
        config.credential_ref = Some("account/secret-ref".to_owned());
        repository.upsert_provider(&config, 1).expect("provider");
        let account = AccountMetadata::from_auth_input(
            "account-1",
            provider_id,
            label,
            "api_key",
            Some("account/secret-ref".to_owned()),
            HashMap::new(),
            true,
            None,
        );
        repository.upsert_account(&account, 1).expect("account");
    }

    fn export_for(
        runtime: &AppRuntime,
        token: &str,
    ) -> Result<DiagnosticsExportResult, CommandError> {
        let (json, current) = build_bundle(runtime)?;
        if current != token {
            return Err(CommandError::InvalidInput(
                "diagnostics changed since the preview; review it again before exporting"
                    .to_owned(),
            ));
        }
        let path =
            super::super::exports::write_export(&runtime.data_dir, "diagnostics", "json", &json)?;
        Ok(DiagnosticsExportResult {
            path: path.to_string_lossy().into_owned(),
            bytes: json.len(),
        })
    }

    #[test]
    fn the_bundle_omits_secrets_paths_labels_and_custom_hosts() {
        let (runtime, dir) = runtime("redaction");
        runtime
            .credentials
            .set("account/secret-ref", "sk-live-must-not-appear")
            .expect("keyring");
        runtime
            .credentials
            .set(
                "transport/proxy",
                "http://user:p4ssw0rd@proxy.internal:8080",
            )
            .expect("proxy");
        seed(
            &runtime,
            "generic.openai_compatible:account-1",
            "Private Gateway",
            "https://private-gateway.internal.example/v1?token=query",
        );
        let (json, token) = build_bundle(&runtime).expect("bundle");
        assert!(!json.contains("sk-live"), "API key leaked");
        assert!(!json.contains("secret-ref"), "credential ref leaked");
        assert!(!json.contains("Private Gateway"), "account label leaked");
        assert!(!json.contains("private-gateway"), "custom host leaked");
        assert!(!json.contains("token=query"), "query leaked");
        assert!(!json.contains("p4ssw0rd"), "proxy credential leaked");
        assert!(json.contains("\"proxyConfigured\": true"));
        assert!(json.contains("openai-compatible"));
        assert!(json.contains("\"endpointKinds\""));
        assert!(json.contains(env!("CARGO_PKG_VERSION")));
        assert_eq!(token.len(), 16);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_builtin_host_is_included_and_still_has_no_path() {
        let (runtime, dir) = runtime("builtin-host");
        seed(
            &runtime,
            "deepseek:account-1",
            "Work",
            "https://api.deepseek.com/v1/private-path",
        );
        let (json, _) = build_bundle(&runtime).expect("bundle");
        assert!(json.contains("api.deepseek.com"));
        assert!(
            !json.contains("/v1/private-path"),
            "only the host may appear for a built-in provider"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_preview_token_is_stable_across_generation_times() {
        let (runtime, dir) = runtime("stable-token");
        let (first_json, first) = build_bundle(&runtime).expect("first");
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        let (second_json, second) = build_bundle(&runtime).expect("second");
        assert_eq!(first, second, "the timestamp must not change the token");
        assert_ne!(
            first_json, second_json,
            "the displayed document still carries a fresh timestamp"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_requires_the_preview_token() {
        let (runtime, dir) = runtime("token");
        let (_json, token) = build_bundle(&runtime).expect("bundle");
        let stale = format!("{token}0");
        let error = export_for(&runtime, &stale).expect_err("stale token");
        assert!(error.to_string().contains("review it again"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_matching_token_writes_the_previewed_document() {
        let (runtime, dir) = runtime("export");
        let (json, token) = build_bundle(&runtime).expect("bundle");
        let result = export_for(&runtime, &token).expect("export");
        let written = std::fs::read_to_string(&result.path).expect("read");
        // `generatedAt` is intentionally fresh on every build; the token
        // covers the stable document, so the rest must match exactly.
        let mut preview: serde_json::Value = serde_json::from_str(&json).expect("preview json");
        let mut exported: serde_json::Value = serde_json::from_str(&written).expect("export json");
        preview
            .as_object_mut()
            .expect("object")
            .remove("generatedAt");
        exported
            .as_object_mut()
            .expect("object")
            .remove("generatedAt");
        assert_eq!(preview, exported);
        assert_eq!(result.bytes, written.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
