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
    self, Notification, NotificationPreferences, OsNotificationSink, PREFERENCES_KEY,
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

const DEFAULT_QUOTA_ENDPOINT: &str = "/usage";

#[derive(Clone)]
pub struct AppRuntime {
    pub database_path: PathBuf,
    pub credentials: CredentialStore,
    pub adapter: OpenAiCompatibleAdapter,
    pub anthropic: AnthropicAdapter,
    pub google: GoogleAiAdapter,
    pub freshness_policy: FreshnessPolicy,
    /// Application data directory; hosts the community-provider pack folder.
    pub data_dir: PathBuf,
    /// Session-scoped LAN confirmation tokens (see [`LanConsentLedger`]).
    pub(crate) lan_consents: LanConsentLedger,
    /// Per-account single-flight gate across every refresh entrance.
    pub(crate) refresh_gate: InFlightGate,
}

/// Per-account single-flight gate across the refresh entrances (background
/// loop, UI manual refresh, global hotkey). A request for an account that is
/// already being refreshed is merged away — the in-flight run's outcome
/// serves both — instead of racing two requests against the same provider
/// and letting the slower, older answer overwrite the newer one.
#[derive(Default, Clone)]
pub(crate) struct InFlightGate {
    held: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl InFlightGate {
    /// Acquires the account, or returns `false` when another refresh holds
    /// it. Clones share the same underlying set, so runtime copies cannot
    /// race past each other.
    fn try_acquire(&self, account_id: &str) -> bool {
        self.held
            .lock()
            .expect("refresh gate lock")
            .insert(account_id.to_owned())
    }

    fn release(&self, account_id: &str) {
        self.held
            .lock()
            .expect("refresh gate lock")
            .remove(account_id);
    }
}

/// Session-scoped, one-time LAN consent tokens.
///
/// A token binds one confirmation to one normalized target URL: it expires
/// quickly, is consumed by the first account created with it, and a changed
/// URL invalidates it. The ledger is deliberately in-memory — consent is a
/// per-session UI contract, not persisted state. What the token proves is
/// that the frontend ran the confirmation flow before `accounts_create`; it
/// is not a defense against a compromised renderer, which could call
/// `lan_consent_begin` itself. The real boundary stays in Rust: creation
/// re-classifies the target and refuses LAN targets without a valid token.
///
/// The inner map sits behind an `Arc` so clones of [`AppRuntime`] share one
/// ledger instead of splitting it into parallel token spaces.
#[derive(Default, Clone)]
pub(crate) struct LanConsentLedger {
    issued: std::sync::Arc<std::sync::Mutex<HashMap<String, LanConsentRecord>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LanConsentRecord {
    target: String,
    granted_at: u64,
    consumed: bool,
}

/// How long a confirmation stays valid before the account is created.
const LAN_CONSENT_TTL_SECS: u64 = 600;

impl LanConsentLedger {
    /// Issues a one-time token for `target`, pruning stale entries first so
    /// the session map stays bounded.
    fn begin(&self, target: &str, now: u64) -> String {
        let mut issued = self.issued.lock().expect("consent ledger lock");
        issued.retain(|_, record| {
            !record.consumed && now.saturating_sub(record.granted_at) <= LAN_CONSENT_TTL_SECS
        });
        let token = nanoid::nanoid!();
        issued.insert(
            token.clone(),
            LanConsentRecord {
                target: target.to_owned(),
                granted_at: now,
                consumed: false,
            },
        );
        token
    }

    /// Consumes the token issued for exactly `target`, or explains why the
    /// creation must not proceed. Every failure keeps the record untouched,
    /// so a rejected attempt never burns a still-valid confirmation.
    fn authorize(&self, target: &str, token: Option<&str>, now: u64) -> Result<(), CommandError> {
        let Some(token) = token else {
            return Err(CommandError::InvalidInput(
                "this target is on the local network; confirm the LAN notice before creating the account"
                    .to_owned(),
            ));
        };
        let mut issued = self.issued.lock().expect("consent ledger lock");
        match issued.get_mut(token) {
            Some(record) if record.target != target => Err(CommandError::InvalidInput(
                "the LAN confirmation was issued for a different URL; confirm again".to_owned(),
            )),
            Some(record) if record.consumed => Err(CommandError::InvalidInput(
                "the LAN confirmation was already used; confirm again".to_owned(),
            )),
            Some(record) if now.saturating_sub(record.granted_at) > LAN_CONSENT_TTL_SECS => {
                Err(CommandError::InvalidInput(
                    "the LAN confirmation expired; confirm again".to_owned(),
                ))
            }
            Some(record) => {
                record.consumed = true;
                Ok(())
            }
            None => Err(CommandError::InvalidInput(
                "no LAN confirmation on record for this target; confirm the notice first"
                    .to_owned(),
            )),
        }
    }

    /// Test-only view of the ledger size (used by the pruning test).
    #[cfg(test)]
    fn len(&self) -> usize {
        self.issued.lock().expect("consent ledger lock").len()
    }
}

/// Fails closed when `target` is on the local network and no valid, unused
/// confirmation token was presented. Classification re-runs here — not in
/// the frontend — so a UI bug or a stale dialog can never substitute for
/// consent, and a classifier outage surfaces as a rejected creation instead
/// of a silently public-looking LAN account.
fn authorize_lan_target(
    ledger: &LanConsentLedger,
    target: &str,
    token: Option<&str>,
) -> Result<(), CommandError> {
    if !resolves_to_lan(target) {
        return Ok(());
    }
    ledger.authorize(target, token, unix_seconds())
}

impl AppRuntime {
    pub fn new(database_path: impl Into<PathBuf>) -> Result<Self, String> {
        let database_path = database_path.into();
        let data_dir = database_path
            .parent()
            .map_or_else(|| PathBuf::from("."), ToOwned::to_owned);
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        LocalRepository::open(&database_path).map_err(|error| error.to_string())?;
        let adapter =
            OpenAiCompatibleAdapter::with_default_limits(OpenAiCompatibleConfig::default())
                .map_err(|error| error.to_string())?;
        let anthropic = AnthropicAdapter::with_default_limits(OpenAiCompatibleConfig::default())
            .map_err(|error| error.to_string())?;
        let google = GoogleAiAdapter::with_default_limits(OpenAiCompatibleConfig::default())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            database_path,
            credentials: CredentialStore::default(),
            adapter,
            anthropic,
            google,
            data_dir,
            lan_consents: LanConsentLedger::default(),
            refresh_gate: InFlightGate::default(),
            freshness_policy: FreshnessPolicy {
                fresh_for: std::time::Duration::from_secs(600),
                stale_after: std::time::Duration::from_secs(1800),
                hard_expire_after: std::time::Duration::from_secs(86_400),
            }
            .normalized(),
        })
    }

