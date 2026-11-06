//! Account command surface, split out of the former single-file module.

use super::{Error, Deserialize, Serialize, PathBuf, CredentialStore, OpenAiCompatibleAdapter, AnthropicAdapter, GoogleAiAdapter, FreshnessPolicy, HashMap, LanConsentRecord, LocalRepository, proxy_setting, OpenAiCompatibleConfig, StorageError, ProviderError, State, Arc, resolves_to_lan, unix_seconds, AppHandle, Emitter, refresh_account, notify_account_events, is_loopback_host, AccountMetadata, AuthMethod, load_lan_trusts, authorize_lan_target, SnapshotViewDto, HealthGrade, SnapshotStatus, worst_grade, remaining_ratio, AccountViewDto, RefreshStatus, QuotaUnit, DataSource, Freshness};

pub(super) const DEFAULT_QUOTA_ENDPOINT: &str = "/usage";

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
    pub(super) fn try_acquire(&self, account_id: &str) -> bool {
        self.held
            .lock()
            .expect("refresh gate lock")
            .insert(account_id.to_owned())
    }

    pub(super) fn release(&self, account_id: &str) {
        self.held
            .lock()
            .expect("refresh gate lock")
            .remove(account_id);
    }
}

/// Holds one account's refresh slot for the refresh task's whole lifetime.
///
/// Releasing is bound to scope instead of to a call at the end of the happy
/// path: dropping the lease — on success, on error, or through the unwind
/// of a panicking task — always frees the account, so one crashed refresh
/// can never wedge that account's gate shut forever. The lease carries only
/// the account id; no mutex guard ever lives across an `await`.
pub(super) struct RefreshLease<'a> {
    pub(super) gate: &'a InFlightGate,
    pub(super) account_id: String,
}

impl RefreshLease<'_> {
    /// Acquires the account's refresh slot, or `None` when a refresh is
    /// already in flight. The returned lease releases the slot on drop.
    pub(super) fn acquire<'a>(
        gate: &'a InFlightGate,
        account_id: &str,
    ) -> Option<RefreshLease<'a>> {
        gate.try_acquire(account_id).then(|| RefreshLease {
            gate,
            account_id: account_id.to_owned(),
        })
    }
}

