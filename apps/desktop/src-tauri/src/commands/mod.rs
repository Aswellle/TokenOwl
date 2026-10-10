// IPC command implementations (Application Layer)
// Original content moved from src-tauri/src/commands.rs
use crate::credentials::CredentialStore;
use crate::domain::provider::{
    AuthMethod, DataSource, OpenAiCompatibleConfig, QuotaUnit, SnapshotStatus,
};
use crate::domain::state::{AccountMetadata, Freshness, FreshnessPolicy, RefreshStatus};
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::scheduler::refresh_account;
use crate::storage::{AccountViewDto, LocalRepository, SnapshotViewDto, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use thiserror::Error;

const DEFAULT_QUOTA_ENDPOINT: &str = "/usage";

#[derive(Clone)]
pub struct AppRuntime {
    pub database_path: PathBuf,
    pub credentials: CredentialStore,
    pub adapter: OpenAiCompatibleAdapter,
    pub freshness_policy: FreshnessPolicy,
}

impl AppRuntime {
    pub fn new(database_path: impl Into<PathBuf>) -> Result<Self, String> {
        let database_path = database_path.into();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        LocalRepository::open(&database_path).map_err(|error| error.to_string())?;
        let adapter =
            OpenAiCompatibleAdapter::with_default_limits(OpenAiCompatibleConfig::default())
                .map_err(|error| error.to_string())?;
        Ok(Self {
            database_path,
            credentials: CredentialStore::default(),
            adapter,
            freshness_policy: FreshnessPolicy {
                fresh_for: std::time::Duration::from_secs(600),
                stale_after: std::time::Duration::from_secs(1800),
                hard_expire_after: std::time::Duration::from_secs(86_400),
            },
        })
    }

    fn repository(&self) -> Result<LocalRepository, CommandError> {
        LocalRepository::open(&self.database_path).map_err(CommandError::from)
    }
}

#[derive(Debug, Error)]
pub(crate) enum CommandError {
    #[error("local storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("provider error: {0}")]
    Provider(#[from] crate::domain::provider::ProviderError),
    #[error("credential error: {0}")]
    Credential(#[from] crate::credentials::CredentialError),
    #[error("invalid account input: {0}")]
    InvalidInput(String),
}

impl From<CommandError> for String {
    fn from(error: CommandError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericOpenAiAccountInput {
    pub provider_id: String,
    pub label: String,
    pub base_url: String,
    pub api_key: String,
    pub quota_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindowViewModel {
    pub window_type: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub reset_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaViewModel {
    pub id: String,
    pub metric_name: String,
    pub unit: String,
    pub currency: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub window: Option<QuotaWindowViewModel>,
    pub observed_at: u64,
    pub source: String,
    pub freshness: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountViewModel {
    pub id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub label: String,
    pub account_type: String,
    pub status: String,
    pub freshness: String,
    pub snapshots: Vec<QuotaViewModel>,
    pub source: Option<String>,
    pub last_updated_at: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[tauri::command]
pub fn accounts_list(state: State<'_, Arc<AppRuntime>>) -> Result<Vec<AccountViewModel>, String> {
    list_accounts(&state).map_err(String::from)
}

#[tauri::command]
pub async fn accounts_create(
    input: GenericOpenAiAccountInput,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<AccountViewModel, String> {
    let account = create_account(&state, input).map_err(String::from)?;
    let account_id = account.id.clone();
    // Best-effort first snapshot. A failed request keeps the account plus an explicit
    // refresh error state instead of dropping the account or inventing values.
    let _ = refresh_accounts(&state, Some(account_id.clone())).await;
    list_accounts(&state)
        .map_err(String::from)?
        .into_iter()
        .find(|item| item.id == account_id)
        .ok_or_else(|| "created account could not be read".to_owned())
}

#[tauri::command]
pub fn accounts_delete(
    account_id: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), String> {
    let repository = state.repository().map_err(String::from)?;
    let account = repository
        .account(&account_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "account not found".to_owned())?;
    // The account row is the primary record: removing it must not be blocked by a
    // credential-store outage, and no secret value is ever logged here.
    if let Some(reference) = account.credential_ref.as_deref() {
        let _ = state.credentials.delete(reference);
    }
    repository
        .delete_account(&account_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn accounts_refresh(
    account_id: Option<String>,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<Vec<AccountViewModel>, String> {
    refresh_accounts(&state, account_id)
        .await
        .map_err(String::from)
}

pub(crate) async fn refresh_accounts(
    state: &AppRuntime,
    account_id: Option<String>,
) -> Result<Vec<AccountViewModel>, CommandError> {
    let refresh_one = account_id.is_some();
    let account_ids = {
        let repository = state.repository()?;
        match account_id {
            Some(account_id) => vec![account_id],
            None => repository
                .accounts()?
                .into_iter()
                .map(|account| account.id)
                .collect(),
        }
    };
    for account_id in account_ids {
        let started_at = unix_seconds();
        let repository = state.repository()?;
        let result = refresh_account(
            repository,
            &state.adapter,
            &state.credentials,
            &account_id,
            started_at,
        )
        .await;
        if let Err(error) = result
            && refresh_one
        {
            return Err(CommandError::InvalidInput(error.to_string()));
        }
    }
    list_accounts(state)
}

fn list_accounts(state: &AppRuntime) -> Result<Vec<AccountViewModel>, CommandError> {
    let repository = state.repository()?;
    let now = unix_seconds();
    repository
        .accounts()?
        .into_iter()
        .map(|account| {
            repository
                .account_view(&account.id, now, state.freshness_policy)
                .map_err(CommandError::from)?
                .ok_or_else(|| {
                    CommandError::InvalidInput("account disappeared during read".to_owned())
                })
                .map(account_view_to_model)
        })
        .collect()
}

fn create_account(
    state: &AppRuntime,
    input: GenericOpenAiAccountInput,
) -> Result<AccountViewModel, CommandError> {
    let label = input.label.trim().to_owned();
    if label.is_empty() {
        return Err(CommandError::InvalidInput(
            "account label is required".to_owned(),
        ));
    }
    if input.api_key.trim().is_empty() {
        return Err(CommandError::InvalidInput("API key is required".to_owned()));
    }
    if input.provider_id != "generic.openai_compatible" {
        return Err(CommandError::InvalidInput(
            "unsupported provider id".to_owned(),
        ));
    }
    let endpoint = normalize_endpoint(
        input
            .quota_endpoint
            .as_deref()
            .unwrap_or(DEFAULT_QUOTA_ENDPOINT),
    )?;
    let account_id = format!("account-{}", unix_millis());
    let provider_id = format!("{}:{account_id}", input.provider_id);
    let credential_ref = format!("account/{account_id}");
    let mut config =
        OpenAiCompatibleConfig::new(provider_id, "OpenAI-compatible", input.base_url.trim());
    config.credential_ref = Some(credential_ref.clone());
    config.quota_endpoint = Some(endpoint);
    config.auth_method = AuthMethod::ApiKey;
    config.validate()?;
    state.credentials.set(&credential_ref, &input.api_key)?;
    let repository = state.repository()?;
    if let Err(error) = repository.upsert_provider(&config, unix_seconds()) {
        let _ = state.credentials.delete(&credential_ref);
        return Err(CommandError::Storage(error));
    }
    let account = AccountMetadata::from_auth_input(
        account_id,
        config.id.clone(),
        label,
        "api_key",
        Some(credential_ref),
        HashMap::new(),
        true,
        None,
    );
    if let Err(error) = repository.upsert_account(&account, unix_seconds()) {
        if let Some(reference) = account.credential_ref.as_deref() {
            let _ = state.credentials.delete(reference);
        }
        return Err(CommandError::Storage(error));
    }
    let view = repository
        .account_view(&account.id, unix_seconds(), state.freshness_policy)?
        .ok_or_else(|| {
            CommandError::InvalidInput("created account could not be read".to_owned())
        })?;
    Ok(account_view_to_model(view))
}

fn normalize_endpoint(value: &str) -> Result<String, CommandError> {
    let value = value.trim();
    if value.is_empty()
        || !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\\')
    {
        return Err(CommandError::InvalidInput(
            "quota endpoint must be a relative path such as /usage".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn account_view_to_model(view: AccountViewDto) -> AccountViewModel {
    let provider_name = view.provider.as_ref().map_or_else(
        || view.account.provider_id.clone(),
        |provider| provider.display_name.clone(),
    );
    let snapshots = view
        .snapshots
        .iter()
        .map(snapshot_to_model)
        .collect::<Vec<_>>();
    let freshness = snapshots
        .iter()
        .map(|snapshot| snapshot.freshness.as_str())
        .max_by_key(|value| match *value {
            "fresh" => 0,
            "stale" => 1,
            "expired" => 2,
            _ => 3,
        })
        .unwrap_or("unknown")
        .to_owned();
    let error_code = view.refresh.error_code.clone();
    let has_error = view.refresh.status == RefreshStatus::Failed;
    let status = if has_error {
        "error"
    } else if snapshots.is_empty() {
        "unavailable"
    } else if freshness == "stale" {
        "stale"
    } else {
        "healthy"
    };
    let source = snapshots.first().map(|snapshot| snapshot.source.clone());
    let last_updated_at = view
        .snapshots
        .iter()
        .map(|snapshot| snapshot.observed_at)
        .max();
    AccountViewModel {
        id: view.account.id,
        provider_id: view.account.provider_id,
        provider_name,
        label: view.account.label,
        account_type: view.account.account_type,
        status: status.to_owned(),
        freshness,
        snapshots,
        source,
        last_updated_at,
        error_code,
        error_message: has_error
            .then(|| "Refresh failed. Last known values are retained.".to_owned()),
    }
}

fn snapshot_to_model(view: &SnapshotViewDto) -> QuotaViewModel {
    let snapshot = &view.snapshot;
    QuotaViewModel {
        id: snapshot
            .id
            .clone()
            .or_else(|| snapshot.metric_id.clone())
            .unwrap_or_else(|| format!("metric-{}", view.observed_at)),
        metric_name: snapshot
            .metric_name
            .clone()
            .or_else(|| snapshot.metric_id.clone())
            .unwrap_or_else(|| "Quota".to_owned()),
        unit: quota_unit(snapshot.unit.as_ref()),
        currency: snapshot.currency.clone(),
        used: snapshot.used,
        limit: snapshot.limit,
        remaining: snapshot.remaining,
        window: snapshot.window.as_ref().map(|window| QuotaWindowViewModel {
            window_type: window
                .window_type
                .map(|value| format!("{value:?}").to_lowercase()),
            starts_at: window.starts_at.clone(),
            ends_at: window.ends_at.clone(),
            reset_at: window
                .reset_at
                .clone()
                .or_else(|| snapshot.reset_at.clone()),
        }),
        observed_at: view.observed_at,
        source: source_name(snapshot.source),
        freshness: freshness_name(view.freshness),
        status: snapshot_status(snapshot.status),
    }
}

fn quota_unit(unit: Option<&QuotaUnit>) -> String {
    unit.map_or_else(
        || "unknown".to_owned(),
        |value| format!("{value:?}").to_lowercase(),
    )
}

fn source_name(source: DataSource) -> String {
    match source {
        DataSource::OfficialApi => "OFFICIAL_API",
        DataSource::OAuthApi => "OAUTH_API",
        DataSource::BrowserSession => "BROWSER_SESSION",
        DataSource::Cookie => "COOKIE",
        DataSource::CliLocal => "CLI_LOCAL",
        DataSource::LocalDb => "LOCAL_DB",
        DataSource::CustomEndpoint => "CUSTOM_ENDPOINT",
        DataSource::Manual => "MANUAL",
        DataSource::Cached => "CACHED",
    }
    .to_owned()
}

fn freshness_name(freshness: Freshness) -> String {
    format!("{freshness:?}").to_lowercase()
}

fn snapshot_status(status: SnapshotStatus) -> String {
    format!("{status:?}").to_lowercase()
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
