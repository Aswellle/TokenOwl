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
    /// A specific quota window (5h, 7d, daily…) dropped below its own
    /// threshold. `docs/01` §12 lists the 5h and 7d windows as separate
    /// triggers; collapsing them into one account-level warning hid which
    /// window was actually running out.
    WindowLow,
    /// Burn rate projects exhaustion before the window resets.
    ProjectedExhaustion,
    /// A condition that previously fired has cleared.
    Recovered,
}

impl NotificationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LowQuota => "low_quota",
            Self::ResetApproaching => "reset_approaching",
            Self::AuthExpired => "auth_expired",
            Self::ProviderUnreachable => "provider_unreachable",
            Self::BalanceBelow => "balance_below",
            Self::WindowLow => "window_low",
            Self::ProjectedExhaustion => "projected_exhaustion",
            Self::Recovered => "recovered",
        }
    }

    pub fn from_str_value(value: &str) -> Option<Self> {
        match value {
            "low_quota" => Some(Self::LowQuota),
            "reset_approaching" => Some(Self::ResetApproaching),
            "auth_expired" => Some(Self::AuthExpired),
            "provider_unreachable" => Some(Self::ProviderUnreachable),
            "balance_below" => Some(Self::BalanceBelow),
            "window_low" => Some(Self::WindowLow),
            "projected_exhaustion" => Some(Self::ProjectedExhaustion),
            "recovered" => Some(Self::Recovered),
            _ => None,
        }
    }
}

/// A recognized quota window class, used to name and scope per-window alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WindowClass {
    FiveHour,
    Daily,
    Weekly,
    Monthly,
    /// A window the provider reports with some span we do not recognize.
    Other,
}

impl WindowClass {
    /// Short label used in the dedup key and the metric tag.
    pub fn tag(self) -> &'static str {
        match self {
            Self::FiveHour => "5h",
            Self::Daily => "daily",
            Self::Weekly => "7d",
            Self::Monthly => "monthly",
            Self::Other => "window",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FiveHour => "5 小时窗口",
            Self::Daily => "每日窗口",
            Self::Weekly => "每周窗口",
            Self::Monthly => "每月窗口",
            Self::Other => "额度窗口",
        }
    }
}

