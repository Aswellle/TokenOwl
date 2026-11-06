// IPC command implementations (Application Layer)
// Original content moved from src-tauri/src/commands.rs
use crate::credentials::CredentialStore;
use crate::domain::health::{HealthGrade, remaining_ratio, worst_grade};
use crate::domain::history::{aggregate_daily_usage, range_usage};
use crate::domain::provider::{
    AuthMethod, DataSource, OpenAiCompatibleConfig, ProviderError, QuotaUnit, SnapshotStatus,
};
use crate::domain::state::{AccountMetadata, Freshness, FreshnessPolicy, RefreshStatus};
use crate::notifications::{
    Notification, NotificationPreferences, OsNotificationSink, PREFERENCES_KEY,
};
use crate::providers::builtin::anthropic::AnthropicAdapter;
use crate::providers::builtin::google::GoogleAiAdapter;
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::scheduler::refresh_account;
use crate::storage::{AccountViewDto, LocalRepository, SnapshotViewDto, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use thiserror::Error;

mod accounts;
mod history;
mod lan;
mod notifications;
mod settings;

pub use accounts::*;
pub use history::*;
pub use lan::*;
pub use notifications::*;
pub use settings::*;

/// Opens the project's GitHub page in the system browser. The URL is a
/// compile-time constant: the webview never picks arbitrary external URLs.
#[tauri::command]
pub fn open_project_page(app: tauri::AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://github.com/Aswellle/TokenOwl", None::<&str>)
        .map_err(|error| CommandError::InvalidInput(error.to_string()))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::networking::HttpClientConfig;
    use std::time::Duration;

    /// The webview consumes these DTOs through hand-written TS interfaces, so
    /// a field rename, casing slip, or newly-skipped optional would break the
    /// UI silently. The golden shapes pin every field name plus the null
    /// branch of every optional.
    #[test]
    fn account_view_model_serializes_with_the_contracted_shape() {
        let view = AccountViewModel {
            id: "a1".to_owned(),
            provider_id: "openai".to_owned(),
            provider_name: "OpenAI".to_owned(),
            label: "Work".to_owned(),
            account_type: "api".to_owned(),
            status: "healthy".to_owned(),
            freshness: "fresh".to_owned(),
            snapshots: vec![QuotaViewModel {
                id: "q1".to_owned(),
                metric_name: "5h quota".to_owned(),
                unit: "token".to_owned(),
                currency: None,
                used: Some(250.0),
                limit: Some(1000.0),
                remaining: Some(750.0),
                window: Some(QuotaWindowViewModel {
                    window_type: Some("rolling".to_owned()),
                    starts_at: Some("2026-11-07T00:00:00Z".to_owned()),
                    ends_at: Some("2026-11-07T05:00:00Z".to_owned()),
                    reset_at: None,
                }),
                observed_at: 1_799_564_400,
                source: "official_api".to_owned(),
                freshness: "fresh".to_owned(),
                status: "available".to_owned(),
            }],
            source: Some("official_api".to_owned()),
            last_updated_at: Some(1_799_564_400),
            error_code: None,
            error_message: None,
            capability_level: Some("L3".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(&view).expect("serialize"),
            serde_json::json!({
                "id": "a1",
                "providerId": "openai",
                "providerName": "OpenAI",
                "label": "Work",
                "accountType": "api",
                "status": "healthy",
                "freshness": "fresh",
                "snapshots": [{
                    "id": "q1",
                    "metricName": "5h quota",
                    "unit": "token",
                    "currency": null,
                    "used": 250.0,
                    "limit": 1000.0,
                    "remaining": 750.0,
                    "window": {
                        "windowType": "rolling",
                        "startsAt": "2026-11-07T00:00:00Z",
                        "endsAt": "2026-11-07T05:00:00Z",
                        "resetAt": null,
                    },
                    "observedAt": 1_799_564_400u64,
                    "source": "official_api",
                    "freshness": "fresh",
                    "status": "available",
                }],
                "source": "official_api",
                "lastUpdatedAt": 1_799_564_400u64,
                "errorCode": null,
                "errorMessage": null,
                "capabilityLevel": "L3",
            })
        );
    }

    #[test]
    fn notification_view_model_serializes_with_the_contracted_shape() {
        let view = NotificationViewModel {
            id: "n1".to_owned(),
            kind: "low_quota".to_owned(),
            severity: "warning".to_owned(),
            title: "Low quota".to_owned(),
            message: "5h window is 80% consumed".to_owned(),
            account_id: Some("a1".to_owned()),
            provider_id: Some("openai".to_owned()),
            created_at: 1_799_564_400,
            read_at: None,
        };
        assert_eq!(
            serde_json::to_value(&view).expect("serialize"),
            serde_json::json!({
                "id": "n1",
                "kind": "low_quota",
                "severity": "warning",
                "title": "Low quota",
                "message": "5h window is 80% consumed",
                "accountId": "a1",
                "providerId": "openai",
                "createdAt": 1_799_564_400u64,
                "readAt": null,
            })
        );
    }

    #[test]
    fn manifest_profile_serializes_with_the_contracted_shape() {
        let profile = crate::providers::builtin::ManifestProfile {
            id: "deepseek".to_owned(),
            name: "DeepSeek".to_owned(),
            base_url: "https://api.deepseek.com".to_owned(),
            api_format: "openai-compatible".to_owned(),
            custom_base_url: false,
            brand: None,
            variant: None,
            level: "L3".to_owned(),
            account_types: vec!["prepaid".to_owned()],
            quota_units: vec!["currency".to_owned()],
            target_windows: vec!["balance".to_owned()],
            endpoints: crate::providers::manifest::ManifestEndpoints {
                models: Some("/models".to_owned()),
                balance: Some("/user/balance".to_owned()),
                usage: None,
                quota: None,
                reset: None,
            },
        };
        assert_eq!(
            serde_json::to_value(&profile).expect("serialize"),
            serde_json::json!({
                "id": "deepseek",
                "name": "DeepSeek",
                "baseUrl": "https://api.deepseek.com",
                "apiFormat": "openai-compatible",
                "customBaseUrl": false,
                "brand": null,
                "variant": null,
                "level": "L3",
                "accountTypes": ["prepaid"],
                "quotaUnits": ["currency"],
                "targetWindows": ["balance"],
                "endpoints": {
                    "models": "/models",
                    "balance": "/user/balance",
                    "usage": null,
                    "quota": null,
                    "reset": null,
                },
            })
        );
    }

    #[test]
    fn proxy_urls_must_be_absolute_http_or_https() {
        assert!(validate_proxy_url("http://127.0.0.1:7890").is_ok());
        assert!(validate_proxy_url("  https://proxy.corp.internal:3128  ").is_ok());
        assert_eq!(
            validate_proxy_url("  https://proxy.corp.internal:3128  ").expect("trim"),
            "https://proxy.corp.internal:3128"
        );
        // Invalid inputs fail with constant messages that never echo the URL.
        assert!(validate_proxy_url("not a url").is_err());
        assert!(validate_proxy_url("ftp://proxy:21").is_err());
        assert!(validate_proxy_url("").is_err());
        for bad in ["not a url", "ftp://proxy:21", ""] {
            let error = validate_proxy_url(bad).expect_err("must reject");
            assert!(
                !error.to_string().contains(bad) || bad.is_empty(),
                "error text must not echo the input"
            );
        }
    }

    #[test]
    fn proxy_passwords_round_trip_through_the_mask() {
        // The stored secret never re-enters the webview in the clear...
        let masked = mask_proxy_password("http://user:s3cret@proxy.corp:3128");
        assert_eq!(masked, "http://user:__tokenowl_masked__@proxy.corp:3128");
        assert!(!masked.contains("s3cret"));
        // ...and a save that kept the sentinel restores it.
        let restored = restore_masked_password(&masked, Some("http://user:s3cret@proxy.corp:3128"));
        assert_eq!(restored, "http://user:s3cret@proxy.corp:3128");
        // Editing other parts while keeping the mask keeps the password.
        let edited = restore_masked_password(
            "http://user:__tokenowl_masked__@proxy2.corp:8080",
            Some("http://user:s3cret@proxy.corp:3128"),
        );
        assert_eq!(edited, "http://user:s3cret@proxy2.corp:8080");
        // With nothing stored the sentinel degrades to empty credentials.
        assert_eq!(
            restore_masked_password("http://user:__tokenowl_masked__@p:1", None),
            "http://user@p:1"
        );
        // Credential-free URLs pass through both directions untouched.
        assert_eq!(
            mask_proxy_password("http://127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            restore_masked_password("http://127.0.0.1:7890", None),
            "http://127.0.0.1:7890"
        );
        // A typed-in password replaces the mask outright.
        assert_eq!(
            restore_masked_password("http://user:newpass@p:1", Some("http://user:old@p:1")),
            "http://user:newpass@p:1"
        );
    }

    #[test]
    fn proxy_config_keeps_global_policies() {
        let config = HttpClientConfig::from_adapter_limits_with_proxy(
            Duration::from_secs(15),
            512 * 1024,
            Some("http://127.0.0.1:7890".to_owned()),
        );
        assert_eq!(config.proxy.as_deref(), Some("http://127.0.0.1:7890"));
        assert!(!config.follow_redirects, "redirect denial survives a proxy");
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn adapters_build_with_and_without_a_proxy() {
        let config = OpenAiCompatibleConfig::default();
        assert!(OpenAiCompatibleAdapter::with_proxy(None, config.clone()).is_ok());
        assert!(
            OpenAiCompatibleAdapter::with_proxy(
                Some("http://127.0.0.1:7890".to_owned()),
                config.clone()
            )
            .is_ok()
        );
        assert!(
            AnthropicAdapter::with_proxy(Some("http://127.0.0.1:7890".to_owned()), config.clone())
                .is_ok()
        );
        assert!(
            GoogleAiAdapter::with_proxy(Some("http://127.0.0.1:7890".to_owned()), config).is_ok()
        );
    }

    #[test]
    fn lan_trust_list_round_trips_and_deduplicates_by_exact_url() {
        let repository = LocalRepository::open_in_memory().expect("database");
        assert_eq!(
            load_lan_trusts(&repository).expect("empty"),
            Vec::<String>::new()
        );
        save_lan_trusts(
            &repository,
            vec![
                "http://127.0.0.1:4000/v1".to_owned(),
                "http://192.168.1.10:8080/v1".to_owned(),
            ],
        )
        .expect("save");
        let trusted = load_lan_trusts(&repository).expect("load");
        assert_eq!(
            trusted,
            vec![
                "http://127.0.0.1:4000/v1".to_owned(),
                "http://192.168.1.10:8080/v1".to_owned()
            ]
        );
    }

    #[test]
    fn a_trusted_lan_target_skips_the_consent_prompt() {
        let ledger = LanConsentLedger::default();
        let trusted = vec!["http://127.0.0.1:4000/v1".to_owned()];
        // A trusted target passes with no token at all.
        assert!(authorize_lan_target(&ledger, &trusted, "http://127.0.0.1:4000/v1", None).is_ok());
        // An untrusted LAN target still demands the one-time confirmation.
        assert!(authorize_lan_target(&ledger, &trusted, "http://192.168.1.9:9000", None).is_err());
        // Trust is per exact URL: a different port is a different target.
        assert!(authorize_lan_target(&ledger, &trusted, "http://127.0.0.1:4001/v1", None).is_err());
    }

    #[test]
    fn command_errors_serialize_with_stable_codes() {
        let invalid = CommandError::InvalidInput("bad input".to_owned());
        let value = serde_json::to_value(&invalid).expect("serialize");
        assert_eq!(value["code"], "InvalidInput");
        assert_eq!(value["message"], "invalid account input: bad input");

        let auth = CommandError::Provider(ProviderError::new(
            ProviderErrorKind::AuthExpired,
            "provider rejected the credential",
        ));
        let value = serde_json::to_value(&auth).expect("serialize");
        assert_eq!(value["code"], "AuthExpired");

        let storage = CommandError::Storage(StorageError::InvalidData("corrupt".to_owned()));
        let value = serde_json::to_value(&storage).expect("serialize");
        assert_eq!(value["code"], "Storage");
    }

    #[test]
    fn provider_error_kind_names_match_their_variants() {
        // The frontend branches on these codes across IPC (bridge helpers,
        // the account-row recovery actions), so a rename is a wire-contract
        // change. `ProviderErrorKind::name` is an exhaustive match, so a new
        // variant fails the build until it is handled; this pins the exact
        // strings every existing variant crosses the boundary with.
        assert_eq!(ProviderErrorKind::AuthRequired.name(), "AuthRequired");
        assert_eq!(ProviderErrorKind::AuthExpired.name(), "AuthExpired");
        assert_eq!(
            ProviderErrorKind::NetworkUnavailable.name(),
            "NetworkUnavailable"
        );
        assert_eq!(ProviderErrorKind::Timeout.name(), "Timeout");
        assert_eq!(ProviderErrorKind::TlsError.name(), "TlsError");
        assert_eq!(ProviderErrorKind::RateLimited.name(), "RateLimited");
        assert_eq!(
            ProviderErrorKind::ProviderUnavailable.name(),
            "ProviderUnavailable"
        );
        assert_eq!(ProviderErrorKind::InvalidResponse.name(), "InvalidResponse");
        assert_eq!(ProviderErrorKind::ParserFailed.name(), "ParserFailed");
        assert_eq!(
            ProviderErrorKind::UnsupportedCapability.name(),
            "UnsupportedCapability"
        );
        assert_eq!(ProviderErrorKind::ConfigInvalid.name(), "ConfigInvalid");
        assert_eq!(ProviderErrorKind::CredentialError.name(), "CredentialError");
        assert_eq!(ProviderErrorKind::StorageError.name(), "StorageError");
        assert_eq!(ProviderErrorKind::Unknown.name(), "Unknown");
    }

    /// Persists a provider row plus its account so the foreign key on
    /// `accounts.provider_id` is satisfied, mirroring the real create path.
    fn persist_keyring_account(
        repository: &LocalRepository,
        account_id: &str,
        credential_ref: Option<&str>,
    ) {
        let provider_id = format!("generic.openai_compatible:{account_id}");
        let mut config =
            OpenAiCompatibleConfig::new(&provider_id, "Test", "https://api.example.com");
        config.credential_ref = credential_ref.map(str::to_owned);
        config.auth_method = AuthMethod::ApiKey;
        config.validate().expect("valid provider config");
        repository
            .upsert_provider(&config, 1)
            .expect("provider row");
        let account = AccountMetadata::from_auth_input(
            account_id,
            &provider_id,
            "Test",
            "api_key",
            credential_ref.map(str::to_owned),
            HashMap::new(),
            true,
            None,
        );
        repository.upsert_account(&account, 1).expect("account row");
    }

    #[test]
    fn credential_update_rejects_blank_keys_before_touching_the_store() {
        let repository = LocalRepository::open_in_memory().expect("database");
        let mut called = false;
        let result = update_credential_in(
            &repository,
            |_, _| {
                called = true;
                Ok(())
            },
            "a1",
            "   ",
        );
        assert!(matches!(result, Err(CommandError::InvalidInput(_))));
        assert!(
            !called,
            "an empty key must never reach the credential store"
        );
    }

    #[test]
    fn credential_update_targets_the_account_own_reference() {
        let repository = LocalRepository::open_in_memory().expect("database");
        persist_keyring_account(&repository, "a1", Some("account/a1"));
        let mut captured: Option<(String, String)> = None;
        update_credential_in(
            &repository,
            |reference, secret| {
                captured = Some((reference.to_owned(), secret.to_owned()));
                Ok(())
            },
            "a1",
            "  new-key  ",
        )
        .expect("update succeeds");
        // The key is trimmed before it lands in the store, and the reference
        // is the account's own — never a synthesized one.
        assert_eq!(
            captured,
            Some(("account/a1".to_owned(), "new-key".to_owned()))
        );
    }

    #[test]
    fn credential_update_fails_closed_for_unknown_accounts() {
        let repository = LocalRepository::open_in_memory().expect("database");
        let result = update_credential_in(&repository, |_, _| Ok(()), "ghost", "key");
        assert!(matches!(result, Err(CommandError::InvalidInput(_))));
    }

    #[test]
    fn credential_update_requires_a_keyring_backed_account() {
        let repository = LocalRepository::open_in_memory().expect("database");
        persist_keyring_account(&repository, "a1", None);
        let result = update_credential_in(&repository, |_, _| Ok(()), "a1", "key");
        assert!(matches!(result, Err(CommandError::InvalidInput(_))));
    }

    #[test]
    fn endpoint_input_must_be_relative() {
        assert!(normalize_endpoint("/usage").is_ok());
        assert!(normalize_endpoint("https://evil.example/usage").is_err());
        assert!(normalize_endpoint("//evil.example/usage").is_err());
    }

    #[test]
    fn source_and_freshness_names_are_stable() {
        assert_eq!(source_name(DataSource::CustomEndpoint), "CUSTOM_ENDPOINT");
        assert_eq!(freshness_name(Freshness::Stale), "stale");
    }

    use crate::domain::provider::{ProviderErrorKind, QuotaSnapshot};
    use crate::domain::state::RefreshState;

    fn snapshot_view(
        remaining: Option<f64>,
        used: Option<f64>,
        limit: Option<f64>,
        status: SnapshotStatus,
        freshness: Freshness,
    ) -> SnapshotViewDto {
        SnapshotViewDto {
            account_id: "a1".to_owned(),
            snapshot: QuotaSnapshot {
                used,
                limit,
                remaining,
                status,
                ..QuotaSnapshot::default()
            },
            observed_at: 1,
            freshness,
        }
    }

    fn view_with(snapshots: Vec<SnapshotViewDto>, refresh_status: RefreshStatus) -> AccountViewDto {
        AccountViewDto {
            account: AccountMetadata::from_auth_input(
                "a1",
                "generic.openai_compatible:a1",
                "Test",
                "api_key",
                None,
                HashMap::new(),
                true,
                None,
            ),
            provider: None,
            snapshots,
            refresh: RefreshState {
                status: refresh_status,
                ..RefreshState::default()
            },
        }
    }

    fn status_of(view: AccountViewDto) -> String {
        account_view_to_model(view).status
    }

    #[test]
    fn fresh_low_remaining_grades_critical_and_attention() {
        let critical = view_with(
            vec![snapshot_view(
                Some(8.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(critical), "critical");
        // Exactly 10% is the attention bound; strictly below is critical.
        let at_bound = view_with(
            vec![snapshot_view(
                Some(10.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(at_bound), "attention");
    }

    #[test]
    fn a_negative_balance_without_a_limit_is_critical() {
        // DeepSeek reports a negative currency balance with no limit; the
        // absence of a ratio must not make the account look healthy.
        let overdrawn = view_with(
            vec![snapshot_view(
                Some(-0.23),
                None,
                None,
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(overdrawn), "critical");

        let funded = view_with(
            vec![snapshot_view(
                Some(12.5),
                None,
                None,
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(funded), "healthy");
    }

    #[test]
    fn attention_band_ends_at_twenty_five_percent() {
        let attention = view_with(
            vec![snapshot_view(
                Some(24.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(attention), "attention");
        let healthy = view_with(
            vec![snapshot_view(
                Some(25.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(healthy), "healthy");
    }

    #[test]
    fn derived_used_limit_ratio_grades_without_remaining() {
        let view = view_with(
            vec![snapshot_view(
                None,
                Some(92.0),
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(view), "critical");
    }

    #[test]
    fn urgent_ratio_outranks_stale_but_plain_stale_survives() {
        let stale_critical = view_with(
            vec![snapshot_view(
                Some(5.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Stale,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(stale_critical), "critical");
        let stale = view_with(
            vec![snapshot_view(
                Some(60.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Stale,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(stale), "stale");
    }

    #[test]
    fn refresh_error_keeps_precedence_over_grades() {
        let view = view_with(
            vec![snapshot_view(
                Some(5.0),
                None,
                Some(100.0),
                SnapshotStatus::Available,
                Freshness::Fresh,
            )],
            RefreshStatus::Failed,
        );
        assert_eq!(status_of(view), "error");
    }

    #[test]
    fn untrusted_snapshots_never_grade_the_account() {
        let view = view_with(
            vec![snapshot_view(
                Some(5.0),
                None,
                Some(100.0),
                SnapshotStatus::Unavailable,
                Freshness::Fresh,
            )],
            RefreshStatus::Idle,
        );
        assert_eq!(status_of(view), "healthy");
        let empty = view_with(Vec::new(), RefreshStatus::Idle);
        assert_eq!(status_of(empty), "unavailable");
    }

    #[test]
    fn builtin_profile_resolves_manifest_endpoints_and_base_url() {
        let input = GenericOpenAiAccountInput {
            provider_id: "deepseek".to_owned(),
            label: "DeepSeek".to_owned(),
            // Client-supplied paths are ignored for built-in providers.
            base_url: "https://evil.test/v1".to_owned(),
            api_key: "key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: Some("/should-not-be-used".to_owned()),
            lan_consent_token: None,
            api_format: None,
        };
        let resolved = resolve_new_account(&input, None).expect("builtin profile");
        assert_eq!(resolved.provider_prefix, "deepseek");
        assert_eq!(resolved.display_name, "DeepSeek");
        assert_eq!(resolved.base_url, "https://api.deepseek.com/v1");
        assert_eq!(resolved.endpoints.balance.as_deref(), Some("/user/balance"));
        assert!(
            resolved.endpoints.quota.is_none() && resolved.endpoints.usage.is_none(),
            "only the endpoints the manifest declares may be probed"
        );
    }

    #[test]
    fn builtin_profile_rejects_unknown_brand() {
        let input = GenericOpenAiAccountInput {
            provider_id: "not-a-brand".to_owned(),
            label: "X".to_owned(),
            base_url: "https://provider.test/v1".to_owned(),
            api_key: "key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: None,
            lan_consent_token: None,
            api_format: None,
        };
        let error = resolve_new_account(&input, None).expect_err("unknown brand");
        assert!(matches!(error, CommandError::InvalidInput(_)));
    }

    #[test]
    fn generic_profile_requires_http_base_url() {
        let input = GenericOpenAiAccountInput {
            provider_id: "generic.openai_compatible".to_owned(),
            label: "X".to_owned(),
            base_url: "sk-not-a-url".to_owned(),
            api_key: "key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: None,
            lan_consent_token: None,
            api_format: None,
        };
        let error = resolve_new_account(&input, None).expect_err("non-http base url");
        assert!(matches!(error, CommandError::InvalidInput(_)));
    }

    #[test]
    fn endpoint_defaults_apply_only_when_nothing_else_is_configured() {
        let input = |quota: Option<&str>, usage: Option<&str>, balance: Option<&str>| {
            GenericOpenAiAccountInput {
                provider_id: "generic.openai_compatible".to_owned(),
                label: "Test".to_owned(),
                base_url: "https://provider.test/v1".to_owned(),
                api_key: "key".to_owned(),
                quota_endpoint: quota.map(ToOwned::to_owned),
                usage_endpoint: usage.map(ToOwned::to_owned),
                balance_endpoint: balance.map(ToOwned::to_owned),
                lan_consent_token: None,
                api_format: None,
            }
        };
        let resolved = resolve_new_account(&input(None, None, None), None).expect("defaults");
        assert_eq!(resolved.endpoints.quota.as_deref(), Some("/usage"));
        assert!(resolved.endpoints.usage.is_none() && resolved.endpoints.balance.is_none());

        let resolved =
            resolve_new_account(&input(None, Some("/v1/usage"), Some("/v1/balance")), None)
                .expect("multi-endpoint input");
        assert!(
            resolved.endpoints.quota.is_none(),
            "default quota must be suppressed when other endpoints are configured"
        );
        assert_eq!(resolved.endpoints.usage.as_deref(), Some("/v1/usage"));
        assert_eq!(resolved.endpoints.balance.as_deref(), Some("/v1/balance"));

        let error = resolve_new_account(&input(None, Some("https://evil.test/x"), None), None)
            .expect_err("absolute endpoint must be rejected");
        assert!(matches!(error, CommandError::InvalidInput(_)));
    }

    /// Gateway profiles declare `customBaseUrl`, so the account form's URL is
    /// the only real host. Persisting the manifest placeholder
    /// (`https://your-gateway.example`) would send every refresh to a host
    /// that does not exist.
    #[test]
    fn gateway_profile_uses_the_user_supplied_base_url() {
        let input = GenericOpenAiAccountInput {
            provider_id: "litellm".to_owned(),
            label: "My gateway".to_owned(),
            base_url: "https://gateway.mine.test:8443/openai".to_owned(),
            api_key: "key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: None,
            lan_consent_token: None,
            api_format: None,
        };
        let resolved = resolve_new_account(&input, None).expect("gateway resolves");
        assert_eq!(
            resolved.base_url, "https://gateway.mine.test:8443/openai",
            "the user's gateway URL must survive resolution"
        );
        assert!(
            !resolved.base_url.contains("your-gateway.example"),
            "the manifest placeholder must never be persisted"
        );
    }

    /// A fixed-URL built-in keeps the manifest's host no matter what the
    /// client sends, so the capability matrix still gates what gets probed.
    #[test]
    fn fixed_url_profile_ignores_the_client_base_url() {
        let input = GenericOpenAiAccountInput {
            provider_id: "deepseek".to_owned(),
            label: "DS".to_owned(),
            base_url: "https://attacker.test/v1".to_owned(),
            api_key: "key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: None,
            lan_consent_token: None,
            api_format: None,
        };
        let resolved = resolve_new_account(&input, None).expect("built-in resolves");
        assert_eq!(resolved.base_url, "https://api.deepseek.com/v1");
    }

    /// A gateway profile still has to receive a usable absolute URL: the
    /// placeholder is not a fallback.
    #[test]
    fn gateway_profile_rejects_a_missing_or_invalid_base_url() {
        for base_url in ["", "   ", "your-gateway.example", "ftp://gw.test"] {
            let input = GenericOpenAiAccountInput {
                provider_id: "litellm".to_owned(),
                label: "My gateway".to_owned(),
                base_url: base_url.to_owned(),
                api_key: "key".to_owned(),
                quota_endpoint: None,
                usage_endpoint: None,
                balance_endpoint: None,
                lan_consent_token: None,
                api_format: None,
            };
            let error = resolve_new_account(&input, None)
                .expect_err("a gateway profile needs a real base URL");
            assert!(
                matches!(error, CommandError::InvalidInput(_)),
                "`{base_url}` should be rejected"
            );
        }
    }

    /// The form prefills the manifest's placeholder host. Submitting it
    /// unchanged must fail loudly instead of creating an account whose every
    /// refresh silently fails against a host that cannot resolve.
    #[test]
    fn reserved_placeholder_hosts_are_rejected() {
        for base_url in [
            "https://your-gateway.example",
            "https://your-gateway.example/v1",
            "https://example.invalid/v1",
            "https://EXAMPLE.INVALID/v1",
        ] {
            let input = GenericOpenAiAccountInput {
                provider_id: "litellm".to_owned(),
                label: "My gateway".to_owned(),
                base_url: base_url.to_owned(),
                api_key: "key".to_owned(),
                quota_endpoint: None,
                usage_endpoint: None,
                balance_endpoint: None,
                lan_consent_token: None,
                api_format: None,
            };
            let error = resolve_new_account(&input, None)
                .expect_err("a reserved placeholder host must be rejected");
            assert!(
                matches!(error, CommandError::InvalidInput(_)),
                "`{base_url}` should be rejected"
            );
        }
    }

    /// The placeholder rule must not swallow hosts real gateways use:
    /// self-hosted instances commonly run on localhost or a `.test` name.
    #[test]
    fn local_and_test_hosts_still_resolve() {
        for base_url in [
            "http://localhost:4000/v1",
            "http://127.0.0.1:4000/v1",
            "https://gateway.internal.test/v1",
            "https://my.example-gateway.test/v1",
        ] {
            let input = GenericOpenAiAccountInput {
                provider_id: "litellm".to_owned(),
                label: "My gateway".to_owned(),
                base_url: base_url.to_owned(),
                api_key: "key".to_owned(),
                quota_endpoint: None,
                usage_endpoint: None,
                balance_endpoint: None,
                lan_consent_token: None,
                api_format: None,
            };
            let resolved = resolve_new_account(&input, None)
                .unwrap_or_else(|error| panic!("`{base_url}` should resolve: {error:?}"));
            assert_eq!(resolved.base_url, base_url);
        }
    }

    /// A base URL is persisted to `SQLite`, which must never hold secrets, so
    /// userinfo in the URL is rejected on every user-supplied path.
    #[test]
    fn userinfo_in_user_supplied_base_urls_is_rejected() {
        for (provider_id, base_url) in [
            ("litellm", "https://user:secret@gateway.test/v1"),
            (
                "generic.openai_compatible",
                "https://user:secret@relay.test/v1",
            ),
        ] {
            let input = GenericOpenAiAccountInput {
                provider_id: provider_id.to_owned(),
                label: "X".to_owned(),
                base_url: base_url.to_owned(),
                api_key: "key".to_owned(),
                quota_endpoint: None,
                usage_endpoint: None,
                balance_endpoint: None,
                lan_consent_token: None,
                api_format: None,
            };
            let error = resolve_new_account(&input, None)
                .expect_err("credentials embedded in the URL must be rejected");
            assert!(matches!(error, CommandError::InvalidInput(_)));
        }
    }

    #[test]
    fn loopback_and_mdns_hosts_are_lan() {
        for host in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "::1",
            "gateway.local",
            "[::1]",
        ] {
            assert!(is_lan_host(host), "`{host}` should count as LAN");
        }
    }

    #[test]
    fn private_ranges_are_lan() {
        for host in [
            "192.168.1.10",
            "10.0.0.5",
            "172.16.0.1",
            "172.31.255.254",
            "169.254.1.1",
            "fe80::1",
            // IPv6 unique-local addresses (fc00::/7, e.g. fd00::/8) sit on a
            // private network even though they are neither loopback nor
            // link-local.
            "fd12:3456:789a::1",
            "fc00::1",
            // IPv4-mapped IPv6 must not bypass the IPv4 classification.
            "::ffff:192.168.1.10",
            "::ffff:10.0.0.5",
        ] {
            assert!(is_lan_host(host), "`{host}` should count as LAN");
        }
    }

    /// Prefix matching would misclassify these; the numeric check must not.
    #[test]
    fn public_addresses_and_hostnames_are_not_lan() {
        for host in [
            "api.openai.com",
            "8.8.8.8",
            "172.32.0.1",
            "172.15.255.255",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_lan_host(host), "`{host}` should not count as LAN");
        }
    }

    /// Resolution-level classification: one private address among the DNS
    /// answers is enough — a public-looking name that points inside the LAN
    /// must trigger the consent prompt (R-16/R-17).
    #[test]
    fn resolved_addresses_are_lan_when_any_answer_is_private() {
        use std::net::IpAddr;
        let parse = |values: &[&str]| -> Vec<IpAddr> {
            values
                .iter()
                .map(|value| value.parse().expect("valid address"))
                .collect()
        };
        assert!(host_resolves_to_lan(&parse(&[
            "203.0.113.10",
            "192.168.1.20"
        ])));
        assert!(host_resolves_to_lan(&parse(&["fdab::1", "2001:db8::9"])));
        assert!(!host_resolves_to_lan(&parse(&["203.0.113.10", "8.8.8.8"])));
        assert!(!host_resolves_to_lan(&[]));
    }

    /// Plaintext HTTP is loopback-only: anything else would send the API key
    /// unencrypted (R-13).
    #[test]
    fn plaintext_http_is_rejected_outside_loopback() {
        for base_url in [
            "http://203.0.113.9/v1",
            "http://192.168.1.5/v1",
            "http://gateway.internal.test/v1",
            "http://api.openai.com/v1",
        ] {
            assert!(
                validate_user_base_url(base_url).is_err(),
                "`{base_url}` must be rejected: plaintext HTTP off loopback"
            );
        }
        for base_url in [
            "http://localhost:4000/v1",
            "http://127.0.0.1:4000/v1",
            "http://[::1]:9000/v1",
            "https://gateway.internal.test/v1",
            "https://api.openai.com/v1",
        ] {
            assert!(
                validate_user_base_url(base_url).is_ok(),
                "`{base_url}` must stay usable (loopback http or https)"
            );
        }
    }

    /// A base URL is persisted to `SQLite`; query or fragment must never ride
    /// along into storage or logs (R-18-adjacent).
    #[test]
    fn base_urls_with_query_or_fragment_are_rejected() {
        for base_url in [
            "https://gateway.test/v1?token=secret",
            "https://gateway.test/v1#section",
            "https://gateway.test/v1?a=1&b=2",
        ] {
            assert!(
                validate_user_base_url(base_url).is_err(),
                "`{base_url}` must be rejected: no query or fragment on base URLs"
            );
        }
    }

    #[test]
    fn base_url_classification_parses_the_host() {
        assert!(resolves_to_lan("http://localhost:4000/v1"));
        assert!(resolves_to_lan("https://192.168.0.20:8443/api"));
        // Literal IPs keep this test offline-deterministic; a DNS name would
        // make the outcome depend on the resolver.
        assert!(!resolves_to_lan("https://8.8.8.8/v1"));
        assert!(!resolves_to_lan("not a url"));
    }

    #[test]
    fn lan_consent_tokens_are_single_use_and_target_bound() {
        let ledger = LanConsentLedger::default();
        let now = 1_000_u64;
        let target = "https://192.168.0.20:8443/api";
        let token = ledger.begin(target, now);

        // Missing token, wrong target: refused, and the valid token survives.
        assert!(ledger.authorize(target, None, now + 1).is_err());
        assert!(
            ledger
                .authorize("https://192.168.0.20:8443/other", Some(&token), now + 1)
                .is_err()
        );
        // Correct target: authorized exactly once.
        assert!(ledger.authorize(target, Some(&token), now + 1).is_ok());
        assert!(ledger.authorize(target, Some(&token), now + 2).is_err());
        // Unknown token is refused without side effects.
        assert!(
            ledger
                .authorize(target, Some("not-a-token"), now + 1)
                .is_err()
        );
    }

    #[test]
    fn lan_consent_tokens_expire() {
        let ledger = LanConsentLedger::default();
        let now = 1_000_u64;
        let target = "https://10.0.0.5/v1";
        let token = ledger.begin(target, now);
        assert!(
            ledger
                .authorize(target, Some(&token), now + LAN_CONSENT_TTL_SECS)
                .is_ok()
        );
        let token = ledger.begin(target, now);
        assert!(
            ledger
                .authorize(target, Some(&token), now + LAN_CONSENT_TTL_SECS + 1)
                .is_err()
        );
    }

    #[test]
    fn lan_consent_begin_prunes_stale_entries() {
        let ledger = LanConsentLedger::default();
        let now = 1_000_u64;
        ledger.begin("https://10.0.0.5/v1", now);
        ledger.begin("https://10.0.0.6/v1", now);
        assert_eq!(ledger.len(), 2);
        // Well past the TTL both entries are stale; a new begin sweeps them.
        ledger.begin("https://10.0.0.7/v1", now + LAN_CONSENT_TTL_SECS + 1);
        assert_eq!(ledger.len(), 1);
    }

    /// The creation gate classifies in Rust: public targets pass with no
    /// token at all, LAN targets never do (R-19). Literal IPs keep this
    /// test offline-deterministic.
    #[test]
    fn authorize_lan_target_fails_closed_for_local_targets() {
        let ledger = LanConsentLedger::default();
        assert!(
            authorize_lan_target(&ledger, &[], "https://8.8.8.8/v1", None).is_ok(),
            "public targets never need a token"
        );
        assert!(
            authorize_lan_target(&ledger, &[], "https://192.168.0.20/v1", None).is_err(),
            "LAN targets without a confirmation are refused"
        );
        let token = ledger.begin("https://192.168.0.20/v1", unix_seconds());
        assert!(
            authorize_lan_target(&ledger, &[], "https://192.168.0.20/v1", Some(&token)).is_ok()
        );
    }

    fn custom_input(api_format: Option<&str>) -> GenericOpenAiAccountInput {
        GenericOpenAiAccountInput {
            provider_id: "generic.openai_compatible".to_owned(),
            label: "relay".to_owned(),
            base_url: "https://relay.example.test/v1".to_owned(),
            api_key: "sk-synthetic-format-key".to_owned(),
            quota_endpoint: None,
            usage_endpoint: None,
            balance_endpoint: None,
            lan_consent_token: None,
            api_format: api_format.map(ToOwned::to_owned),
        }
    }

    /// A custom Anthropic-format account rides the native adapter via the
    /// `anthropic:` provider-id prefix and authenticates with its header.
    #[test]
    fn resolve_custom_anthropic_format_uses_the_native_prefix() {
        let resolved = resolve_new_account(&custom_input(Some("anthropic")), None)
            .expect("anthropic format resolves");
        assert_eq!(resolved.provider_prefix, "anthropic");
        assert_eq!(resolved.display_name, "Anthropic-compatible");
        assert_eq!(resolved.api_key_header, "x-api-key");
        assert_eq!(
            resolved.endpoints.quota, None,
            "native formats carry no probe endpoints"
        );
    }

    #[test]
    fn resolve_custom_gemini_format_uses_the_native_prefix() {
        let resolved = resolve_new_account(&custom_input(Some("gemini")), None)
            .expect("gemini format resolves");
        assert_eq!(resolved.provider_prefix, "gemini");
        assert_eq!(resolved.display_name, "Gemini-compatible");
        assert_eq!(resolved.api_key_header, "x-goog-api-key");
    }

    /// Omitting the format keeps the historical OpenAI-compatible behavior,
    /// including the default quota probe endpoint.
    #[test]
    fn resolve_custom_without_format_defaults_to_openai_compatible() {
        let resolved = resolve_new_account(&custom_input(None), None).expect("default resolves");
        assert_eq!(resolved.provider_prefix, "generic.openai_compatible");
        assert_eq!(resolved.api_key_header, "Authorization");
        assert!(
            resolved.endpoints.quota.is_some(),
            "default quota probe stays on"
        );
    }

    #[test]
    fn resolve_custom_unknown_format_names_the_supported_set() {
        let error = resolve_new_account(&custom_input(Some("soap")), None)
            .expect_err("unknown format must be rejected");
        let text = error.to_string();
        assert!(text.contains("unsupported API format"), "got `{text}`");
        for supported in ["openai_compatible", "anthropic", "gemini"] {
            assert!(text.contains(supported), "must name {supported}: {text}");
        }
    }

    #[test]
    fn refresh_gate_admits_one_holder_and_shares_state_across_clones() {
        let gate = InFlightGate::default();
        let clone = gate.clone();
        assert!(gate.try_acquire("account-a"));
        // The same account is refused everywhere, including through clones.
        assert!(!gate.try_acquire("account-a"));
        assert!(!clone.try_acquire("account-a"));
        // Other accounts are unaffected.
        assert!(clone.try_acquire("account-b"));
        // Release makes the account acquirable again, exactly once.
        clone.release("account-a");
        assert!(gate.try_acquire("account-a"));
        assert!(!gate.try_acquire("account-a"));
    }

    /// Rapid successive creations must never collide: the wall clock is not
    /// a uniqueness source (two creates inside one millisecond used to share
    /// an id, a provider id, and a credential reference). A regression back
    /// to a millisecond timestamp fails this test almost immediately — a
    /// tight loop of ten thousand calls spans only a few dozen
    /// milliseconds.
    #[test]
    fn rapid_account_id_generation_never_collides() {
        let count = 10_000;
        let mut ids = std::collections::HashSet::with_capacity(count);
        let mut provider_ids = std::collections::HashSet::with_capacity(count);
        let mut credential_refs = std::collections::HashSet::with_capacity(count);
        for _ in 0..count {
            let (account_id, provider_id, credential_ref) = new_account_identity("gateway");
            assert!(
                ids.insert(account_id.clone()),
                "duplicate account id {account_id}"
            );
            assert!(
                provider_ids.insert(provider_id.clone()),
                "duplicate provider id {provider_id}"
            );
            assert!(
                credential_refs.insert(credential_ref.clone()),
                "duplicate credential reference {credential_ref}"
            );
            // All three names derive from the one id, so they cannot drift.
            assert!(provider_id.ends_with(&account_id));
            assert_eq!(credential_ref, format!("account/{account_id}"));
            // The id is opaque and secret-free by construction: a fixed
            // prefix plus nanoid's `[A-Za-z0-9_-]` alphabet.
            let local = account_id.strip_prefix("account-").expect("prefix");
            assert!(
                !local.is_empty()
                    && local
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "id `{account_id}` left the nanoid alphabet"
            );
        }
    }

    /// Create-path compensation: when the rollback delete fails and storage
    /// is available, the reference lands in the durable queue with a stable
    /// error code — and never any secret material.
    #[test]
    fn failed_create_rollback_delete_is_queued_when_storage_exists() {
        use crate::credentials::CredentialError;
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        let secret = "sk-synthetic-compensation-key";
        compensate_credential_failure(
            |_reference| Err(CredentialError::StoreUnavailable),
            Some(&repository),
            "account/orphaned",
            "provider_upsert",
        );
        let queue = load_pending_cleanups(&repository).expect("load queue");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].credential_ref, "account/orphaned");
        assert_eq!(queue[0].error_code, "StoreUnavailable");
        // The queue payload is reference + stable code + timestamp only.
        let raw = repository
            .get_setting(CLEANUP_SETTING_KEY)
            .expect("read setting")
            .expect("present");
        let text = raw.to_string();
        assert!(
            !text.contains(secret),
            "secret leaked into the cleanup queue"
        );
        assert!(!text.contains("StoreUnavailable("), "raw error text leaked");
    }

    /// A successful rollback delete leaves no queue entry behind.
    #[test]
    fn successful_create_rollback_does_not_queue() {
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        compensate_credential_failure(
            |_reference| Ok(()),
            Some(&repository),
            "account/cleaned",
            "account_upsert",
        );
        let queue = load_pending_cleanups(&repository).expect("load queue");
        assert!(queue.is_empty(), "a completed rollback must not be queued");
    }

    /// When the repository itself is unavailable the failed delete cannot be
    /// persisted; the compensation must not panic and must not pretend the
    /// credential was removed (the limitation is what gets logged).
    #[test]
    fn failed_rollback_without_storage_is_reported_not_claimed() {
        use crate::credentials::CredentialError;
        let mut delete_calls = 0;
        compensate_credential_failure(
            |_reference| {
                delete_calls += 1;
                Err(CredentialError::Store("synthetic store outage".to_owned()))
            },
            None,
            "account/unqueueable",
            "repository_open",
        );
        assert_eq!(delete_calls, 1, "the rollback delete is still attempted");
    }

    /// The lease must hold the single-flight slot until it is dropped —
    /// no earlier, no later.
    #[test]
    fn refresh_lease_holds_the_slot_until_dropped() {
        let gate = InFlightGate::default();
        let lease = RefreshLease::acquire(&gate, "account-a").expect("first acquire wins");
        assert!(
            !gate.try_acquire("account-a"),
            "the lease must hold the slot"
        );
        drop(lease);
        assert!(gate.try_acquire("account-a"), "drop must release the slot");
    }

    /// A refresh task that exits abnormally — a panic unwind, or any future
    /// early return that skips an explicit release — must still free the
    /// account's gate slot, or one crash would silence that account's
    /// refreshes forever.
    #[test]
    fn refresh_lease_releases_through_a_panicking_task() {
        let gate = InFlightGate::default();
        let task_gate = gate.clone();
        let task = tauri::async_runtime::spawn(async move {
            let _lease = RefreshLease::acquire(&task_gate, "account-b");
            panic!("synthetic refresh crash");
        });
        let joined = tauri::async_runtime::block_on(task);
        assert!(joined.is_err(), "the task joined with the panic");
        assert!(
            gate.try_acquire("account-b"),
            "the panic must not wedge the account's gate shut"
        );
    }

    #[test]
    fn pending_cleanup_round_trips_and_deduplicates() {
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        record_pending_cleanup(&repository, "account/one", "Store", 1_000).expect("record");
        // A second failure for the same reference is a no-op, not a duplicate.
        record_pending_cleanup(&repository, "account/one", "Store", 2_000).expect("record again");
        record_pending_cleanup(&repository, "account/two", "StoreUnavailable", 3_000)
            .expect("record two");
        let queue = load_pending_cleanups(&repository).expect("load");
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].credential_ref, "account/one");
        assert_eq!(queue[0].error_code, "Store");
        assert_eq!(queue[1].credential_ref, "account/two");
        // Entries carry no secret-bearing payload by construction; the stored
        // JSON must not grow extra fields either.
        let raw = repository
            .get_setting(CLEANUP_SETTING_KEY)
            .expect("read setting")
            .expect("present");
        let text = raw.to_string();
        assert!(text.contains("account/one"));
        assert!(!text.contains("sk-"), "no secret material in the queue");
    }

    #[test]
    fn cleanup_retry_drops_successes_and_keeps_failures() {
        use crate::credentials::CredentialError;
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        record_pending_cleanup(&repository, "account/ok", "Store", 1_000).expect("record ok");
        record_pending_cleanup(&repository, "account/stuck", "StoreUnavailable", 1_000)
            .expect("record stuck");

        let still_pending = retry_pending_cleanups(&repository, |reference| {
            if reference == "account/ok" {
                Ok(())
            } else {
                Err(CredentialError::StoreUnavailable)
            }
        })
        .expect("retry");
        assert_eq!(still_pending, 1);
        let queue = load_pending_cleanups(&repository).expect("load");
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].credential_ref, "account/stuck");

        // Once the store recovers, the last entry clears and the queue
        // empties completely.
        let still_pending = retry_pending_cleanups(&repository, |_| Ok(())).expect("retry again");
        assert_eq!(still_pending, 0);
        let queue = load_pending_cleanups(&repository).expect("load");
        assert_eq!(queue.len(), 0, "the queue must be fully drained");
    }
}
