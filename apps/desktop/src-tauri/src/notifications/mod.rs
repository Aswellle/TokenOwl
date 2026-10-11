//! Notification trigger pipeline: threshold evaluation, dedup, and dispatch.
//!
//! Triggers per `docs/01` §12: low quota (remaining below the user threshold),
//! balance below a user-set floor, an approaching quota reset, expired
//! authentication, and a provider that stays unreachable across refresh
//! attempts. Evaluation is a pure function over the last known snapshot view;
//! delivery goes through `SQLite` history first (which carries the dedup
//! cooldown) and then to a [`NotificationSink`] so the OS banner and the
//! in-app log can never diverge. Secrets never enter a notification: drafts
//! are built from normalized values and account labels only.

use crate::domain::health::{CRITICAL_RATIO, remaining_ratio};
use crate::domain::provider::{QuotaSnapshot, SnapshotStatus};
use crate::domain::state::{RefreshState, RefreshStatus};
use crate::storage::{LocalRepository, NotificationRecord, SnapshotViewDto, StorageError};
use serde::{Deserialize, Serialize};

/// A notification must not repeat for the same condition inside this window.
pub const DEDUP_COOLDOWN_SECS: u64 = 86_400;
/// Notification history older than this is pruned on every enqueue.
pub const RETENTION_SECS: u64 = 30 * 86_400;
/// Hard cap on stored history rows regardless of age.
pub const MAX_HISTORY_ROWS: i64 = 200;
/// A provider is "long disconnected" after this many consecutive refresh
/// failures with a connectivity-shaped error code.
const UNREACHABLE_AFTER_FAILURES: u32 = 3;
/// Settings key under which [`NotificationPreferences`] is persisted.
pub const PREFERENCES_KEY: &str = "notification_preferences";

/// Notification severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

impl NotificationSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn from_str_value(value: &str) -> Option<Self> {
        match value {
            "INFO" => Some(Self::Info),
            "WARNING" => Some(Self::Warning),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// What kind of condition produced a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    LowQuota,
    ResetApproaching,
    AuthExpired,
    ProviderUnreachable,
    BalanceBelow,
}

impl NotificationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LowQuota => "low_quota",
            Self::ResetApproaching => "reset_approaching",
            Self::AuthExpired => "auth_expired",
            Self::ProviderUnreachable => "provider_unreachable",
            Self::BalanceBelow => "balance_below",
        }
    }

    pub fn from_str_value(value: &str) -> Option<Self> {
        match value {
            "low_quota" => Some(Self::LowQuota),
            "reset_approaching" => Some(Self::ResetApproaching),
            "auth_expired" => Some(Self::AuthExpired),
            "provider_unreachable" => Some(Self::ProviderUnreachable),
            "balance_below" => Some(Self::BalanceBelow),
            _ => None,
        }
    }
}

/// A single notification, normalized and safe to persist or emit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub kind: NotificationKind,
    pub severity: NotificationSeverity,
    pub title: String,
    pub message: String,
    pub provider_id: Option<String>,
    pub account_id: Option<String>,
    /// Stable condition identity used by the dedup cooldown.
    pub dedup_key: String,
    pub created_at: u64,
}

/// Account scope for building a draft: whom the notification is about and
/// when it was observed. Kept separate from the rule inputs so a draft call
/// stays readable.
struct DraftScope<'a> {
    account_id: &'a str,
    provider_id: &'a str,
    now: u64,
}

impl<'a> DraftScope<'a> {
    fn of(context: &TriggerContext<'a>) -> Self {
        Self {
            account_id: context.account_id,
            provider_id: context.provider_id,
            now: context.now,
        }
    }
}

impl Notification {
    fn draft(
        scope: &DraftScope<'_>,
        kind: NotificationKind,
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
        dedup_key: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("ntf-{}", nanoid::nanoid!(10)),
            kind,
            severity,
            title: title.into(),
            message: message.into(),
            provider_id: Some(scope.provider_id.to_owned()),
            account_id: Some(scope.account_id.to_owned()),
            dedup_key: dedup_key.into(),
            created_at: scope.now,
        }
    }
}