impl Drop for RefreshLease<'_> {
    fn drop(&mut self) {
        self.gate.release(&self.account_id);
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
    pub(super) issued: std::sync::Arc<std::sync::Mutex<HashMap<String, LanConsentRecord>>>,
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
        // The proxy URL (which may embed proxy credentials) lives in the OS
        // credential store under its own reference, so it never lands in
        // SQLite or a log. A keyring failure leaves the transport direct —
        // availability beats guessing at a half-read secret.
        let credentials = CredentialStore::default();
        let proxy = proxy_setting(&credentials);
        let adapter =
            OpenAiCompatibleAdapter::with_proxy(proxy.clone(), OpenAiCompatibleConfig::default())
                .map_err(|error| error.to_string())?;
        let anthropic =
            AnthropicAdapter::with_proxy(proxy.clone(), OpenAiCompatibleConfig::default())
                .map_err(|error| error.to_string())?;
        let google = GoogleAiAdapter::with_proxy(proxy, OpenAiCompatibleConfig::default())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            database_path,
            credentials,
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

    pub(super) fn repository(&self) -> Result<LocalRepository, CommandError> {
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

impl CommandError {
    /// Stable, programmable category carried in every IPC error payload.
    /// The human-facing text stays in `Display`; the frontend branches on
    /// this code (e.g. `AuthExpired` offers the key-update action).
    pub(super) fn code(&self) -> &'static str {
        match self {
            CommandError::Storage(_) => "Storage",
            CommandError::Provider(error) => error.code.name(),
            CommandError::Credential(_) => "Credential",
            CommandError::InvalidInput(_) => "InvalidInput",
        }
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CommandError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
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
    /// Wire format of a CUSTOM provider target: `openai_compatible` (the
    /// default), `anthropic`, or `gemini`. Built-in provider ids carry
    /// their format in the manifest and ignore this field.
    #[serde(default)]
    pub api_format: Option<String>,
}

impl std::fmt::Debug for GenericOpenAiAccountInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // This DTO carries the raw API key across IPC; a derived `Debug`
        // would print it into any log or error path that formats the input.
        // Only field presence is safe to show.
        formatter
            .debug_struct("GenericOpenAiAccountInput")
            .field("provider_id", &self.provider_id)
            .field("api_format", &self.api_format)
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
    /// Capability level of the provider's embedded manifest (L1..L4), `None`
    /// for custom accounts. Lets the UI explain an always-empty account as
    /// "no key-level quota published" instead of a failure.
    pub capability_level: Option<String>,
}

#[tauri::command]
pub fn accounts_list(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<Vec<AccountViewModel>, CommandError> {
    list_accounts(&state)
}

#[tauri::command]
pub async fn lan_consent_begin(
    base_url: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<Option<String>, CommandError> {
    let trimmed = base_url.trim().to_owned();
    // Classification can hit DNS, so keep the blocking work off the async
    // runtime's workers.
    let target = trimmed.clone();
    let needs_consent = tauri::async_runtime::spawn_blocking(move || resolves_to_lan(&target))
        .await
        .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
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
) -> Result<AccountViewModel, CommandError> {
    let account = create_account(&state, input)?;
    let account_id = account.id.clone();
    // Best-effort first snapshot. A failed request keeps the account plus an explicit
    // refresh error state instead of dropping the account or inventing values.
    if let Err(error) = refresh_accounts(state.inner(), Some(&app), Some(account_id.clone())).await
    {
        tracing::warn!("initial refresh failed for account {account_id}: {error}");
    }
    list_accounts(&state)?
        .into_iter()
        .find(|item| item.id == account_id)
        .ok_or_else(|| CommandError::InvalidInput("created account could not be read".to_owned()))
}

/// Overwrites an account's API key and validates it with an immediate
/// refresh of that account.
///
/// The keyring entry is replaced under the account's existing credential
/// reference, so the account, provider row and history are untouched — a
/// revoked or expired key recovers without the destructive delete-and-recreate
/// path. Validation is the refresh itself: a rejected key surfaces in the
/// returned view model as the account's error state, and only keyring or
/// lookup failures become command errors here.
#[tauri::command]
pub async fn accounts_update_credential(
    app: AppHandle,
    account_id: String,
    api_key: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<AccountViewModel, CommandError> {
    update_credential(&state, &account_id, &api_key)?;
    if let Err(error) = refresh_accounts(state.inner(), Some(&app), Some(account_id.clone())).await
    {
        tracing::warn!("post-update refresh failed for account {account_id}: {error}");
    }
    list_accounts(&state)?
        .into_iter()
        .find(|item| item.id == account_id)
        .ok_or_else(|| CommandError::InvalidInput("updated account could not be read".to_owned()))
}

#[tauri::command]
pub fn accounts_delete(
    account_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    let repository = state.repository()?;
    let account = repository
        .account(&account_id)?
        .ok_or_else(|| CommandError::InvalidInput("account not found".to_owned()))?;
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
    repository.delete_account(&account_id)?;
    // The bubble would keep drawing the removed account's rings until the
    // next scheduler tick; tell it the account set changed now.
    let _ = app.emit("quota:updated", ());
    Ok(())
}

/// Settings key holding credential deletions that failed and must be retried.
pub(super) const CLEANUP_SETTING_KEY: &str = "credential_cleanup_pending";

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
pub(super) fn cleanup_error_code(error: &crate::credentials::CredentialError) -> &'static str {
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

/// Rolls a just-written credential back after its account could not be
/// persisted (the create path's compensation half).
///
/// A successful delete ends the lifecycle. A failed delete is recorded in
/// the persistent cleanup queue when a repository is available, and only
/// logged — with the reference, the failed stage, and a stable error code,
/// never any secret — when it is not. Logging "deleted" here without proof
/// would orphan the secret silently; the queue is what makes the startup
/// sweep able to finish the job.
///
/// The delete operation is injected so tests can exercise every outcome
/// without touching a real OS credential store.
pub(super) fn compensate_credential_failure(
    delete: impl FnOnce(&str) -> Result<(), crate::credentials::CredentialError>,
    repository: Option<&LocalRepository>,
    credential_ref: &str,
    stage: &'static str,
) {
    if let Err(error) = delete(credential_ref) {
        let code = cleanup_error_code(&error);
        match repository {
            Some(repository) => {
                if let Err(record_error) =
                    record_pending_cleanup(repository, credential_ref, code, unix_seconds())
                {
                    tracing::warn!(
                        reference = %credential_ref,
                        stage,
                        code,
                        record_error = %record_error,
                        "credential rollback delete failed and the cleanup queue rejected the entry"
                    );
                }
            }
            None => {
                tracing::warn!(
                    reference = %credential_ref,
                    stage,
                    code,
                    "credential rollback delete failed and no writable storage exists to queue it"
                );
            }
        }
    }
}

pub(super) fn load_pending_cleanups(
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
) -> Result<Vec<AccountViewModel>, CommandError> {
    refresh_accounts(state.inner(), Some(&app), account_id).await
}

/// Accounts refreshed concurrently per wave. Bounded on purpose: unbounded
/// fan-out would trip provider rate limits, and a quota monitor has nothing
/// to gain from hammering every account at once.
pub(super) const REFRESH_WAVE_SIZE: usize = 4;

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
    let mut single_failure: Option<CommandError> = None;
    for wave in account_ids.chunks(REFRESH_WAVE_SIZE) {
        let mut tasks = Vec::with_capacity(wave.len());
        for account_id in wave {
            let runtime = Arc::clone(runtime);
            let account_id = account_id.to_owned();
            let handle = handle.cloned();
            tasks.push(tauri::async_runtime::spawn(async move {
                // The lease owns the slot from acquisition to the end of the
                // task — success, error, or panic all release it on drop.
                let Some(_lease) = RefreshLease::acquire(&runtime.refresh_gate, &account_id) else {
                    // Merged into an in-flight refresh; its outcome applies.
                    return Ok(account_id);
                };
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
                // The lease releases the gate when it drops at task end —
                // no explicit release to forget on a new early-return path.
                let failed_id = account_id.clone();
                outcome.map(|()| account_id).map_err(|error| {
                    // The stable code drives the log line; the full error
                    // travels too so a single-account request can surface it.
                    // Display strings are built from kinds, statuses, and
                    // constants — provider response bodies never enter them.
                    let code = match &error {
                        CommandError::Provider(provider) => provider.code.name().to_owned(),
                        CommandError::Storage(_) => "Storage".to_owned(),
                        _ => "CommandFailed".to_owned(),
                    };
                    (failed_id, code, error)
                })
            }));
        }
        for task in tasks {
            match task.await {
                Ok(Ok(_)) => {}
                Ok(Err((account_id, code, error))) => {
                    tracing::warn!(account = %account_id, code = %code, "refresh failed");
                    failed_requested |= refresh_one;
                    // A single-account request surfaces the original failure
                    // so its stable code reaches the webview (the expired-key
                    // branch keys on it); multi-account waves only report
                    // through the snapshot event.
                    if refresh_one {
                        single_failure = Some(error);
                    }
                }
                Err(join_error) => {
                    tracing::warn!("refresh task joined with error: {join_error}");
                    failed_requested |= refresh_one;
                }
            }
        }
    }
    if refresh_one && failed_requested {
        return Err(single_failure.unwrap_or_else(|| {
            CommandError::InvalidInput("failed to refresh account".to_owned())
        }));
    }
    // The bubble is a separate webview and never sees this command's return
    // value; error snapshots and account changes reach it only through this
    // event, so it must fire on the manual and first-refresh paths too — not
    // just on the scheduler and hotkey callers that emit on their own.
    if let Some(handle) = handle {
        let _ = handle.emit("quota:updated", ());
    }
    list_accounts(runtime)
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
) -> Result<ManifestProfilesView, CommandError> {
    let (profiles, skipped) =
        crate::providers::builtin::all_manifest_profiles(Some(&state.data_dir));
    Ok(ManifestProfilesView { profiles, skipped })
}

/// The `docs/09` provider capability catalog: every planned provider with its
/// account types, units, and windows; implemented providers carry their
/// capability level from the embedded manifest.
#[tauri::command]
pub fn provider_matrix() -> Result<Vec<crate::providers::builtin::ProviderMatrixRow>, CommandError>
{
    Ok(crate::providers::builtin::provider_matrix())
}

pub(super) fn list_accounts(state: &AppRuntime) -> Result<Vec<AccountViewModel>, CommandError> {
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
pub(super) struct AccountEndpoints {
    pub(super) models: Option<String>,
    pub(super) quota: Option<String>,
    pub(super) usage: Option<String>,
    pub(super) balance: Option<String>,
}

/// Everything account persistence needs, resolved from either the generic
/// OpenAI-compatible path or a built-in provider manifest.
#[derive(Debug)]
pub(super) struct ResolvedNewAccount {
    /// Brand prefix for the per-account provider id (`deepseek:account-…`).
    pub(super) provider_prefix: String,
    pub(super) display_name: String,
    pub(super) base_url: String,
    pub(super) endpoints: AccountEndpoints,
    /// The credential header the resolved wire format authenticates with.
    pub(super) api_key_header: &'static str,
    /// Wire protocol the account speaks in the config vocabulary
    /// (`anthropic` / `google-ai`); `None` means OpenAI-compatible. Persisted
    /// on the config so refresh routing never re-derives it from the id.
    pub(super) api_format: Option<String>,
}

/// Normalizes a creation-time wire format onto the config vocabulary: native
/// formats keep their canonical spelling, and the OpenAI-compatible default
/// (plus anything unrecognized) stores as `None`.
pub(super) fn config_api_format(api_format: &str) -> Option<String> {
    match crate::domain::provider::WireFormat::parse(api_format) {
        Some(crate::domain::provider::WireFormat::OpenAiCompatible) | None => None,
        Some(format) => Some(format.as_str().to_owned()),
    }
}

/// The credential header each supported wire format authenticates with.
/// Unrecognized formats ride the OpenAI-compatible transport and its
/// `Authorization` header.
pub(super) fn api_key_header_for_format(api_format: &str) -> &'static str {
    crate::domain::provider::WireFormat::parse(api_format).map_or(
        "Authorization",
        crate::domain::provider::WireFormat::api_key_header,
    )
}

pub(super) fn resolve_new_account(
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
        let base_url = validate_user_base_url(&input.base_url)?;
        let api_format = input.api_format.as_deref().unwrap_or("openai_compatible");
        return match api_format {
            // The probing format: optional user-named quota/usage/balance
            // endpoints are honored alongside the default quota probe.
            "openai_compatible" => {
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
                Ok(ResolvedNewAccount {
                    provider_prefix: "generic.openai_compatible".to_owned(),
                    display_name: "OpenAI-compatible".to_owned(),
                    base_url,
                    endpoints: AccountEndpoints {
                        models: None,
                        quota,
                        usage,
                        balance,
                    },
                    api_key_header: api_key_header_for_format(api_format),
                    api_format: None,
                })
            }
            // The native wire formats dispatch to their own adapters by the
            // config's `api_format` field. Their capability is the adapter's
            // own (L2: key probe plus any rate-limit headers the gateway
            // returns), so user-supplied probe endpoints are not applicable
            // and are deliberately ignored.
            "anthropic" | "gemini" => Ok(ResolvedNewAccount {
                provider_prefix: api_format.to_owned(),
                display_name: if api_format == "anthropic" {
                    "Anthropic-compatible".to_owned()
                } else {
                    "Gemini-compatible".to_owned()
                },
                base_url,
                endpoints: AccountEndpoints {
                    models: None,
                    quota: None,
                    usage: None,
                    balance: None,
                },
                api_key_header: api_key_header_for_format(api_format),
                api_format: config_api_format(api_format),
            }),
            other => Err(CommandError::InvalidInput(format!(
                "unsupported API format `{other}` (openai_compatible, anthropic, gemini)"
            ))),
        };
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
        api_key_header: api_key_header_for_format(&profile.api_format),
        api_format: config_api_format(&profile.api_format),
    })
}

/// Validates a base URL the user typed (custom provider or gateway profile).
///
/// Absolute http(s) with a host. Credentials embedded in the URL are rejected
/// because the base URL is persisted to `SQLite`, which must never hold secrets.
pub(super) fn validate_user_base_url(base_url: &str) -> Result<String, CommandError> {
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

/// Unpredictable, collision-free account identifier.
///
/// The wall clock is not a uniqueness source: two concurrent creates inside
/// the same millisecond used to produce identical ids — and therefore
/// identical provider ids and credential references, racing each other in
/// the keyring and the database. A nanoid gives every creation its own
/// namespace without depending on timing.
pub(super) fn generate_account_id() -> String {
    format!("account-{}", nanoid::nanoid!(12))
}

/// The three names a new account registers under — the account id and the
/// provider id and credential reference derived from it, so they cannot
/// drift apart.
pub(super) fn new_account_identity(provider_prefix: &str) -> (String, String, String) {
    let account_id = generate_account_id();
    let provider_id = format!("{provider_prefix}:{account_id}");
    let credential_ref = format!("account/{account_id}");
    (account_id, provider_id, credential_ref)
}

pub(super) fn persist_new_account(
    state: &AppRuntime,
    label: &str,
    resolved: &ResolvedNewAccount,
    api_key: &str,
) -> Result<AccountMetadata, CommandError> {
    let (account_id, provider_id, credential_ref) = new_account_identity(&resolved.provider_prefix);
    let mut config = OpenAiCompatibleConfig::new(
        provider_id,
        resolved.display_name.clone(),
        resolved.base_url.clone(),
    );
    config.credential_ref = Some(credential_ref.clone());
    resolved
        .api_key_header
        .clone_into(&mut config.api_key_header);
    config.api_format.clone_from(&resolved.api_format);
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
    // From here on the secret exists in the OS store, so every failure path
    // must attempt a rollback and, when the rollback itself fails, leave a
    // durable trace the startup sweep can act on.
    let rollback = |reference: &str| state.credentials.delete(reference);
    let repository = match state.repository() {
        Ok(repository) => repository,
        Err(error) => {
            // Without a database there is nowhere to queue a failed delete;
            // the limitation is logged, not claimed as cleaned.
            compensate_credential_failure(rollback, None, &credential_ref, "repository_open");
            return Err(error);
        }
    };
    if let Err(error) = repository.upsert_provider(&config, unix_seconds()) {
        compensate_credential_failure(
            rollback,
            Some(&repository),
            &credential_ref,
            "provider_upsert",
        );
        return Err(CommandError::Storage(error));
    }
    let account = AccountMetadata::from_auth_input(
        account_id,
        config.id.clone(),
        label.to_owned(),
        "api_key",
        Some(credential_ref.clone()),
        HashMap::new(),
        true,
        None,
    );
    if let Err(error) = repository.upsert_account(&account, unix_seconds()) {
        compensate_credential_failure(
            rollback,
            Some(&repository),
            &credential_ref,
            "account_upsert",
        );
        return Err(CommandError::Storage(error));
    }
    Ok(account)
}

pub(super) fn create_account(
    state: &AppRuntime,
    input: GenericOpenAiAccountInput,
) -> Result<AccountViewModel, CommandError> {
    let resolved = resolve_new_account(&input, Some(&state.data_dir))?;
    // The consent gate runs on the validated target the account will really
    // use — never on a frontend-supplied boolean. Targets the user has
    // explicitly trusted skip the per-creation prompt; the trust list is
    // revocable in the settings page and the one-time token stays the only
    // path for everything unlisted.
    let trusted = load_lan_trusts(&state.repository()?)?;
    authorize_lan_target(
        &state.lan_consents,
        &trusted,
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

/// Writes a replacement secret under an account's existing credential
/// reference, keeping the account, provider row and history untouched.
///
/// The setter is injected so tests can exercise the guard rails without a
/// real OS credential store.
pub(super) fn update_credential_in(
    repository: &LocalRepository,
    set_secret: impl FnOnce(&str, &str) -> Result<(), crate::credentials::CredentialError>,
    account_id: &str,
    api_key: &str,
) -> Result<(), CommandError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(CommandError::InvalidInput(
            "api key must not be empty".to_owned(),
        ));
    }
    let account = repository
        .account(account_id)?
        .ok_or_else(|| CommandError::InvalidInput("account not found".to_owned()))?;
    let reference = account.credential_ref.ok_or_else(|| {
        CommandError::InvalidInput("account does not store a keyring credential".to_owned())
    })?;
    set_secret(&reference, key).map_err(CommandError::from)
}

pub(super) fn update_credential(
    state: &AppRuntime,
    account_id: &str,
    api_key: &str,
) -> Result<(), CommandError> {
    let repository = state.repository()?;
    let credentials = state.credentials.clone();
    update_credential_in(
        &repository,
        |reference, secret| credentials.set(reference, secret),
        account_id,
        api_key,
    )
}

pub(super) fn normalize_endpoint(value: &str) -> Result<String, CommandError> {
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
pub(super) fn account_data_grade(snapshots: &[SnapshotViewDto]) -> Option<HealthGrade> {
    // A negative balance is a hard failure even when the provider reports no
    // limit to form a ratio: the account cannot pay for the next request.
    // DeepSeek reports exactly this shape (`total_balance: "-0.23"` with
    // `is_available: false`), and calling it healthy would hide the problem.
    if snapshots.iter().any(|view| {
        matches!(
            view.snapshot.status,
            SnapshotStatus::Available | SnapshotStatus::Partial
        ) && view
            .snapshot
            .remaining
            .is_some_and(|remaining| remaining < 0.0)
    }) {
        return Some(HealthGrade::Critical);
    }
    worst_grade(snapshots.iter().map(|view| match view.snapshot.status {
        SnapshotStatus::Available | SnapshotStatus::Partial => remaining_ratio(
            view.snapshot.remaining,
            view.snapshot.used,
            view.snapshot.limit,
        ),
        _ => None,
    }))
}

pub(super) fn account_view_to_model(view: AccountViewDto) -> AccountViewModel {
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
        capability_level: crate::providers::builtin::builtin_capability_level(
            &view.account.provider_id,
        ),
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

pub(super) fn snapshot_to_model(view: &SnapshotViewDto) -> QuotaViewModel {
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

pub(super) fn quota_unit(unit: Option<&QuotaUnit>) -> String {
    unit.map_or_else(
        || "unknown".to_owned(),
        |value| format!("{value:?}").to_lowercase(),
    )
}

pub(super) fn source_name(source: DataSource) -> String {
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

pub(super) fn freshness_name(freshness: Freshness) -> String {
    format!("{freshness:?}").to_lowercase()
}

pub(super) fn snapshot_status(status: SnapshotStatus) -> String {
    format!("{status:?}").to_lowercase()
}