/// Classifies a snapshot's window by its declared span.
///
/// The bands mirror the frontend's `quotaWindowTag` so the bubble's ring
/// labels and the notification text never disagree: 4–6h reads as 5h and
/// 144–192h as 7d, which is how providers round their own windows.
fn window_class(snapshot: &QuotaSnapshot) -> Option<WindowClass> {
    let window = snapshot.window.as_ref();
    if let Some(window) = window {
        if let Some(seconds) = window.duration_seconds.filter(|value| *value > 0) {
            #[allow(clippy::cast_precision_loss)] // window spans are small integers
            let hours = seconds as f64 / 3_600.0;
            return Some(classify_hours(hours));
        }
        let start = window.starts_at.as_deref().and_then(parse_unix_seconds);
        let end = window.ends_at.as_deref().and_then(parse_unix_seconds);
        if let (Some(start), Some(end)) = (start, end)
            && end > start
        {
            #[allow(clippy::cast_precision_loss)] // window spans are small integers
            let hours = (end - start) as f64 / 3_600.0;
            return Some(classify_hours(hours));
        }
    }
    // No span reported (or no window at all): fall back to the provider's own
    // metric naming, the same signals the bubble uses when a window carries no
    // timestamps.
    let text = format!(
        "{} {}",
        snapshot.metric_name.as_deref().unwrap_or_default(),
        snapshot.metric_id.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    if text.contains("5h") || text.contains("5 小时") || text.contains("5小时") {
        return Some(WindowClass::FiveHour);
    }
    if text.contains("7d") || text.contains("7 天") || text.contains("7天") || text.contains("周")
    {
        return Some(WindowClass::Weekly);
    }
    if text.contains("month") || text.contains("月") {
        return Some(WindowClass::Monthly);
    }
    if text.contains("day") || text.contains("日") {
        return Some(WindowClass::Daily);
    }
    None
}

fn classify_hours(hours: f64) -> WindowClass {
    if (4.0..=6.0).contains(&hours) {
        WindowClass::FiveHour
    } else if (144.0..=192.0).contains(&hours) {
        WindowClass::Weekly
    } else if hours >= 600.0 {
        WindowClass::Monthly
    } else if (20.0..=28.0).contains(&hours) {
        WindowClass::Daily
    } else {
        WindowClass::Other
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
    /// Warn when the projected exhaustion is at most this many seconds away.
    /// `0` disables the predictive rule.
    pub projected_exhaustion_horizon: u64,
    /// Do not raise OS banners inside this local-time window, expressed as
    /// minutes since midnight. Equal values disable quiet hours.
    pub quiet_hours_start: u32,
    pub quiet_hours_end: u32,
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
            // 45 minutes: long enough to act on, short enough that a healthy
            // rolling window never trips it.
            projected_exhaustion_horizon: 2_700,
            quiet_hours_start: 22 * 60,
            quiet_hours_end: 8 * 60,
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
        if let Some(threshold) = self.balance_threshold
            && (!threshold.is_finite() || threshold < 0.0)
        {
            return Err("余额下限不能为负数。".to_owned());
        }
        if !(300..=86_400).contains(&self.reset_soon_window) {
            return Err("重置提醒需在 5 分钟到 24 小时之间。".to_owned());
        }
        if self.projected_exhaustion_horizon > 86_400 {
            return Err("耗尽预测窗口不能超过 24 小时。".to_owned());
        }
        if self.quiet_hours_start > 1_439 || self.quiet_hours_end > 1_439 {
            return Err("安静时段需在一天之内。".to_owned());
        }
        Ok(())
    }

    /// Whether `now` falls inside the configured quiet hours.
    ///
    /// The window may wrap past midnight (22:00 → 08:00), so the comparison
    /// branches on which side of midnight the range crosses. Critical
    /// notifications bypass quiet hours — silencing "your key was rejected"
    /// overnight would be actively unhelpful.
    pub fn in_quiet_hours(&self, now: u64) -> bool {
        use chrono::Timelike;

        if self.quiet_hours_start == self.quiet_hours_end {
            return false;
        }
        let local = chrono::Local;
        let moment = chrono::DateTime::from_timestamp(i64::try_from(now).unwrap_or(0), 0)
            .map(|utc| utc.with_timezone(&local));
        let Some(moment) = moment else {
            return false;
        };
        let minutes = moment.hour() * 60 + moment.minute();
        if self.quiet_hours_start < self.quiet_hours_end {
            (self.quiet_hours_start..self.quiet_hours_end).contains(&minutes)
        } else {
            minutes >= self.quiet_hours_start || minutes < self.quiet_hours_end
        }
    }
}

/// Everything the pure evaluator needs. `snapshots` carries the last known
/// views (expired views are already filtered upstream); unknown values stay
/// unknown and never trigger. `pace` is the burn rate the caller derived from
/// history; `None` means history is too thin to project, which disables the
/// predictive rule rather than guessing.
pub struct TriggerContext<'a> {
    pub account_id: &'a str,
    pub account_label: &'a str,
    pub provider_id: &'a str,
    pub snapshots: &'a [SnapshotViewDto],
    pub refresh: &'a RefreshState,
    pub prefs: &'a NotificationPreferences,
    pub now: u64,
    pub pace: Option<&'a crate::domain::history::BurnRate>,
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
    if let Some(recovered) = recovered_draft(&context) {
        candidates.push(recovered);
    }
    // Per-window alerts first: they are the most specific statement of what is
    // running out, and the account-level rule only covers windowless metrics.
    candidates.extend(window_low_drafts(&context));
    if let Some(low_quota) = low_quota_draft(&context) {
        candidates.push(low_quota);
    }
    if let Some(projected) = projected_exhaustion_draft(&context) {
        candidates.push(projected);
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

/// Per-window low-quota drafts.
///
/// `docs/01` §12 lists the 5h and 7d windows as separate triggers, so each
/// recognized window gets its own alert and its own dedup key: a 5h window at
/// 8% is a different (and more urgent) fact than a 7d window at 8%, and
/// collapsing them into one account-level "low quota" hid which one was
/// running out. Windows we cannot classify fall back to the account-level
/// rule so nothing is lost.
fn window_low_drafts(context: &TriggerContext<'_>) -> Vec<Notification> {
    let threshold = context.prefs.low_quota_threshold / 100.0;
    let scope = DraftScope::of(context);
    let mut drafts = Vec::new();
    let mut seen: Vec<WindowClass> = Vec::new();

    for view in context.snapshots {
        if !matches!(
            view.snapshot.status,
            SnapshotStatus::Available | SnapshotStatus::Partial
        ) {
            continue;
        }
        let Some(class) = window_class(&view.snapshot) else {
            continue;
        };
        // One alert per window class: providers often expose several metrics
        // inside the same span, and three "5h window low" banners is noise.
        if seen.contains(&class) {
            continue;
        }
        let Some(ratio) = remaining_ratio(
            view.snapshot.remaining,
            view.snapshot.used,
            view.snapshot.limit,
        ) else {
            continue;
        };
        if ratio >= threshold {
            continue;
        }
        seen.push(class);
        let severity = if ratio < CRITICAL_RATIO {
            NotificationSeverity::Critical
        } else {
            NotificationSeverity::Warning
        };
        let resets_in = reset_timestamp(&view.snapshot)
            .filter(|reset_at| *reset_at > context.now)
            .map(|reset_at| humanize_countdown(reset_at - context.now));
        let detail = match resets_in {
            Some(countdown) => format!("仅剩 {:.0}%，约 {}后重置。", ratio * 100.0, countdown),
            None => format!("仅剩 {:.0}%。", ratio * 100.0),
        };
        drafts.push(Notification::draft(
            &scope,
            NotificationKind::WindowLow,
            severity,
            format!("{}额度不足", class.label()),
            format!("「{}」{}：{}", context.account_label, class.label(), detail),
            format!("window-low:{}:{}", context.account_id, class.tag()),
        ));
    }
    drafts
}

/// The worst trusted remaining ratio crossed the user's warning boundary.
///
/// Only covers snapshots without a recognizable window — anything with a
/// window is handled by [`window_low_drafts`], which reports *which* window
/// is low instead of one undifferentiated account-level number.
fn low_quota_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let (view, ratio) = worst_trusted_ratio(context.snapshots, |snapshot| {
        window_class(snapshot).is_none()
    })?;
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

/// Warns when the current pace exhausts a window before it resets.
///
/// This is the difference between "you have 12% left" and "at this pace you
/// run out in 40 minutes, and the window does not reset for 3 hours" — the
/// second one is actionable. Only token-denominated windows participate,
/// because burn rate needs a single meaningful pace unit.
fn projected_exhaustion_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    let pace = context.pace?;
    let hours_left = pace.projected_exhaustion_hours?;
    if !hours_left.is_finite() || hours_left <= 0.0 {
        return None;
    }
    // Compare in whole seconds on both sides so the arithmetic stays integer
    // where the rest of the pipeline is.
    let horizon_secs = context.prefs.projected_exhaustion_horizon;
    // `hours_left` is finite and positive (checked above), so this stays in
    // range; the saturating conversion is belt-and-braces.
    #[allow(clippy::cast_possible_truncation)] // bounded by the finite check
    let projected_secs = u64::try_from((hours_left * 3_600.0).round() as i64).unwrap_or(0);
    if projected_secs > horizon_secs {
        return None;
    }
    let resets_in = soonest_reset_within(context.snapshots, u64::MAX, context.now)
        .map(|(reset_at, view)| (reset_at, view, window_class(&view.snapshot)));
    // If the window resets before the projected exhaustion, there is nothing
    // to act on — that is the normal shape of a healthy rolling window.
    if let Some((reset_at, _, _)) = resets_in
        && reset_at.saturating_sub(context.now) < projected_secs
    {
        return None;
    }
    let window_name = resets_in
        .and_then(|(_, view, class)| {
            class
                .map(WindowClass::label)
                .or(view.snapshot.metric_name.as_deref())
        })
        .unwrap_or("额度");
    let scope = DraftScope::of(context);
    Some(Notification::draft(
        &scope,
        NotificationKind::ProjectedExhaustion,
        NotificationSeverity::Warning,
        "按当前速度即将耗尽",
        format!(
            "「{}」的{}按当前消耗速度约 {} 后耗尽，早于重置时间。",
            context.account_label,
            window_name,
            humanize_countdown(projected_secs)
        ),
        format!("projected-exhaustion:{}", context.account_id),
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

/// A previously-failing account is healthy again.
///
/// Without this, the notification centre only ever accumulates bad news: a
/// user who saw "long disconnected" has no way to learn it resolved except by
/// opening the dashboard. Fires once per recovery, keyed on the transition.
fn recovered_draft(context: &TriggerContext<'_>) -> Option<Notification> {
    if context.refresh.status != RefreshStatus::Succeeded {
        return None;
    }
    if context.refresh.consecutive_failures > 0 {
        return None;
    }
    // Only announce a recovery if the account was actually in a bad state we
    // would have told the user about: an error code on the previous attempt.
    let last_error = context.refresh.last_error_code.as_deref()?;
    let scope = DraftScope::of(context);
    let reason = if is_unreachable_code(last_error) {
        "连接已恢复"
    } else {
        "刷新已恢复正常"
    };
    Some(Notification::draft(
        &scope,
        NotificationKind::Recovered,
        NotificationSeverity::Info,
        reason,
        format!(
            "「{}」已恢复正常，额度数据重新更新。",
            context.account_label
        ),
        // Keyed on the error that cleared, so a later, different outage can
        // announce its own recovery.
        format!("recovered:{}:{}", context.account_id, last_error),
    ))
}
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

/// Merges the banners one refresh would raise for the same account.
///
/// Per-window alerting means a single refresh can legitimately produce a 5h
/// low, a 7d low, and an exhaustion projection at once. Three banners for one
/// account is exactly the noise the "don't disturb" principle forbids, so the
/// audible side is collapsed into one summary while every underlying
/// condition keeps its own history row (the notification centre stays
/// complete and each condition keeps its own dedup key).
///
/// Returns the notifications to show as banners. History is written by the
/// caller before this runs, so the summary never hides information — it only
/// decides how much of it arrives as a popup.
pub fn summarize_banners(delivered: &[Notification]) -> Vec<Notification> {
    use std::collections::BTreeMap;

    let mut grouped: BTreeMap<Option<&str>, Vec<&Notification>> = BTreeMap::new();
    for item in delivered {
        grouped
            .entry(item.account_id.as_deref())
            .or_default()
            .push(item);
    }

    let mut banners = Vec::new();
    for (account_id, items) in grouped {
        if items.len() == 1 {
            banners.push(items[0].clone());
            continue;
        }
        // The summary inherits the worst severity so the banner's urgency and
        // sound still reflect the most serious condition inside it.
        let severity = items
            .iter()
            .map(|item| item.severity)
            .max_by_key(|value| match value {
                NotificationSeverity::Info => 0,
                NotificationSeverity::Warning => 1,
                NotificationSeverity::Critical => 2,
            })
            .unwrap_or(NotificationSeverity::Info);
        let first = items[0];
        // The account label lives in each message; the summary title names the
        // conditions instead of repeating every full sentence.
        let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
        banners.push(Notification {
            id: format!("ntf-{}", nanoid::nanoid!(10)),
            kind: first.kind,
            severity,
            title: format!("{} 条额度提醒", items.len()),
            message: format!(
                "{}：{}",
                items
                    .iter()
                    .find_map(|item| account_label_from_message(&item.message))
                    .unwrap_or("当前账户"),
                titles.join("、")
            ),
            provider_id: first.provider_id.clone(),
            account_id: account_id.map(ToOwned::to_owned),
            dedup_key: format!("summary:{}", account_id.unwrap_or("account")),
            created_at: first.created_at,
        });
    }
    banners
}

/// Extracts the account label from a draft message.
///
/// Drafts are built as `「label」…`, so the quoted prefix is the label. Kept as
/// a helper rather than threading the label through the merge so the merge
/// stays a pure function of the drafts it is given.
fn account_label_from_message(message: &str) -> Option<&str> {
    let rest = message.strip_prefix('「')?;
    let end = rest.find('」')?;
    Some(&rest[..end])
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
    let mut audible_items = Vec::new();
    for candidate in candidates {
        let mut record = NotificationRecord::from(candidate.clone());
        record.created_at = now;
        if !repository.insert_notification(record, DEDUP_COOLDOWN_SECS, now)? {
            continue;
        }
        let mut delivered_notification = candidate;
        delivered_notification.created_at = now;
        // Critical alerts pierce quiet hours: silencing "your key was
        // rejected" overnight would hide the one message worth waking for.
        let quiet = prefs.in_quiet_hours(now)
            && delivered_notification.severity != NotificationSeverity::Critical;
        let audible = !prefs.silent_mode
            && !quiet
            && (!prefs.critical_only
                || delivered_notification.severity == NotificationSeverity::Critical);
        if audible {
            audible_items.push(delivered_notification.clone());
        }
        delivered.push(delivered_notification);
    }
    // One banner per account per refresh: several conditions can be true at
    // once, but the user should not receive a stack of popups for them.
    for banner in summarize_banners(&audible_items) {
        sink.show(&banner);
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
        let mut builder = self
            .app
            .notification()
            .builder()
            .title(&notification.title)
            .body(&notification.message);
        // One OS thread per account keeps a user's banners grouped the way
        // the notification centre groups them; the severity decides whether
        // the banner should also make a sound.
        if let Some(account) = notification.account_id.as_deref() {
            builder = builder.group(format!("account:{account}"));
        }
        if notification.severity == NotificationSeverity::Critical {
            builder = builder.sound("default");
        }
        let _ = builder.show();
    }
}

fn is_unreachable_code(code: &str) -> bool {
    matches!(
        code,
        "NetworkUnavailable" | "Timeout" | "TlsError" | "ProviderUnavailable"
    )
}

/// Worst remaining ratio across snapshots the parser actually trusted, with
/// an optional filter so the caller can scope it to one kind of metric.
fn worst_trusted_ratio(
    snapshots: &[SnapshotViewDto],
    include: impl Fn(&QuotaSnapshot) -> bool,
) -> Option<(&SnapshotViewDto, f64)> {
    snapshots
        .iter()
        .filter(|view| {
            matches!(
                view.snapshot.status,
                SnapshotStatus::Available | SnapshotStatus::Partial
            )
        })
        .filter(|view| include(&view.snapshot))
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
            pace: None,
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
        assert_eq!(drafts, [] as [Notification; 0]);
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
        assert_eq!(drafts, [] as [Notification; 0]);
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
        assert_eq!(
            evaluate_account_notifications(context(&views, &early, &prefs, 1_000)),
            [] as [Notification; 0]
        );
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
        assert_eq!(
            evaluate_account_notifications(context(&views, &server_only, &prefs, 1_000)),
            [] as [Notification; 0]
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
        assert_eq!(drafts, [] as [Notification; 0]);
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
        assert_eq!(drafts, [] as [Notification; 0]);
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
        assert_eq!(
            evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000)),
            [] as [Notification; 0]
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
            NotificationKind::WindowLow,
            NotificationKind::ProjectedExhaustion,
            NotificationKind::Recovered,
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

    // ---------- per-window alerts ----------

    fn windowed_view(metric: &str, hours: i64, remaining: f64, limit: f64) -> SnapshotViewDto {
        let mut view = snapshot_view(
            Some(remaining),
            None,
            Some(limit),
            SnapshotStatus::Available,
        );
        view.snapshot.metric_id = Some(metric.to_owned());
        view.snapshot.metric_name = Some(metric.to_owned());
        view.snapshot.window = Some(crate::domain::provider::QuotaWindow {
            window_type: Some(crate::domain::provider::WindowType::Rolling),
            duration_seconds: Some(hours * 3_600),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        view
    }

    /// `docs/01` §12 lists 5h and 7d as separate triggers; each must produce
    /// its own alert so the user learns *which* window is running out.
    #[test]
    fn each_window_class_alerts_separately() {
        let prefs = NotificationPreferences::default();
        let views = [
            windowed_view("5h", 5, 8.0, 100.0),
            windowed_view("7d", 168, 10.0, 100.0),
        ];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        let window_lows: Vec<_> = drafts
            .iter()
            .filter(|draft| draft.kind == NotificationKind::WindowLow)
            .collect();
        assert_eq!(window_lows.len(), 2, "one alert per window class");
        let keys: Vec<&str> = window_lows
            .iter()
            .map(|draft| draft.dedup_key.as_str())
            .collect();
        assert!(keys.contains(&"window-low:a1:5h"));
        assert!(keys.contains(&"window-low:a1:7d"));
        assert!(
            window_lows
                .iter()
                .any(|draft| draft.title.contains("5 小时窗口"))
        );
        assert!(
            window_lows
                .iter()
                .any(|draft| draft.title.contains("每周窗口"))
        );
    }

    #[test]
    fn windowed_snapshots_do_not_also_fire_the_account_level_rule() {
        let prefs = NotificationPreferences::default();
        let views = [windowed_view("5h", 5, 5.0, 100.0)];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert!(
            drafts
                .iter()
                .all(|draft| draft.kind != NotificationKind::LowQuota),
            "a windowed metric must not double-report as account-level low quota"
        );
    }

    #[test]
    fn window_class_bands_match_the_bubble_labels() {
        assert_eq!(classify_hours(5.0), WindowClass::FiveHour);
        assert_eq!(classify_hours(4.0), WindowClass::FiveHour);
        assert_eq!(classify_hours(6.0), WindowClass::FiveHour);
        assert_eq!(classify_hours(168.0), WindowClass::Weekly);
        assert_eq!(classify_hours(144.0), WindowClass::Weekly);
        assert_eq!(classify_hours(24.0), WindowClass::Daily);
        assert_eq!(classify_hours(720.0), WindowClass::Monthly);
        assert_eq!(classify_hours(2.0), WindowClass::Other);
    }

    #[test]
    fn window_class_falls_back_to_metric_naming_without_a_span() {
        let mut view = snapshot_view(Some(5.0), None, Some(100.0), SnapshotStatus::Available);
        view.snapshot.metric_name = Some("5 小时用量窗口".to_owned());
        assert_eq!(window_class(&view.snapshot), Some(WindowClass::FiveHour));
        view.snapshot.metric_name = Some("每周额度".to_owned());
        assert_eq!(window_class(&view.snapshot), Some(WindowClass::Weekly));
        view.snapshot.metric_name = Some("余额".to_owned());
        assert_eq!(window_class(&view.snapshot), None);
    }

    // ---------- predictive exhaustion ----------

    #[test]
    fn projected_exhaustion_fires_when_pace_beats_the_reset() {
        let prefs = NotificationPreferences::default();
        let mut view = windowed_view("5h", 5, 12.0, 100.0);
        // Resets in 3 hours — well after the projected 40-minute horizon.
        let now = 1_000_u64;
        view.snapshot.window = Some(crate::domain::provider::QuotaWindow {
            window_type: Some(crate::domain::provider::WindowType::Rolling),
            duration_seconds: Some(5 * 3_600),
            starts_at: None,
            ends_at: None,
            reset_at: Some(
                chrono::DateTime::from_timestamp(i64::try_from(now + 3 * 3_600).unwrap(), 0)
                    .unwrap()
                    .to_rfc3339(),
            ),
        });
        let pace = crate::domain::history::BurnRate {
            tokens_per_hour: 18.0,
            projected_exhaustion_hours: Some(0.67),
        };
        let views = [view];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(TriggerContext {
            account_id: "a1",
            account_label: "测试账户",
            provider_id: "generic.openai_compatible:a1",
            snapshots: &views,
            refresh: &refresh,
            prefs: &prefs,
            now,
            pace: Some(&pace),
        });
        let projected = drafts
            .iter()
            .find(|draft| draft.kind == NotificationKind::ProjectedExhaustion)
            .expect("projection draft");
        assert_eq!(projected.severity, NotificationSeverity::Warning);
        assert!(projected.message.contains("耗尽"));
    }

    #[test]
    fn projected_exhaustion_stays_quiet_when_the_window_resets_first() {
        let prefs = NotificationPreferences::default();
        let mut view = windowed_view("5h", 5, 12.0, 100.0);
        let now = 1_000_u64;
        view.snapshot.window = Some(crate::domain::provider::QuotaWindow {
            window_type: Some(crate::domain::provider::WindowType::Rolling),
            duration_seconds: Some(5 * 3_600),
            starts_at: None,
            ends_at: None,
            // Resets in 10 minutes, exhaustion projected in 40 — nothing to do.
            reset_at: Some(
                chrono::DateTime::from_timestamp(i64::try_from(now + 600).unwrap(), 0)
                    .unwrap()
                    .to_rfc3339(),
            ),
        });
        let pace = crate::domain::history::BurnRate {
            tokens_per_hour: 18.0,
            projected_exhaustion_hours: Some(0.67),
        };
        let views = [view];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(TriggerContext {
            account_id: "a1",
            account_label: "测试账户",
            provider_id: "generic.openai_compatible:a1",
            snapshots: &views,
            refresh: &refresh,
            prefs: &prefs,
            now,
            pace: Some(&pace),
        });
        assert!(
            drafts
                .iter()
                .all(|draft| draft.kind != NotificationKind::ProjectedExhaustion)
        );
    }

    #[test]
    fn projected_exhaustion_is_disabled_without_a_pace() {
        let prefs = NotificationPreferences::default();
        let views = [windowed_view("5h", 5, 12.0, 100.0)];
        let refresh = refresh_state();
        let drafts = evaluate_account_notifications(context(&views, &refresh, &prefs, 1_000));
        assert!(
            drafts
                .iter()
                .all(|draft| draft.kind != NotificationKind::ProjectedExhaustion)
        );
    }

    // ---------- recovery ----------

    #[test]
    fn recovery_announces_once_and_only_after_a_failure() {
        let prefs = NotificationPreferences::default();
        let views = [windowed_view("5h", 5, 80.0, 100.0)];
        let recovered = RefreshState {
            status: RefreshStatus::Succeeded,
            consecutive_failures: 0,
            last_error_code: Some("NetworkUnavailable".to_owned()),
            ..RefreshState::default()
        };
        let drafts = evaluate_account_notifications(context(&views, &recovered, &prefs, 1_000));
        let draft = drafts
            .iter()
            .find(|item| item.kind == NotificationKind::Recovered)
            .expect("recovery draft");
        assert_eq!(draft.severity, NotificationSeverity::Info);
        assert!(draft.dedup_key.contains("NetworkUnavailable"));

        // Never-broken account: nothing to announce.
        let healthy = RefreshState {
            status: RefreshStatus::Succeeded,
            ..RefreshState::default()
        };
        let drafts = evaluate_account_notifications(context(&views, &healthy, &prefs, 1_000));
        assert!(
            drafts
                .iter()
                .all(|item| item.kind != NotificationKind::Recovered)
        );
    }

    // ---------- quiet hours ----------

    #[test]
    fn quiet_hours_wrap_midnight_and_spare_critical_alerts() {
        let prefs = NotificationPreferences {
            quiet_hours_start: 22 * 60,
            quiet_hours_end: 8 * 60,
            ..NotificationPreferences::default()
        };
        assert!(prefs.in_quiet_hours(quiet_instant(23, 30)));
        assert!(prefs.in_quiet_hours(quiet_instant(2, 0)));
        assert!(!prefs.in_quiet_hours(quiet_instant(12, 0)));
        assert!(!prefs.in_quiet_hours(quiet_instant(8, 0)));

        // Critical alerts pierce quiet hours; warnings do not.
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let sink = Arc::new(CountingSink::default());
        let now = quiet_instant(23, 0);
        let critical = vec![Notification::draft(
            &DraftScope {
                account_id: "a1",
                provider_id: "generic.openai_compatible:a1",
                now,
            },
            NotificationKind::AuthExpired,
            NotificationSeverity::Critical,
            "认证已过期",
            "消息",
            "auth-expired:a1",
        )];
        enqueue_new(&repository, sink.as_ref(), &prefs, critical, now).expect("enqueue");
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            1,
            "critical must pierce quiet hours"
        );
    }

    /// Builds a timestamp whose *local* wall-clock time is the given hour and
    /// minute, so the quiet-hours test is timezone independent.
    fn quiet_instant(hour: u32, minute: u32) -> u64 {
        use chrono::{Datelike, Local, TimeZone, Timelike};
        let today = Local::now();
        let mut candidate = Local
            .with_ymd_and_hms(today.year(), today.month(), today.day(), hour, minute, 0)
            .single()
            .expect("local time exists");
        // `with_ymd_and_hms` can resolve to a different instant across DST;
        // nudge by an hour until the wall clock matches.
        for _ in 0..3 {
            if candidate.hour() == hour && candidate.minute() == minute {
                break;
            }
            candidate += chrono::Duration::hours(1);
        }
        u64::try_from(candidate.timestamp()).unwrap_or(0)
    }

    // ---------- banner summarization ----------

    fn draft_for(account: &str, title: &str, message: &str, key: &str) -> Notification {
        Notification::draft(
            &DraftScope {
                account_id: account,
                // Must match the seeded provider row: the history insert has a
                // foreign key on provider_id.
                provider_id: "generic.openai_compatible:a1",
                now: 1_000,
            },
            NotificationKind::WindowLow,
            NotificationSeverity::Warning,
            title,
            message,
            key,
        )
    }

    /// Several conditions on one account must arrive as one banner; the
    /// notification centre still records every condition separately.
    #[test]
    fn multiple_conditions_on_one_account_collapse_into_one_banner() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let sink = Arc::new(CountingSink::default());
        let prefs = NotificationPreferences {
            quiet_hours_start: 0,
            quiet_hours_end: 0,
            ..NotificationPreferences::default()
        };
        let drafts = vec![
            draft_for(
                "a1",
                "5 小时窗口额度不足",
                "「测试账户」5 小时窗口：仅剩 8%。",
                "window-low:a1:5h",
            ),
            draft_for(
                "a1",
                "每周窗口额度不足",
                "「测试账户」每周窗口：仅剩 12%。",
                "window-low:a1:7d",
            ),
        ];
        let delivered =
            enqueue_new(&repository, sink.as_ref(), &prefs, drafts, 1_000).expect("enqueue");
        assert_eq!(delivered.len(), 2, "history keeps both conditions");
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            1,
            "but only one banner is raised"
        );
        assert_eq!(inserted_ids(&repository).len(), 2);
    }

    #[test]
    fn separate_accounts_keep_separate_banners() {
        use crate::domain::provider::OpenAiCompatibleConfig;
        use crate::domain::state::AccountMetadata;
        use std::collections::HashMap;

        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let config = OpenAiCompatibleConfig::new("p2", "Second", "https://provider.test/v2");
        repository.upsert_provider(&config, 1).expect("provider");
        repository
            .upsert_account(
                &AccountMetadata::from_auth_input(
                    "a2",
                    "p2",
                    "第二个账户",
                    "api_key",
                    None,
                    HashMap::new(),
                    true,
                    None,
                ),
                1,
            )
            .expect("account");
        let sink = Arc::new(CountingSink::default());
        let prefs = NotificationPreferences {
            quiet_hours_start: 0,
            quiet_hours_end: 0,
            ..NotificationPreferences::default()
        };
        let drafts = vec![
            draft_for(
                "a1",
                "5 小时窗口额度不足",
                "「测试账户」…",
                "window-low:a1:5h",
            ),
            draft_for(
                "a2",
                "5 小时窗口额度不足",
                "「第二个账户」…",
                "window-low:a2:5h",
            ),
        ];
        enqueue_new(&repository, sink.as_ref(), &prefs, drafts, 1_000).expect("enqueue");
        assert_eq!(
            sink.0.load(Ordering::SeqCst),
            2,
            "each account gets its own banner"
        );
    }

    #[test]
    fn summary_inherits_the_worst_severity() {
        let info = draft_for("a1", "额度即将重置", "「测试账户」…", "reset-soon:a1:1");
        let mut critical = draft_for("a1", "认证已过期", "「测试账户」…", "auth-expired:a1");
        critical.severity = NotificationSeverity::Critical;
        let banners = summarize_banners(&[info, critical]);
        assert_eq!(banners.len(), 1);
        assert_eq!(banners[0].severity, NotificationSeverity::Critical);
        assert!(banners[0].title.contains('2'));
        assert!(banners[0].message.contains("测试账户"));
    }

    #[test]
    fn single_condition_passes_through_unsummarized() {
        let only = draft_for(
            "a1",
            "5 小时窗口额度不足",
            "「测试账户」…",
            "window-low:a1:5h",
        );
        let banners = summarize_banners(std::slice::from_ref(&only));
        assert_eq!(banners.len(), 1);
        assert_eq!(banners[0].title, only.title);
    }
}