impl From<Notification> for NotificationRecord {
    fn from(notification: Notification) -> Self {
        Self {
            id: notification.id,
            account_id: notification.account_id,
            provider_id: notification.provider_id,
            kind: notification.kind.as_str().to_owned(),
            severity: notification.severity.as_str().to_owned(),
            title: notification.title,
            message: notification.message,
            dedup_key: notification.dedup_key,
            created_at: notification.created_at,
            read_at: None,
        }
    }
}

impl From<NotificationRecord> for Notification {
    fn from(record: NotificationRecord) -> Self {
        Self {
            id: record.id,
            kind: NotificationKind::from_str_value(&record.kind)
                .unwrap_or(NotificationKind::LowQuota),
            severity: NotificationSeverity::from_str_value(&record.severity)
                .unwrap_or(NotificationSeverity::Info),
            title: record.title,
            message: record.message,
            provider_id: record.provider_id,
            account_id: record.account_id,
            dedup_key: record.dedup_key,
            created_at: record.created_at,
        }
    }
}

/// User preferences for the notification pipeline. Persisted as JSON under
/// [`PREFERENCES_KEY`]; missing or malformed entries fall back to defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationPreferences {
    /// Master switch. When off, nothing is evaluated or stored.
    pub enabled: bool,
    /// Keep history but never raise the OS banner.
    pub silent_mode: bool,
    /// Raise the OS banner only for [`NotificationSeverity::Critical`].
    pub critical_only: bool,
    /// Low-quota warning boundary in percent remaining (`docs/01` §12: 20%).
    pub low_quota_threshold: f64,
    /// Warn when a balance metric drops below this value; `None` disables it.
    pub balance_threshold: Option<f64>,
    /// How long before a reset the "reset approaching" notice fires, in seconds.
    pub reset_soon_window: u64,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            silent_mode: false,
            critical_only: false,
            low_quota_threshold: 20.0,
            balance_threshold: None,
            reset_soon_window: 1_800,
        }
    }
}

impl NotificationPreferences {
    /// Bounds user-supplied values before persistence; rejects rather than
    /// clamps so the settings UI can surface the problem.
    pub fn validate(&self) -> Result<(), String> {
        if !self.low_quota_threshold.is_finite()
            || !(1.0..=99.0).contains(&self.low_quota_threshold)
        {
            return Err("低额度阈值需在 1–99 之间。".to_owned());
        }
        if let Some(threshold) = self.balance_threshold {
            if !threshold.is_finite() || threshold < 0.0 {
                return Err("余额下限不能为负数。".to_owned());
            }
        }
        if !(300..=86_400).contains(&self.reset_soon_window) {
            return Err("重置提醒需在 5 分钟到 24 小时之间。".to_owned());
        }
        Ok(())
    }
}

/// Everything the pure evaluator needs. `snapshots` carries the last known
/// views (expired views are already filtered upstream); unknown values stay
/// unknown and never trigger.
pub struct TriggerContext<'a> {
    pub account_id: &'a str,
    pub account_label: &'a str,
    pub provider_id: &'a str,
    pub snapshots: &'a [SnapshotViewDto],
    pub refresh: &'a RefreshState,
    pub prefs: &'a NotificationPreferences,
    pub now: u64,
}