    /// Startup hook: retries credential deletions a previous run could not
    /// finish. Best-effort — failures stay queued for the next launch.
    pub fn run_startup_cleanup(&self) {
        run_credential_cleanup_sweep(self);
    }

    fn repository(&self) -> Result<LocalRepository, CommandError> {
        LocalRepository::open(&self.database_path).map_err(CommandError::from)
    }

    pub fn save_bubble_position(&self, x: f64, y: f64) -> Result<(), CommandError> {
        let repo = self.repository()?;
        let value = serde_json::json!({ "x": x, "y": y });
        repo.set_setting("bubble_position", &value)?;
        Ok(())
    }

    pub fn load_bubble_position(&self) -> Result<Option<(f64, f64)>, CommandError> {
        let repo = self.repository()?;
        let value = repo.get_setting("bubble_position")?;
        match value {
            Some(v) => {
                let x = v
                    .get("x")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let y = v
                    .get("y")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(Some((x, y)))
            }
            None => Ok(None),
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("local storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericOpenAiAccountInput {
    pub provider_id: String,
    pub label: String,
    pub base_url: String,
    pub api_key: String,
    pub quota_endpoint: Option<String>,
    pub usage_endpoint: Option<String>,
    pub balance_endpoint: Option<String>,
    /// One-time LAN confirmation token from `lan_consent_begin`; required
    /// exactly when Rust classifies the target as local-network.
    #[serde(default)]
    pub lan_consent_token: Option<String>,
}

impl std::fmt::Debug for GenericOpenAiAccountInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // This DTO carries the raw API key across IPC; a derived `Debug`
        // would print it into any log or error path that formats the input.
        // Only field presence is safe to show.
        formatter
            .debug_struct("GenericOpenAiAccountInput")
            .field("provider_id", &self.provider_id)
            .field("label", &self.label)
            .field("base_url", &self.base_url)
            .field("quota_endpoint", &self.quota_endpoint)
            .field("usage_endpoint", &self.usage_endpoint)
            .field("balance_endpoint", &self.balance_endpoint)
            .field(
                "lan_consent_token",
                &self.lan_consent_token.as_ref().map(|_| "<present>"),
            )
            .field("api_key", &"<redacted>")
            .finish()
    }
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationViewModel {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub account_id: Option<String>,
    pub provider_id: Option<String>,
    pub created_at: u64,
    pub read_at: Option<u64>,
}

impl NotificationViewModel {
    fn from_record(record: crate::storage::NotificationRecord) -> Self {
        Self {
            id: record.id,
            kind: record.kind,
            severity: record.severity,
            title: record.title,
            message: record.message,
            account_id: record.account_id,
            provider_id: record.provider_id,
            created_at: record.created_at,
            read_at: record.read_at,
        }
    }
}

#[tauri::command]
pub fn accounts_list(state: State<'_, Arc<AppRuntime>>) -> Result<Vec<AccountViewModel>, String> {
    list_accounts(&state).map_err(String::from)
}

/// Tells the setup form whether a candidate account points at the LAN.
///
/// The form calls this before submitting so it can show the `docs/03` §12
/// consent prompt. Classification lives in Rust because the webview has no
/// network stack and must not grow one just to parse a host. DNS resolution
/// is blocking, so it runs off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn account_target_is_lan(base_url: String) -> bool {
    let trimmed = base_url.trim().to_owned();
    tauri::async_runtime::spawn_blocking(move || resolves_to_lan(&trimmed))
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn lan_consent_begin(
    base_url: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<Option<String>, String> {
    let trimmed = base_url.trim().to_owned();
    // Classification can hit DNS, so keep the blocking work off the async
    // runtime's workers.
    let target = trimmed.clone();
    let needs_consent = tauri::async_runtime::spawn_blocking(move || resolves_to_lan(&target))
        .await
        .map_err(|error| error.to_string())?;
    if !needs_consent {
        return Ok(None);
    }
    Ok(Some(state.lan_consents.begin(&trimmed, unix_seconds())))
}

#[tauri::command]
pub async fn accounts_create(
    app: AppHandle,
    input: GenericOpenAiAccountInput,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<AccountViewModel, String> {
    let account = create_account(&state, input).map_err(String::from)?;
    let account_id = account.id.clone();
    // Best-effort first snapshot. A failed request keeps the account plus an explicit
    // refresh error state instead of dropping the account or inventing values.
    if let Err(error) = refresh_accounts(state.inner(), Some(&app), Some(account_id.clone())).await
    {
        tracing::warn!("initial refresh failed for account {account_id}: {error}");
    }
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
    // The account row is the primary record: removing it must not be blocked
    // by a credential-store outage, and no secret value is ever logged here.
    // A failed keyring delete is recorded for startup retry instead of being
    // silently dropped — otherwise the secret would linger in the OS store
    // with nothing left pointing at it.
    if let Some(reference) = account.credential_ref.as_deref()
        && let Err(error) = state.credentials.delete(reference)
        && let Err(record_error) = record_pending_cleanup(
            &repository,
            reference,
            cleanup_error_code(&error),
            unix_seconds(),
        )
    {
        tracing::warn!("could not record pending credential cleanup: {record_error}");
    }
    repository
        .delete_account(&account_id)
        .map_err(|error| error.to_string())
}

/// Settings key holding credential deletions that failed and must be retried.
const CLEANUP_SETTING_KEY: &str = "credential_cleanup_pending";

/// One orphaned credential reference waiting for a retry. Only the stable
/// error code is kept — never the secret, and never the store's raw error
/// text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingCredentialCleanup {
    pub credential_ref: String,
    pub error_code: String,
    pub recorded_at: u64,
}

/// Stable, secret-free error code for a failed credential-store operation.
fn cleanup_error_code(error: &crate::credentials::CredentialError) -> &'static str {
    match error {
        crate::credentials::CredentialError::EmptyReference => "EmptyReference",
        crate::credentials::CredentialError::StoreUnavailable => "StoreUnavailable",
        crate::credentials::CredentialError::Missing => "Missing",
        crate::credentials::CredentialError::Store(_) => "Store",
    }
}

/// Appends a failed deletion to the persistent queue (deduplicated by
/// reference). The queue lives in the settings table, so it survives
/// restarts without a schema change.
pub(crate) fn record_pending_cleanup(
    repository: &LocalRepository,
    credential_ref: &str,
    error_code: &str,
    now: u64,
) -> Result<(), StorageError> {
    let mut queue = load_pending_cleanups(repository)?;
    if queue
        .iter()
        .any(|entry| entry.credential_ref == credential_ref)
    {
        return Ok(());
    }
    queue.push(PendingCredentialCleanup {
        credential_ref: credential_ref.to_owned(),
        error_code: error_code.to_owned(),
        recorded_at: now,
    });
    let value = serde_json::to_value(&queue)?;
    repository.set_setting(CLEANUP_SETTING_KEY, &value)
}

fn load_pending_cleanups(
    repository: &LocalRepository,
) -> Result<Vec<PendingCredentialCleanup>, StorageError> {
    match repository.get_setting(CLEANUP_SETTING_KEY)? {
        None => Ok(Vec::new()),
        Some(value) => Ok(serde_json::from_value(value)?),
    }
}

/// Retries every queued deletion; entries whose retry succeeds are dropped,
/// the rest stay queued for the next startup. The delete operation is
/// injected so tests can exercise both outcomes without a real keyring.
/// Returns the number of orphans still pending.
pub(crate) fn retry_pending_cleanups(
    repository: &LocalRepository,
    delete: impl Fn(&str) -> Result<(), crate::credentials::CredentialError>,
) -> Result<usize, StorageError> {
    let queue = load_pending_cleanups(repository)?;
    let mut remaining = Vec::new();
    for entry in queue {
        match delete(&entry.credential_ref) {
            Ok(()) => {}
            Err(_) => remaining.push(entry),
        }
    }
    let still_pending = remaining.len();
    if still_pending == 0 {
        // Clear the key entirely rather than persisting an empty array.
        let empty = serde_json::Value::Array(Vec::new());
        repository.set_setting(CLEANUP_SETTING_KEY, &empty)?;
    } else if let Ok(value) = serde_json::to_value(&remaining) {
        repository.set_setting(CLEANUP_SETTING_KEY, &value)?;
    }
    Ok(still_pending)
}

/// Startup sweep: retries orphaned credential deletions recorded by a
/// previous run. Failures stay queued; the app is unaffected either way.
pub(crate) fn run_credential_cleanup_sweep(state: &AppRuntime) {
    let Ok(repository) = state.repository() else {
        return;
    };
    let credentials = state.credentials.clone();
    match retry_pending_cleanups(&repository, |reference| credentials.delete(reference)) {
        Ok(0) => {}
        Ok(still_pending) => {
            tracing::warn!(still_pending, "credential cleanup entries remain queued");
        }
        Err(error) => tracing::warn!("credential cleanup sweep failed: {error}"),
    }
}

#[tauri::command]
pub async fn accounts_refresh(
    app: AppHandle,
    account_id: Option<String>,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<Vec<AccountViewModel>, String> {
    refresh_accounts(state.inner(), Some(&app), account_id)
        .await
        .map_err(String::from)
}

/// Accounts refreshed concurrently per wave. Bounded on purpose: unbounded
/// fan-out would trip provider rate limits, and a quota monitor has nothing
/// to gain from hammering every account at once.
const REFRESH_WAVE_SIZE: usize = 4;

/// Refreshes accounts with per-account single-flight and bounded parallelism.
///
/// An account already being refreshed by another entrance (background loop,
/// hotkey, UI) is skipped — the in-flight run's result serves both — and a
/// failing account never blocks its wave-mates. Single-account failures only
/// propagate as an error when that specific account was requested.
pub(crate) async fn refresh_accounts(
    runtime: &Arc<AppRuntime>,
    handle: Option<&AppHandle>,
    account_id: Option<String>,
) -> Result<Vec<AccountViewModel>, CommandError> {
    let refresh_one = account_id.is_some();
    let account_ids: Vec<String> = {
        let repository = runtime.repository()?;
        match account_id {
            Some(id) => vec![id],
            None => repository
                .accounts()?
                .into_iter()
                .map(|account| account.id)
                .collect(),
        }
    };
    let mut failed_requested = false;
    for wave in account_ids.chunks(REFRESH_WAVE_SIZE) {
        let mut tasks = Vec::with_capacity(wave.len());
        for account_id in wave {
            let runtime = Arc::clone(runtime);
            let account_id = account_id.to_owned();
            let handle = handle.cloned();
            tasks.push(tauri::async_runtime::spawn(async move {
                if !runtime.refresh_gate.try_acquire(&account_id) {
                    // Merged into an in-flight refresh; its outcome applies.
                    return Ok(account_id);
                }
                let started_at = unix_seconds();
                let outcome: Result<(), CommandError> = async {
                    let repository = runtime.repository()?;
                    refresh_account(
                        repository,
                        &runtime.adapter,
                        &runtime.anthropic,
                        &runtime.google,
                        &runtime.credentials,
                        &account_id,
                        started_at,
                    )
                    .await
                    .map_err(|error| match error {
                        crate::scheduler::RefreshError::Provider(provider) => {
                            CommandError::Provider(provider)
                        }
                        crate::scheduler::RefreshError::Storage(storage) => {
                            CommandError::Storage(storage)
                        }
                    })?;
                    Ok(())
                }
                .await;
                // Notifications are best-effort and never change the outcome.
                if let Some(handle) = handle.as_ref() {
                    notify_account_events(runtime.as_ref(), handle, &account_id);
                }
                runtime.refresh_gate.release(&account_id);
                let failed_id = account_id.clone();
                outcome.map(|()| account_id).map_err(|error| {
                    // Only the stable error code crosses the task boundary;
                    // the full error text can echo provider payload details.
                    let code = match &error {
                        CommandError::Provider(provider) => format!("{:?}", provider.code),
                        CommandError::Storage(_) => "Storage".to_owned(),
                        _ => "CommandFailed".to_owned(),
                    };
                    (failed_id, code)
                })
            }));
        }
        for task in tasks {
            match task.await {
                Ok(Ok(_)) => {}
                Ok(Err((account_id, code))) => {
                    tracing::warn!(account = %account_id, code = %code, "refresh failed");
                    failed_requested |= refresh_one;
                }
                Err(join_error) => {
                    tracing::warn!("refresh task joined with error: {join_error}");
                    failed_requested |= refresh_one;
                }
            }
        }
    }
    if refresh_one && failed_requested {
        return Err(CommandError::InvalidInput(
            "failed to refresh account".to_owned(),
        ));
    }
    list_accounts(runtime)
}

#[tauri::command]
pub fn notifications_list(
    state: State<'_, Arc<AppRuntime>>,
    limit: Option<u32>,
) -> Result<Vec<NotificationViewModel>, String> {
    let repository = state.repository().map_err(String::from)?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let records = repository
        .notifications(limit)
        .map_err(|error| error.to_string())?;
    Ok(records
        .into_iter()
        .map(NotificationViewModel::from_record)
        .collect())
}

#[tauri::command]
pub fn notifications_mark_read(
    state: State<'_, Arc<AppRuntime>>,
    ids: Option<Vec<String>>,
) -> Result<(), String> {
    let repository = state.repository().map_err(String::from)?;
    let now = unix_seconds();
    let changed = match ids.as_deref() {
        Some(list) if !list.is_empty() => {
            let refs = list.iter().map(String::as_str).collect::<Vec<_>>();
            repository.mark_notifications_read(&refs, now)
        }
        _ => repository.mark_all_notifications_read(now),
    };
    changed.map(|_| ()).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn notification_preferences_get(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<NotificationPreferences, String> {
    load_notification_preferences(&state).map_err(String::from)
}

#[tauri::command]
pub fn notification_preferences_set(
    state: State<'_, Arc<AppRuntime>>,
    prefs: NotificationPreferences,
) -> Result<(), String> {
    prefs.validate()?;
    let repository = state.repository().map_err(String::from)?;
    let value = serde_json::to_value(&prefs).map_err(|error| error.to_string())?;
    repository
        .set_setting(PREFERENCES_KEY, &value)
        .map_err(|error| error.to_string())
}

/// QA drill: sends one synthetic notification through the production enqueue
/// and OS sink chain. Only reached via the `TOKENOWL_TEST_BANNER=1` startup
/// hook; the timestamped dedup key keeps repeat drills outside the cooldown.
pub(crate) fn run_test_banner_drill(state: &AppRuntime, app: &tauri::AppHandle) {
    let prefs = match load_notification_preferences(state) {
        Ok(prefs) => prefs,
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: preferences unavailable");
            return;
        }
    };
    let repository = match state.repository() {
        Ok(repository) => repository,
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: repository unavailable");
            return;
        }
    };
    let now = unix_seconds();
    let draft = notifications::Notification {
        id: format!("ntf-drill-{now}"),
        kind: notifications::NotificationKind::LowQuota,
        severity: notifications::NotificationSeverity::Critical,
        title: "TokenOwl 演练通知".to_owned(),
        message: "本条通知通过真实派发链路发送，用于验证系统横幅。".to_owned(),
        provider_id: None,
        account_id: None,
        dedup_key: format!("drill-{now}"),
        created_at: now,
    };
    let sink = notifications::OsNotificationSink { app: app.clone() };
    match notifications::enqueue_new(&repository, &sink, &prefs, vec![draft], now) {
        Ok(delivered) => {
            tracing::info!(target: "drill", delivered = delivered.len(), "banner drill dispatched");
        }
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: enqueue failed");
        }
    }
}

/// Loads persisted preferences; a missing or malformed entry means defaults.
fn load_notification_preferences(
    state: &AppRuntime,
) -> Result<NotificationPreferences, CommandError> {
    let value = state.repository()?.get_setting(PREFERENCES_KEY)?;
    value
        .map(|value| {
            serde_json::from_value::<NotificationPreferences>(value)
                .map_err(|error| CommandError::InvalidInput(error.to_string()))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

/// Runs the trigger evaluation for one account after its refresh and raises
/// the enqueue event for everything that was actually delivered. Failures are
/// logged and swallowed: notification problems must never fail a refresh.
fn notify_account_events(state: &AppRuntime, handle: &AppHandle, account_id: &str) {
    let sink = OsNotificationSink {
        app: handle.clone(),
    };
    match enqueue_account_notifications(state, &sink, account_id) {
        Ok(delivered) => {
            for notification in &delivered {
                let _ = handle.emit("notification:enqueue", notification);
            }
        }
        Err(error) => {
            tracing::warn!("notification evaluation failed for account {account_id}: {error}");
        }
    }
}

/// Shared evaluation + enqueue path, separated from the OS sink so tests can
/// pass a `LogSink`.
pub(crate) fn enqueue_account_notifications(
    state: &AppRuntime,
    sink: &dyn notifications::NotificationSink,
    account_id: &str,
) -> Result<Vec<Notification>, CommandError> {
    let prefs = load_notification_preferences(state)?;
    if !prefs.enabled {
        return Ok(Vec::new());
    }
    let repository = state.repository()?;
    let account = repository
        .account(account_id)?
        .ok_or_else(|| CommandError::InvalidInput("account not found".to_owned()))?;
    let now = unix_seconds();
    let view = repository
        .account_view(account_id, now, state.freshness_policy)?
        .ok_or_else(|| CommandError::InvalidInput("account view unavailable".to_owned()))?;
    // Burn rate comes from the same history the analytics view reads, so a
    // "runs out in 40 minutes" banner agrees with the number on screen. Thin
    // history yields `None`, which disables the predictive rule instead of
    // guessing.
    let pace = repository
        .history_samples(now.saturating_sub(86_400), now)
        .ok()
        .and_then(|samples| {
            let daily: Vec<crate::domain::history::DailySample> = samples
                .iter()
                .map(|sample| crate::domain::history::DailySample {
                    account_id: sample.account_id.clone(),
                    metric_key: sample.metric_key.clone(),
                    observed_at: sample.observed_at,
                    used: sample.used,
                    remaining: sample.remaining,
                    unit: sample.unit.clone(),
                    currency: sample.currency.clone(),
                })
                .collect();
            crate::domain::history::burn_rate_hourly(&daily, now.saturating_sub(86_400), now)
        });
    let candidates = notifications::evaluate_account_notifications(notifications::TriggerContext {
        account_id: &account.id,
        account_label: &account.label,
        provider_id: &account.provider_id,
        snapshots: &view.snapshots,
        refresh: &view.refresh,
        prefs: &prefs,
        now,
        pace: pace.as_ref(),
    });
    let delivered = notifications::enqueue_new(&repository, sink, &prefs, candidates, now)
        .map_err(CommandError::from)?;
    // A recovery notice is one-shot: drop the retained error so the next
    // successful refresh does not announce the same recovery again.
    if delivered
        .iter()
        .any(|item| item.kind == notifications::NotificationKind::Recovered)
    {
        let _ = repository.clear_recovered_error(account_id, now);
    }
    Ok(delivered)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayViewModel {
    /// UTC timestamp of local midnight, so the webview renders the user's own
    /// calendar day regardless of its own timezone handling.
    pub day_start: u64,
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    /// False when nothing was observed that day: unknown, not zero.
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummaryViewModel {
    pub days: Vec<HistoryDayViewModel>,
    pub total_tokens: f64,
    pub total_cost: Option<f64>,
    pub currency: Option<String>,
    pub active_days: u32,
    pub range_days: u32,
}

/// UTC offset in seconds at `timestamp`, for local-calendar day bucketing.
fn local_offset_at(timestamp: i64) -> i32 {
    use chrono::{Offset as _, TimeZone as _};
    chrono::Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map_or(0, |dt| dt.offset().fix().local_minus_utc())
}

#[tauri::command]
pub fn history_summary(
    state: State<'_, Arc<AppRuntime>>,
    days: Option<u32>,
    account_id: Option<String>,
) -> Result<HistorySummaryViewModel, String> {
    let range_days = days.unwrap_or(182).clamp(7, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(range_days) * 86_400);
    let repository = state.repository().map_err(String::from)?;
    let mut samples = repository
        .history_samples(from, now)
        .map_err(|error| error.to_string())?;
    if let Some(id) = account_id.as_deref() {
        samples.retain(|sample| sample.account_id == id);
    }
    let daily = aggregate_daily_usage(
        &samples
            .into_iter()
            .map(|sample| crate::domain::history::DailySample {
                account_id: sample.account_id,
                metric_key: sample.metric_key,
                observed_at: sample.observed_at,
                used: sample.used,
                remaining: sample.remaining,
                unit: sample.unit,
                currency: sample.currency,
            })
            .collect::<Vec<_>>(),
        local_offset_at,
    );

    let mut total_tokens = 0.0;
    let mut totals_by_currency: HashMap<String, f64> = HashMap::new();
    let mut active_days = 0_u32;
    let view_days = daily
        .into_iter()
        .map(|day| {
            total_tokens += day.tokens_used;
            if day.tokens_used > 0.0 || day.cost.is_some() {
                active_days += 1;
            }
            if let (Some(cost), currency) = (day.cost, day.currency.clone()) {
                *totals_by_currency
                    .entry(currency.unwrap_or_default())
                    .or_insert(0.0) += cost;
            }
            HistoryDayViewModel {
                day_start: day.day_start,
                tokens: day.tokens_used,
                cost: day.cost,
                currency: day.currency,
                has_data: day.has_data,
            }
        })
        .collect::<Vec<_>>();
    let (dominant_currency, total_cost) = totals_by_currency
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or((None, None), |(key, total)| {
            (
                if key.is_empty() {
                    None
                } else {
                    Some(key.clone())
                },
                Some(*total),
            )
        });

    Ok(HistorySummaryViewModel {
        days: view_days,
        total_tokens,
        total_cost: total_cost.filter(|total| *total > 0.0),
        currency: dominant_currency,
        active_days,
        range_days,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeUsageViewModel {
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindowsViewModel {
    /// Usage observed in the trailing 2 hours.
    pub hours2: RangeUsageViewModel,
    /// Usage observed in the trailing 24 hours.
    pub hours24: RangeUsageViewModel,
    /// Usage observed in the trailing 30 days.
    pub days30: RangeUsageViewModel,
    /// Token consumption velocity over the trailing 24 hours and how long
    /// the remaining quota lasts at that pace. `None` when the window holds
    /// no trusted rising token series — unknown, not zero.
    pub burn_rate: Option<crate::domain::history::BurnRate>,
}

/// Trailing-window usage for one account (or every account when `account_id`
/// is omitted), derived from recorded history. Windows without observations
/// report `has_data: false` — unknown, not zero.
#[tauri::command]
pub fn usage_windows(
    state: State<'_, Arc<AppRuntime>>,
    account_id: Option<String>,
) -> Result<UsageWindowsViewModel, String> {
    let now = unix_seconds();
    let from = now.saturating_sub(30 * 86_400);
    let repository = state.repository().map_err(String::from)?;
    let mut samples = repository
        .history_samples(from, now)
        .map_err(|error| error.to_string())?;
    if let Some(id) = account_id.as_deref() {
        samples.retain(|sample| sample.account_id == id);
    }
    let view = |range: u64| {
        let usage = range_usage(
            &samples
                .iter()
                .map(|sample| crate::domain::history::DailySample {
                    account_id: sample.account_id.clone(),
                    metric_key: sample.metric_key.clone(),
                    observed_at: sample.observed_at,
                    used: sample.used,
                    remaining: sample.remaining,
                    unit: sample.unit.clone(),
                    currency: sample.currency.clone(),
                })
                .collect::<Vec<_>>(),
            now.saturating_sub(range),
            now,
        );
        RangeUsageViewModel {
            tokens: usage.tokens,
            cost: usage.cost,
            currency: usage.currency,
            has_data: usage.has_data,
        }
    };
    let daily: Vec<crate::domain::history::DailySample> = samples
        .iter()
        .map(|sample| crate::domain::history::DailySample {
            account_id: sample.account_id.clone(),
            metric_key: sample.metric_key.clone(),
            observed_at: sample.observed_at,
            used: sample.used,
            remaining: sample.remaining,
            unit: sample.unit.clone(),
            currency: sample.currency.clone(),
        })
        .collect();
    let burn_rate =
        crate::domain::history::burn_rate_hourly(&daily, now.saturating_sub(24 * 3_600), now);
    Ok(UsageWindowsViewModel {
        hours2: view(2 * 3_600),
        hours24: view(24 * 3_600),
        days30: view(30 * 86_400),
        burn_rate,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayAccountViewModel {
    pub account_id: String,
    pub label: String,
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayBreakdownViewModel {
    /// UTC timestamp of local midnight for the day bucket.
    pub day_start: u64,
    pub accounts: Vec<HistoryDayAccountViewModel>,
}

/// Per-provider token/cost breakdown for each recorded day — the data behind
/// the trend chart's hover details. Quota overviews are joined client-side
/// from the accounts list, which already carries the latest snapshots.
#[tauri::command]
pub fn history_breakdown(
    state: State<'_, Arc<AppRuntime>>,
    days: Option<u32>,
) -> Result<Vec<HistoryDayBreakdownViewModel>, String> {
    let range_days = days.unwrap_or(30).clamp(7, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(range_days) * 86_400);
    let repository = state.repository().map_err(String::from)?;
    let samples = repository
        .history_samples(from, now)
        .map_err(|error| error.to_string())?;
    let labels: HashMap<String, String> = repository
        .accounts()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|account| (account.id, account.label))
        .collect();

    let mut per_account: HashMap<String, Vec<crate::domain::history::DailySample>> = HashMap::new();
    for sample in samples {
        per_account
            .entry(sample.account_id.clone())
            .or_default()
            .push(crate::domain::history::DailySample {
                account_id: sample.account_id,
                metric_key: sample.metric_key,
                observed_at: sample.observed_at,
                used: sample.used,
                remaining: sample.remaining,
                unit: sample.unit,
                currency: sample.currency,
            });
    }

    let mut days_map: std::collections::BTreeMap<u64, Vec<HistoryDayAccountViewModel>> =
        std::collections::BTreeMap::new();
    for (account_id, account_samples) in per_account {
        let daily = aggregate_daily_usage(&account_samples, local_offset_at);
        for day in daily {
            if !day.has_data || (day.tokens_used <= 0.0 && day.cost.is_none()) {
                continue;
            }
            days_map
                .entry(day.day_start)
                .or_default()
                .push(HistoryDayAccountViewModel {
                    account_id: account_id.clone(),
                    label: labels
                        .get(&account_id)
                        .cloned()
                        .unwrap_or_else(|| account_id.clone()),
                    tokens: day.tokens_used,
                    cost: day.cost,
                    currency: day.currency,
                });
        }
    }
    Ok(days_map
        .into_iter()
        .rev()
        .map(|(day_start, accounts)| HistoryDayBreakdownViewModel {
            day_start,
            accounts,
        })
        .collect())
}

/// Setup profiles for the built-in providers, straight from their embedded
/// manifests: base URL, capability level, and the endpoints each provider
/// actually declares. Community packs from the data directory are appended.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProfilesView {
    pub profiles: Vec<crate::providers::builtin::ManifestProfile>,
    /// Community packs that failed validation, as "file: reason" strings.
    pub skipped: Vec<String>,
}

#[tauri::command]
pub fn manifest_profiles(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<ManifestProfilesView, String> {
    let (profiles, skipped) =
        crate::providers::builtin::all_manifest_profiles(Some(&state.data_dir));
    Ok(ManifestProfilesView { profiles, skipped })
}

/// The `docs/09` provider capability catalog: every planned provider with its
/// account types, units, and windows; implemented providers carry their
/// capability level from the embedded manifest.
#[tauri::command]
pub fn provider_matrix() -> Result<Vec<crate::providers::builtin::ProviderMatrixRow>, String> {
    Ok(crate::providers::builtin::provider_matrix())
}

/// Opens the project's GitHub page in the system browser. The URL is a
/// compile-time constant: the webview never picks arbitrary external URLs.
#[tauri::command]
pub fn open_project_page(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("https://github.com/Aswellle/TokenOwl", None::<&str>)
        .map_err(|error| error.to_string())
}

fn list_accounts(state: &AppRuntime) -> Result<Vec<AccountViewModel>, CommandError> {
    let repository = state.repository()?;
    let now = unix_seconds();
    let accounts = repository.accounts()?;
    let mut views = Vec::with_capacity(accounts.len());
    for account in &accounts {
        if let Some(view) = repository.account_view(&account.id, now, state.freshness_policy)? {
            views.push(account_view_to_model(view));
        }
    }
    Ok(views)
}

/// Validated relative endpoints for a new account. The default `/usage`
/// endpoint only applies when nothing else is configured, so accounts that
/// declare only a balance endpoint are not probed at a path that may not exist.
#[derive(Debug)]
struct AccountEndpoints {
    models: Option<String>,
    quota: Option<String>,
    usage: Option<String>,
    balance: Option<String>,
}

/// Everything account persistence needs, resolved from either the generic
/// OpenAI-compatible path or a built-in provider manifest.
#[derive(Debug)]
struct ResolvedNewAccount {
    /// Brand prefix for the per-account provider id (`deepseek:account-…`).
    provider_prefix: String,
    display_name: String,
    base_url: String,
    endpoints: AccountEndpoints,
}

/// Whether a host targets the local machine or a private network.
///
/// `docs/06` blocks LAN access by default and `docs/03` §12 requires an
/// explicit prompt the first time a provider points at one. This is the
/// classification half; the consent itself is collected by the account form.
///
/// Loopback and link-local ranges are always local. Private ranges are
/// detected numerically rather than by prefix matching, so `172.16.0.1` is
/// private while `172.32.0.1` (public) is not. IPv6 unique-local addresses
/// (`fc00::/7`) and IPv4-mapped IPv6 addresses are classified by the same
/// rules — the v6 form must not bypass the v4 classification.
pub(crate) fn is_lan_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.localdomain")
    {
        return true;
    }
    // `.local` is mDNS: resolution stays on the local link.
    if host.to_ascii_lowercase().ends_with(".local") {
        return true;
    }
    let Ok(address) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    ip_is_lan(&address)
}

/// Numeric LAN classification of one address, shared by literal hosts and
/// DNS answers so both sides follow identical rules.
fn ip_is_lan(address: &std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            // `::ffff:a.b.c.d` carries an IPv4 address; classify by it so the
            // mapped form cannot sidestep the IPv4 private-range rules.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_is_lan(&std::net::IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                // fe80::/10 link-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // fc00::/7 unique-local (fd00::/8 in practice)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Whether a host is this machine only (loopback), not the wider LAN. Used
/// for the plaintext-HTTP exception: `http` is tolerated for local
/// development proxies, never for remote or LAN targets.
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.localdomain")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(std::net::IpAddr::V6(v6)) => v6.is_loopback(),
        Err(_) => false,
    }
}

/// One private answer among the DNS results is enough to classify the name
/// as LAN: split-horizon setups routinely mix public and private records.
fn host_resolves_to_lan(addresses: &[std::net::IpAddr]) -> bool {
    addresses.iter().any(ip_is_lan)
}

/// True when the resolved account would talk to the LAN or this machine.
///
/// Classification is layered: literal hosts are checked numerically, and a
/// plain DNS name is resolved once so a public-looking name that points
/// inside the network still triggers the consent prompt. This is a
/// best-effort classification for the consent UX — not a connect-time
/// guarantee against DNS rebinding (the resolver answer and the later
/// connection are not pinned together); the transport's URL policy remains
/// the enforcement layer.
fn resolves_to_lan(base_url: &str) -> bool {
    use std::net::ToSocketAddrs as _;

    let Some(host) = url::Url::parse(base_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
    else {
        return false;
    };
    if is_lan_host(&host) {
        return true;
    }
    // `ToSocketAddrs` needs a port; only the addresses matter here, so the
    // port is a placeholder and never dialed.
    let host_for_lookup = host.trim_start_matches('[').trim_end_matches(']');
    match (host_for_lookup, 0).to_socket_addrs() {
        Ok(addresses) => host_resolves_to_lan(&addresses.map(|addr| addr.ip()).collect::<Vec<_>>()),
        Err(_) => false,
    }
}

fn resolve_new_account(
    input: &GenericOpenAiAccountInput,
    data_dir: Option<&std::path::Path>,
) -> Result<ResolvedNewAccount, CommandError> {
    let label = input.label.trim().to_owned();
    if label.is_empty() {
        return Err(CommandError::InvalidInput(
            "account label is required".to_owned(),
        ));
    }
    if input.api_key.trim().is_empty() {
        return Err(CommandError::InvalidInput("API key is required".to_owned()));
    }

    if input.provider_id == "generic.openai_compatible" {
        let usage = input
            .usage_endpoint
            .as_deref()
            .map(normalize_endpoint)
            .transpose()?;
        let balance = input
            .balance_endpoint
            .as_deref()
            .map(normalize_endpoint)
            .transpose()?;
        let quota = match input.quota_endpoint.as_deref() {
            Some(path) => Some(normalize_endpoint(path)?),
            None if usage.is_none() && balance.is_none() => {
                Some(normalize_endpoint(DEFAULT_QUOTA_ENDPOINT)?)
            }
            None => None,
        };
        let base_url = validate_user_base_url(&input.base_url)?;
        return Ok(ResolvedNewAccount {
            provider_prefix: "generic.openai_compatible".to_owned(),
            display_name: "OpenAI-compatible".to_owned(),
            base_url,
            endpoints: AccountEndpoints {
                models: None,
                quota,
                usage,
                balance,
            },
        });
    }

    // A built-in provider: the embedded manifest is authoritative for the
    // base URL and endpoints — client-supplied paths are ignored, so the
    // capability matrix actually gates what gets probed.
    let (profiles, _) = crate::providers::builtin::all_manifest_profiles(data_dir);
    let profile = profiles
        .into_iter()
        .find(|profile| profile.id == input.provider_id)
        .ok_or_else(|| CommandError::InvalidInput("unsupported provider id".to_owned()))?;
    // A gateway (`customBaseUrl`) has no fixed host — its manifest carries a
    // placeholder like `https://your-gateway.example`, so the account form's
    // URL is the only real host and must be used.
    let base_url = if profile.custom_base_url {
        validate_user_base_url(&input.base_url)?
    } else {
        profile.base_url
    };
    Ok(ResolvedNewAccount {
        provider_prefix: profile.id,
        display_name: profile.name,
        base_url,
        endpoints: AccountEndpoints {
            models: profile.endpoints.models,
            quota: profile.endpoints.quota,
            usage: profile.endpoints.usage,
            balance: profile.endpoints.balance,
        },
    })
}

/// Validates a base URL the user typed (custom provider or gateway profile).
///
/// Absolute http(s) with a host. Credentials embedded in the URL are rejected
/// because the base URL is persisted to `SQLite`, which must never hold secrets.
fn validate_user_base_url(base_url: &str) -> Result<String, CommandError> {
    let trimmed = base_url.trim();
    let parsed = url::Url::parse(trimmed).map_err(|_| {
        CommandError::InvalidInput("base URL must be an absolute http or https URL".to_owned())
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(CommandError::InvalidInput(
            "base URL must use http or https".to_owned(),
        ));
    }
    if parsed.host_str().is_none() {
        return Err(CommandError::InvalidInput(
            "base URL must include a host".to_owned(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(CommandError::InvalidInput(
            "base URL must not embed credentials".to_owned(),
        ));
    }
    // The base URL is persisted and shown in logs and error paths; a query or
    // fragment could carry a token into all of them. Endpoint paths belong in
    // their own form fields.
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(CommandError::InvalidInput(
            "base URL must not include a query or fragment".to_owned(),
        ));
    }
    // Plaintext HTTP exposes the API key to everyone on the path. It stays
    // available for loopback development proxies only; remote and LAN hosts
    // must use https.
    if parsed.scheme() == "http" && !is_loopback_host(parsed.host_str().unwrap_or_default()) {
        return Err(CommandError::InvalidInput(
            "plaintext http is only allowed for localhost; use https for this host".to_owned(),
        ));
    }
    // The gateway form prefills the manifest placeholder, so an unedited
    // submit would otherwise persist a host that can never resolve and fail
    // every refresh silently. `.example`/`.invalid` are IANA-reserved
    // (RFC 2606) and never resolve; `.test` and `localhost` are left alone
    // because local gateways legitimately use them.
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "example"
        || host.ends_with(".example")
        || host == "invalid"
        || host.ends_with(".invalid")
    {
        return Err(CommandError::InvalidInput(format!(
            "base URL host `{host}` is a reserved placeholder that can never resolve; enter your own host"
        )));
    }
    Ok(trimmed.to_owned())
}

fn persist_new_account(
    state: &AppRuntime,
    label: &str,
    resolved: &ResolvedNewAccount,
    api_key: &str,
) -> Result<AccountMetadata, CommandError> {
    let account_id = format!("account-{}", unix_millis());
    let provider_id = format!("{}:{account_id}", resolved.provider_prefix);
    let credential_ref = format!("account/{account_id}");
    let mut config = OpenAiCompatibleConfig::new(
        provider_id,
        resolved.display_name.clone(),
        resolved.base_url.clone(),
    );
    config.credential_ref = Some(credential_ref.clone());
    config
        .models_endpoint
        .clone_from(&resolved.endpoints.models);
    config.quota_endpoint.clone_from(&resolved.endpoints.quota);
    config.usage_endpoint.clone_from(&resolved.endpoints.usage);
    config
        .balance_endpoint
        .clone_from(&resolved.endpoints.balance);
    config.auth_method = AuthMethod::ApiKey;
    config.validate()?;
    state.credentials.set(&credential_ref, api_key)?;
    let repository = match state.repository() {
        Ok(repository) => repository,
        Err(error) => {
            // The secret was just written; without this compensation a failed
            // database open would orphan it in the OS store with no record.
            let _ = state.credentials.delete(&credential_ref);
            return Err(error);
        }
    };
    if let Err(error) = repository.upsert_provider(&config, unix_seconds()) {
        let _ = state.credentials.delete(&credential_ref);
        return Err(CommandError::Storage(error));
    }
    let account = AccountMetadata::from_auth_input(
        account_id,
        config.id.clone(),
        label.to_owned(),
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
    Ok(account)
}

fn create_account(
    state: &AppRuntime,
    input: GenericOpenAiAccountInput,
) -> Result<AccountViewModel, CommandError> {
    let resolved = resolve_new_account(&input, Some(&state.data_dir))?;
    // The consent gate runs on the validated target the account will really
    // use — never on a frontend-supplied boolean.
    authorize_lan_target(
        &state.lan_consents,
        &resolved.base_url,
        input.lan_consent_token.as_deref(),
    )?;
    let label = input.label.trim().to_owned();
    let account = persist_new_account(state, &label, &resolved, &input.api_key)?;
    let view = state
        .repository()?
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

/// Worst PRD health grade across the account's usable quota values. Only
/// snapshots the parser trusted (`Available`/`Partial`) can drive a severity;
/// unknown or failed metrics stay unmeasurable instead of implying an empty
/// quota. Last known values count regardless of snapshot age — freshness is
/// surfaced separately in the UI.
fn account_data_grade(snapshots: &[SnapshotViewDto]) -> Option<HealthGrade> {
    worst_grade(snapshots.iter().map(|view| match view.snapshot.status {
        SnapshotStatus::Available | SnapshotStatus::Partial => remaining_ratio(
            view.snapshot.remaining,
            view.snapshot.used,
            view.snapshot.limit,
        ),
        _ => None,
    }))
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
    // Error and unavailable describe connection/data reliability and win over
    // value-driven grades; a critical/attention ratio outranks plain staleness
    // because the stale state stays visible through freshness and notices.
    let status = if has_error {
        "error"
    } else if snapshots.is_empty() {
        "unavailable"
    } else {
        match account_data_grade(&view.snapshots) {
            Some(HealthGrade::Critical) => "critical",
            Some(HealthGrade::Attention) => "attention",
            _ if freshness == "stale" => "stale",
            _ => "healthy",
        }
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

    use crate::domain::provider::QuotaSnapshot;
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
            authorize_lan_target(&ledger, "https://8.8.8.8/v1", None).is_ok(),
            "public targets never need a token"
        );
        assert!(
            authorize_lan_target(&ledger, "https://192.168.0.20/v1", None).is_err(),
            "LAN targets without a confirmation are refused"
        );
        let token = ledger.begin("https://192.168.0.20/v1", unix_seconds());
        assert!(authorize_lan_target(&ledger, "https://192.168.0.20/v1", Some(&token)).is_ok());
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
