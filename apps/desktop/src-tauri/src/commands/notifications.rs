//! Notification command surface, split out of the former single-file module.

use super::{
    AppHandle, AppRuntime, Arc, CommandError, Emitter, LocalRepository, Notification,
    NotificationPreferences, OsNotificationSink, PREFERENCES_KEY, Serialize, State, StorageError,
    unix_seconds,
};

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
    pub(super) fn from_record(record: crate::storage::NotificationRecord) -> Self {
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
pub fn notifications_list(
    state: State<'_, Arc<AppRuntime>>,
    limit: Option<u32>,
) -> Result<Vec<NotificationViewModel>, CommandError> {
    let repository = state.repository()?;
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let records = repository.notifications(limit)?;
    Ok(records
        .into_iter()
        .map(NotificationViewModel::from_record)
        .collect())
}

#[tauri::command]
pub fn notifications_mark_read(
    state: State<'_, Arc<AppRuntime>>,
    ids: Option<Vec<String>>,
) -> Result<(), CommandError> {
    let repository = state.repository()?;
    let now = unix_seconds();
    let changed = match ids.as_deref() {
        Some(list) if !list.is_empty() => {
            let refs = list.iter().map(String::as_str).collect::<Vec<_>>();
            repository.mark_notifications_read(&refs, now)
        }
        _ => repository.mark_all_notifications_read(now),
    };
    changed.map(|_| ())?;
    Ok(())
}

#[tauri::command]
pub fn notification_preferences_get(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<NotificationPreferences, CommandError> {
    load_notification_preferences(&state)
}

#[tauri::command]
pub fn notification_preferences_set(
    state: State<'_, Arc<AppRuntime>>,
    prefs: NotificationPreferences,
) -> Result<(), CommandError> {
    prefs.validate().map_err(CommandError::InvalidInput)?;
    let repository = state.repository()?;
    let value = serde_json::to_value(&prefs).map_err(StorageError::from)?;
    repository.set_setting(PREFERENCES_KEY, &value)?;
    Ok(())
}

/// QA drill: sends one synthetic notification through the production enqueue
/// and OS sink chain. Only reached via the `TOKENOWL_TEST_BANNER=1` startup
/// hook; the timestamped dedup key keeps repeat drills outside the cooldown.
pub(crate) fn run_test_banner_drill(state: &AppRuntime, app: &tauri::AppHandle) {
    let repository = match state.repository() {
        Ok(repository) => repository,
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: repository unavailable");
            return;
        }
    };
    let prefs = match load_notification_preferences_from(&repository) {
        Ok(prefs) => prefs,
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: preferences unavailable");
            return;
        }
    };
    let now = unix_seconds();
    let draft = crate::notifications::Notification {
        id: format!("ntf-drill-{now}"),
        kind: crate::notifications::NotificationKind::LowQuota,
        severity: crate::notifications::NotificationSeverity::Critical,
        title: "TokenOwl 演练通知".to_owned(),
        message: "本条通知通过真实派发链路发送，用于验证系统横幅。".to_owned(),
        provider_id: None,
        account_id: None,
        dedup_key: format!("drill-{now}"),
        created_at: now,
    };
    let sink = crate::notifications::OsNotificationSink { app: app.clone() };
    match crate::notifications::enqueue_new(&repository, &sink, &prefs, vec![draft], now) {
        Ok(delivered) => {
            tracing::info!(target: "drill", delivered = delivered.len(), "banner drill dispatched");
        }
        Err(error) => {
            tracing::error!(target: "drill", error = %error, "banner drill: enqueue failed");
        }
    }
}

/// Loads persisted preferences; a missing or malformed entry means defaults.
pub(super) fn load_notification_preferences(
    state: &AppRuntime,
) -> Result<NotificationPreferences, CommandError> {
    let repository = state.repository()?;
    load_notification_preferences_from(&repository)
}

pub(super) fn load_notification_preferences_from(
    repository: &LocalRepository,
) -> Result<NotificationPreferences, CommandError> {
    let value = repository.get_setting(PREFERENCES_KEY)?;
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
pub(super) fn notify_account_events(state: &AppRuntime, handle: &AppHandle, account_id: &str) {
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
    sink: &dyn crate::notifications::NotificationSink,
    account_id: &str,
) -> Result<Vec<Notification>, CommandError> {
    let repository = state.repository()?;
    let prefs = load_notification_preferences_from(&repository)?;
    if !prefs.enabled {
        return Ok(Vec::new());
    }
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
    let candidates = crate::notifications::evaluate_account_notifications(
        crate::notifications::TriggerContext {
            account_id: &account.id,
            account_label: &account.label,
            provider_id: &account.provider_id,
            snapshots: &view.snapshots,
            refresh: &view.refresh,
            prefs: &prefs,
            now,
            pace: pace.as_ref(),
        },
    );
    let delivered = crate::notifications::enqueue_new(&repository, sink, &prefs, candidates, now)
        .map_err(CommandError::from)?;
    // A recovery notice is one-shot: drop the retained error so the next
    // successful refresh does not announce the same recovery again.
    if delivered
        .iter()
        .any(|item| item.kind == crate::notifications::NotificationKind::Recovered)
    {
        let _ = repository.clear_recovered_error(account_id, now);
    }
    Ok(delivered)
}