/// Evaluates every notification rule for one account. Returns at most one
/// draft per condition; the dedup cooldown lives in storage, not here.
pub fn evaluate_account_notifications(context: TriggerContext<'_>) -> Vec<Notification> {
    if !context.prefs.enabled {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    // An expired credential blocks any fresh data, so it replaces the
    // value-driven rules for this cycle instead of stacking on stale values.
    if let Some(auth) = auth_expired_draft(&context) {
        candidates.push(auth);
        return candidates;
    }
    if let Some(unreachable) = provider_unreachable_draft(&context) {
        candidates.push(unreachable);
    }
    if let Some(low_quota) = low_quota_draft(&context) {
        candidates.push(low_quota);
    }
    if let Some(balance) = balance_below_draft(&context) {
        candidates.push(balance);
    }
    if let Some(reset) = reset_approaching_draft(&context) {
        candidates.push(reset);
    }
    candidates
}

/// The credential was rejected: notify instead of re-reporting stale values.
fn auth_expired_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let code = context.refresh.error_code.as_deref()?;
    if context.refresh.status != RefreshStatus::Failed
        || !matches!(code, "AuthRequired" | "AuthExpired")
    {
        return None;
    }
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::AuthExpired,
        NotificationSeverity::Critical,
        "认证已过期",
        format!(
            "「{}」的 API 密钥已被拒绝，请更新凭据后重试。",
            context.account_label
        ),
        format!("auth-expired:{}", context.account_id),
    ))
}

/// The provider stayed unreachable across repeated refresh attempts.
fn provider_unreachable_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let code = context.refresh.error_code.as_deref()?;
    if context.refresh.status != RefreshStatus::Failed
        || context.refresh.consecutive_failures < UNREACHABLE_AFTER_FAILURES
        || !is_unreachable_code(code)
    {
        return None;
    }
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::ProviderUnreachable,
        NotificationSeverity::Warning,
        "长时间无法连接",
        format!(
            "「{}」已连续 {} 次刷新失败，请检查网络或服务状态。",
            context.account_label, context.refresh.consecutive_failures
        ),
        format!("provider-unreachable:{}", context.account_id),
    ))
}

/// The worst trusted remaining ratio crossed the user's warning boundary;
/// below the global critical boundary the severity escalates.
fn low_quota_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let (view, ratio) = worst_trusted_ratio(context.snapshots)?;
    let threshold = context.prefs.low_quota_threshold / 100.0;
    if ratio >= threshold {
        return None;
    }
    let severity = if ratio < CRITICAL_RATIO {
        NotificationSeverity::Critical
    } else {
        NotificationSeverity::Warning
    };
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::LowQuota,
        severity,
        "额度紧张",
        format!(
            "「{}」的{}仅剩 {:.0}%。",
            context.account_label,
            metric_label(&view.snapshot),
            ratio * 100.0
        ),
        format!("low-quota:{}", context.account_id),
    ))
}

/// A balance metric dropped below the user-configured floor.
fn balance_below_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let floor = context.prefs.balance_threshold?;
    let (view, remaining) = balance_below(context.snapshots, floor)?;
    let value = match view.snapshot.currency.as_deref() {
        Some(currency) => format!("{currency}{remaining:.2}"),
        None => format!("{remaining:.2}"),
    };
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::BalanceBelow,
        NotificationSeverity::Warning,
        "余额不足",
        format!(
            "「{}」的余额 {} 已低于设定的下限 {}。",
            context.account_label, value, floor
        ),
        format!("balance-below:{}", context.account_id),
    ))
}

/// A quota reset happens inside the configured lead time. The reset timestamp
/// is part of the dedup key, so each cycle reminds exactly once.
fn reset_approaching_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let (reset_at, view) = soonest_reset_within(
        context.snapshots,
        context.prefs.reset_soon_window,
        context.now,
    )?;
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::ResetApproaching,
        NotificationSeverity::Info,
        "额度即将重置",
        format!(
            "「{}」的{}将在约 {} 后重置。",
            context.account_label,
            metric_label(&view.snapshot),
            humanize_countdown(reset_at.saturating_sub(context.now))
        ),
        format!("reset-soon:{}:{reset_at}", context.account_id),
    ))
}

/// Persists candidate notifications with the dedup cooldown and dispatches the
/// ones that were actually inserted to `sink`. Returns the delivered set so
/// the caller can emit the enqueue event.
pub fn enqueue_new(
    repository: &LocalRepository,
    sink: &dyn NotificationSink,
    prefs: &NotificationPreferences,
    candidates: Vec<Notification>,
    now: u64,
) -> Result<Vec<Notification>, StorageError> {
    let mut delivered = Vec::new();
    for candidate in candidates {
        let mut record = NotificationRecord::from(candidate.clone());
        record.created_at = now;
        if !repository.insert_notification(record, DEDUP_COOLDOWN_SECS, now)? {
            continue;
        }
        let mut delivered_notification = candidate;
        delivered_notification.created_at = now;
        let audible = !prefs.silent_mode
            && (!prefs.critical_only
                || delivered_notification.severity == NotificationSeverity::Critical);
        if audible {
            sink.show(&delivered_notification);
        }
        delivered.push(delivered_notification);
    }
    repository.prune_notifications(now, RETENTION_SECS, MAX_HISTORY_ROWS)?;
    Ok(delivered)
}

/// Destination for notifications that passed the filter. Implementations must
/// not panic; a failed OS banner must not break the refresh loop.
pub trait NotificationSink: Send + Sync {
    fn show(&self, notification: &Notification);
}

/// Test/logging sink: records the event without touching the OS.
#[derive(Debug, Default)]
pub struct LogSink;

impl NotificationSink for LogSink {
    fn show(&self, notification: &Notification) {
        // Account labels are user-provided names, not secrets, but the log
        // stays minimal anyway: kind and severity are enough to audit flow.
        tracing::info!(
            target: "notifications",
            kind = notification.kind.as_str(),
            severity = notification.severity.as_str(),
            "notification dispatched"
        );
    }
}

/// Dispatches through the Tauri notification plugin. Called only from the
/// Rust refresh path; the webview holds no notification permission.
pub struct OsNotificationSink {
    pub app: tauri::AppHandle,
}

impl NotificationSink for OsNotificationSink {
    fn show(&self, notification: &Notification) {
        use tauri_plugin_notification::{NotificationExt, PermissionState};
        // macOS gates delivery behind the plugin's authorization surface, so
        // the dispatch path asks before sending. In tauri-plugin-notification
        // 2.4 the desktop implementation is a `Granted` stub — the real
        // first-run prompt comes from the legacy NSUserNotification path
        // auto-registering the app on first delivery — so on Windows and
        // Linux this check is a constant no-op that keeps the verified
        // dispatch byte-identical while making the path correct the day the
        // plugin implements desktop authorization.
        match self.app.notification().request_permission() {
            Ok(PermissionState::Granted) => {}
            Ok(state) => {
                tracing::warn!(
                    target: "notifications",
                    state = ?state,
                    "notification not dispatched: permission not granted"
                );
                return;
            }
            Err(error) => {
                tracing::warn!(
                    target: "notifications",
                    "notification permission check failed: {error}"
                );
                return;
            }
        }
        let _ = self
            .app
            .notification()
            .builder()
            .title(&notification.title)
            .body(&notification.message)
            .show();
    }
}

fn is_unreachable_code(code: &str) -> bool {
    matches!(
        code,
        "NetworkUnavailable" | "Timeout" | "TlsError" | "ProviderUnavailable"
    )
}

/// Worst remaining ratio across snapshots the parser actually trusted.
fn worst_trusted_ratio(snapshots: &[SnapshotViewDto]) -> Option<(&SnapshotViewDto, f64)> {
    snapshots
        .iter()
        .filter(|view| {
            matches!(
                view.snapshot.status,
                SnapshotStatus::Available | SnapshotStatus::Partial
            )
        })
        .filter_map(|view| {
            remaining_ratio(
                view.snapshot.remaining,
                view.snapshot.used,
                view.snapshot.limit,
            )
            .map(|ratio| (view, ratio))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

fn balance_below(snapshots: &[SnapshotViewDto], floor: f64) -> Option<(&SnapshotViewDto, f64)> {
    snapshots
        .iter()
        .filter(|view| {
            matches!(
                view.snapshot.status,
                SnapshotStatus::Available | SnapshotStatus::Partial
            )
        })
        .filter(|view| is_balance_metric(&view.snapshot))
        .filter_map(|view| view.snapshot.remaining.map(|remaining| (view, remaining)))
        .find(|(_, remaining)| *remaining < floor)
}

fn is_balance_metric(snapshot: &QuotaSnapshot) -> bool {
    snapshot.metric_id.as_deref() == Some("balance")
        || snapshot
            .metric_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("balance"))
}

fn soonest_reset_within(
    snapshots: &[SnapshotViewDto],
    window_secs: u64,
    now: u64,
) -> Option<(u64, &SnapshotViewDto)> {
    snapshots
        .iter()
        .filter(|view| {
            matches!(
                view.snapshot.status,
                SnapshotStatus::Available | SnapshotStatus::Partial
            )
        })
        .filter_map(|view| reset_timestamp(&view.snapshot).map(|reset_at| (reset_at, view)))
        .filter(|(reset_at, _)| *reset_at > now && reset_at - now <= window_secs)
        .min_by_key(|(reset_at, _)| *reset_at)
}

/// Reset instant of a snapshot: the window's `reset_at`, the snapshot-level
/// `reset_at`, or the rolling window's `ends_at`, parsed as RFC 3339.
fn reset_timestamp(snapshot: &QuotaSnapshot) -> Option<u64> {
    let window = snapshot.window.as_ref();
    window
        .and_then(|value| value.reset_at.as_deref())
        .or(snapshot.reset_at.as_deref())
        .or(window.and_then(|value| value.ends_at.as_deref()))
        .and_then(parse_unix_seconds)
}

fn parse_unix_seconds(value: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    u64::try_from(parsed.timestamp()).ok()
}

fn metric_label(snapshot: &QuotaSnapshot) -> &str {
    match snapshot.metric_id.as_deref() {
        Some("quota") => "额度",
        Some("usage") => "用量",
        Some("balance") => "余额",
        _ => snapshot.metric_name.as_deref().unwrap_or("额度"),
    }
}

fn humanize_countdown(seconds: u64) -> String {
    if seconds >= 3_600 {
        let hours = seconds / 3_600;
        let minutes = (seconds % 3_600) / 60;
        if minutes > 0 {
            format!("{hours} 小时 {minutes} 分")
        } else {
            format!("{hours} 小时")
        }
    } else if seconds >= 60 {
        format!("{} 分钟", seconds / 60)
    } else {
        format!("{seconds} 秒")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::state::Freshness;
    use crate::domain::state::RefreshState;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn snapshot_view(
        remaining: Option<f64>,
        used: Option<f64>,
        limit: Option<f64>,
        status: SnapshotStatus,
    ) -> SnapshotViewDto {
        SnapshotViewDto {
            account_id: "a1".to_owned(),
            snapshot: QuotaSnapshot {
                metric_id: Some("quota".to_owned()),
                used,
                limit,
                remaining,
                status,
                ..QuotaSnapshot::default()
            },
            observed_at: 1,
            freshness: Freshness::Fresh,
        }
    }

    fn refresh_state() -> RefreshState {
        RefreshState::default()
    }

    /// The enqueue path always runs against a real account; standalone
    /// repositories need the referenced provider/account rows first.
    fn seed_account(repository: &LocalRepository) {
        use crate::domain::provider::OpenAiCompatibleConfig;
        use crate::domain::state::AccountMetadata;
        use std::collections::HashMap;
        let config = OpenAiCompatibleConfig::new(
            "generic.openai_compatible:a1",
            "Test provider",
            "https://provider.test/v1",
        );
        repository.upsert_provider(&config, 1).expect("provider");
        let account = AccountMetadata::from_auth_input(
            "a1",
            "generic.openai_compatible:a1",
            "测试账户",
            "api_key",
            None,
            HashMap::new(),
            true,
            None,
        );
        repository.upsert_account(&account, 1).expect("account");
    }

    fn context<'a>(
        snapshots: &'a [SnapshotViewDto],
        refresh: &'a RefreshState,
        prefs: &'a NotificationPreferences,
        now: u64,
    ) -> TriggerContext<'a> {
        TriggerContext {
            account_id: "a1",
            account_label: "测试账户",
            provider_id: "generic.openai_compatible:a1",
            snapshots,
            refresh,
            prefs,
            now,
        }
    }

    #[derive(Default)]
    struct CountingSink(AtomicUsize);

    impl NotificationSink for CountingSink {
        fn show(&self, _notification: &Notification) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn inserted_ids(repository: &LocalRepository) -> Vec<NotificationRecord> {
        repository.notifications(100).expect("history")
    }

    #[test]
    fn low_quota_fires_warning_at_default_threshold() {
        let prefs = NotificationPreferences::default();
        let views = [snapshot_view(
            Some(15.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        let low = drafts
            .iter()
            .find(|draft| draft.kind == NotificationKind::LowQuota)
            .expect("low quota draft");
        assert_eq!(low.severity, NotificationSeverity::Warning);
        assert_eq!(low.dedup_key, "low-quota:a1");
        assert!(low.message.contains("15%"));
        assert!(low.message.contains("测试账户"));
    }

    #[test]
    fn low_quota_is_critical_below_ten_percent() {
        let prefs = NotificationPreferences::default();
        let views = [snapshot_view(
            Some(5.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        let low = drafts
            .iter()
            .find(|draft| draft.kind == NotificationKind::LowQuota)
            .expect("low quota draft");
        assert_eq!(low.severity, NotificationSeverity::Critical);
    }

    #[test]
    fn healthy_ratio_produces_no_quota_drafts() {
        let prefs = NotificationPreferences::default();
        let views = [snapshot_view(
            Some(80.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert!(drafts.is_empty());
    }

    #[test]
    fn untrusted_snapshots_never_trigger() {
        let prefs = NotificationPreferences::default();
        let views = [
            snapshot_view(Some(5.0), None, Some(100.0), SnapshotStatus::Unavailable),
            snapshot_view(Some(5.0), None, Some(100.0), SnapshotStatus::Error),
        ];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert!(drafts.is_empty());
    }

    #[test]
    fn auth_expired_replaces_value_rules_for_the_cycle() {
        let prefs = NotificationPreferences::default();
        let views = [snapshot_view(
            Some(5.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = RefreshState {
            status: RefreshStatus::Failed,
            consecutive_failures: 5,
            error_code: Some("AuthExpired".to_owned()),
            ..RefreshState::default()
        };
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert_eq!(drafts.len(), 1, "auth failure must dominate the cycle");
        assert_eq!(drafts[0].kind, NotificationKind::AuthExpired);
        assert_eq!(drafts[0].severity, NotificationSeverity::Critical);
        assert_eq!(drafts[0].dedup_key, "auth-expired:a1");
    }

    #[test]
    fn provider_unreachable_requires_connectivity_code_and_repeat_failures() {
        let prefs = NotificationPreferences::default();
        let views = [];
        let base = RefreshState {
            status: RefreshStatus::Failed,
            error_code: Some("NetworkUnavailable".to_owned()),
            ..RefreshState::default()
        };
        let early = RefreshState {
            consecutive_failures: 2,
            ..base.clone()
        };
        assert!(evaluate_account_notifications(context(&views, &early, &prefs, 1_000)).is_empty());
        let late = RefreshState {
            consecutive_failures: 3,
            ..base.clone()
        };
        let drafts = evaluate_account_notifications(context(&views, &late, &prefs, 1_000));
        assert_eq!(drafts[0].kind, NotificationKind::ProviderUnreachable);
        let server_only = RefreshState {
            consecutive_failures: 9,
            error_code: Some("Server".to_owned()),
            ..base
        };
        assert!(
            evaluate_account_notifications(context(&views, &server_only, &prefs, 1_000)).is_empty()
        );
    }

    #[test]
    fn reset_approaching_uses_window_reset_and_per_cycle_dedup_key() {
        let prefs = NotificationPreferences::default();
        let mut view = snapshot_view(Some(80.0), None, Some(100.0), SnapshotStatus::Available);
        view.snapshot.window = Some(crate::domain::provider::QuotaWindow {
            window_type: None,
            duration_seconds: None,
            starts_at: None,
            ends_at: None,
            reset_at: Some("2030-01-01T01:00:00Z".to_owned()),
        });
        let views = [view];
        let refresh = refresh_state();
        // 2030-01-01T01:00:00Z == 1893459600
        let now = 1_893_459_600 - 1_500;
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, now));
        let reset = drafts
            .iter()
            .find(|draft| draft.kind == NotificationKind::ResetApproaching)
            .expect("reset draft");
        assert_eq!(reset.severity, NotificationSeverity::Info);
        assert_eq!(reset.dedup_key, "reset-soon:a1:1893459600");
        assert!(reset.message.contains("25 分钟"));
        // Outside the window nothing fires.
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, now - 600));
        assert!(drafts.is_empty());
    }

    #[test]
    fn balance_below_user_floor_triggers_once_per_condition() {
        let prefs = NotificationPreferences {
            balance_threshold: Some(100.0),
            ..NotificationPreferences::default()
        };
        let mut view = snapshot_view(Some(86.4), None, None, SnapshotStatus::Partial);
        view.snapshot.metric_id = Some("balance".to_owned());
        view.snapshot.metric_name = Some("Balance".to_owned());
        view.snapshot.currency = Some("¥".to_owned());
        let views = [view];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        let balance = drafts
            .iter()
            .find(|draft| draft.kind == NotificationKind::BalanceBelow)
            .expect("balance draft");
        assert!(balance.message.contains("¥86.40"));
        assert_eq!(balance.dedup_key, "balance-below:a1");
        // At or above the floor nothing fires.
        let prefs = NotificationPreferences {
            balance_threshold: Some(86.4),
            ..NotificationPreferences::default()
        };
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert!(drafts.is_empty());
    }

    #[test]
    fn disabled_preferences_skip_evaluation_entirely() {
        let prefs = NotificationPreferences {
            enabled: false,
            ..NotificationPreferences::default()
        };
        let views = [snapshot_view(
            Some(5.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = RefreshState {
            status: RefreshStatus::Failed,
            consecutive_failures: 9,
            error_code: Some("AuthExpired".to_owned()),
            ..RefreshState::default()
        };
        assert!(
            evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000)).is_empty()
        );
    }

    #[test]
    fn preferences_validation_bounds_inputs() {
        let prefs = NotificationPreferences::default();
        assert!(prefs.validate().is_ok());
        let invalid = NotificationPreferences {
            low_quota_threshold: 0.0,
            ..NotificationPreferences::default()
        };
        assert!(invalid.validate().is_err());
        let invalid = NotificationPreferences {
            balance_threshold: Some(-1.0),
            ..NotificationPreferences::default()
        };
        assert!(invalid.validate().is_err());
        let invalid = NotificationPreferences {
            reset_soon_window: 10,
            ..NotificationPreferences::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn enqueue_dedups_within_cooldown_and_reports_delivered() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let sink = Arc::new(CountingSink::default());
        let prefs = NotificationPreferences::default();
        let views = [snapshot_view(
            Some(5.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));

        let delivered = enqueue_new(&repository, sink.as_ref(), &prefs, drafts.clone(), 1_000)
            .expect("first enqueue");
        assert_eq!(delivered.len(), 1);
        assert_eq!(sink.0.load(Ordering::SeqCst), 1);

        let repeated =
            enqueue_new(&repository, sink.as_ref(), &prefs, drafts, 2_000).expect("second enqueue");
        assert!(repeated.is_empty(), "cooldown must suppress repeats");
        assert_eq!(sink.0.load(Ordering::SeqCst), 1);
        assert_eq!(inserted_ids(&repository).len(), 1);
    }

    #[test]
    fn enqueue_respects_silent_and_critical_only_modes() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let sink = Arc::new(CountingSink::default());
        let silent = NotificationPreferences {
            silent_mode: true,
            ..NotificationPreferences::default()
        };
        let views = [snapshot_view(
            Some(5.0),
            None,
            Some(100.0),
            SnapshotStatus::Available,
        )];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &silent, 1_000));
        let delivered = enqueue_new(&repository, sink.as_ref(), &silent, drafts, 1_000)
            .expect("silent enqueue");
        assert_eq!(delivered.len(), 1, "silent mode still records history");
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            0,
            "no OS banner while silent"
        );

        let critical_only = NotificationPreferences {
            critical_only: true,
            ..NotificationPreferences::default()
        };
        let fresh_repo = LocalRepository::open_in_memory().expect("database");
        seed_account(&fresh_repo);
        let drafts =
            evaluate_account_notifications(context(&views, &refresh, &critical_only, 1_000));
        let delivered = enqueue_new(&fresh_repo, sink.as_ref(), &critical_only, drafts, 1_000)
            .expect("critical-only enqueue");
        assert_eq!(delivered.len(), 1);
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            1,
            "critical passes the filter"
        );
    }

    #[test]
    fn enqueue_prunes_history_beyond_retention_and_cap() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let sink = LogSink;
        let prefs = NotificationPreferences::default();
        for index in 0..40_u64 {
            let drafts = vec![Notification::draft(
                &DraftScope {
                    account_id: "a1",
                    provider_id: "generic.openai_compatible:a1",
                    now: 1_000 + index,
                },
                NotificationKind::ResetApproaching,
                NotificationSeverity::Info,
                "重置",
                "消息",
                // Unique keys per index so each insert lands.
                format!("reset-soon:a1:{index}"),
            )];
            enqueue_new(&repository, &sink, &prefs, drafts, 1_000 + index).expect("enqueue");
        }
        // Cap applies relative to the newest insert timestamp.
        repository
            .prune_notifications(1_039, RETENTION_SECS, 30)
            .expect("prune");
        assert_eq!(inserted_ids(&repository).len(), 30);
    }

    #[test]
    fn kind_and_severity_round_trip_through_storage_strings() {
        for kind in [
            NotificationKind::LowQuota,
            NotificationKind::ResetApproaching,
            NotificationKind::AuthExpired,
            NotificationKind::ProviderUnreachable,
            NotificationKind::BalanceBelow,
        ] {
            assert_eq!(NotificationKind::from_str_value(kind.as_str()), Some(kind));
        }
        for severity in [
            NotificationSeverity::Info,
            NotificationSeverity::Warning,
            NotificationSeverity::Critical,
        ] {
            assert_eq!(
                NotificationSeverity::from_str_value(severity.as_str()),
                Some(severity)
            );
        }
        let record = NotificationRecord::from(Notification::draft(
            &DraftScope {
                account_id: "a1",
                provider_id: "p1",
                now: 7,
            },
            NotificationKind::LowQuota,
            NotificationSeverity::Warning,
            "标题",
            "内容",
            "low-quota:a1",
        ));
        assert_eq!(record.kind, "low_quota");
        assert_eq!(record.severity, "WARNING");
        assert_eq!(record.read_at, None);
    }
}
