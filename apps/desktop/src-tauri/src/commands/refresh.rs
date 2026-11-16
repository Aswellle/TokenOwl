//! Refresh-cadence settings and the adaptive-interval policy.
//!
//! Two independent knobs live here: the user's cadence preference (automatic
//! provider defaults, or an explicit 1/5/10/15/30 minutes) and the opt-in
//! adaptive mode. Adaptive mode only nudges within bounds — it halves the
//! provider's base interval at most when a window is nearly exhausted or a
//! reset is imminent, and doubles it at most for an idle account, never
//! crossing the configured ceiling. The provider default stays the anchor, so
//! a manifest's own rate-limit knowledge is still respected.

use super::{AppRuntime, Arc, CommandError, State};
use crate::domain::state::RefreshStatus;
use serde::Serialize;
use std::time::Duration;

/// Settings keys. Both are non-secret preferences, so they ride the settings
/// table rather than the credential store.
pub(super) const ADAPTIVE_REFRESH_KEY: &str = "adaptive_refresh";
pub(super) const REFRESH_CADENCE_KEY: &str = "refresh_cadence_minutes";

/// Cadences the settings UI offers, plus `0` for the provider-default mode.
pub(super) const ALLOWED_CADENCE_MINUTES: [u32; 6] = [0, 1, 5, 10, 15, 30];

/// The hard lower bound between refresh waves. A misconfigured manifest or a
/// user-chosen "1 minute" never becomes a hot loop.
pub(super) const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
/// An imminent window reset justifies the shorter branch.
const RESET_HORIZON: Duration = Duration::from_secs(15 * 60);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSettingsView {
    /// When true, a high-consumption or near-reset account is refreshed at up
    /// to half its base interval, and an idle account waits up to twice as
    /// long (capped by the cadence).
    pub adaptive: bool,
    /// `0` means "provider manifest defaults"; otherwise one of
    /// [`ALLOWED_CADENCE_MINUTES`].
    pub cadence_minutes: u32,
}

/// Reads the persisted settings. Missing or malformed values mean the safe
/// defaults: provider cadence, adaptive off.
pub(super) fn refresh_settings(
    repository: &crate::storage::LocalRepository,
) -> RefreshSettingsView {
    let adaptive = matches!(
        repository.get_setting(ADAPTIVE_REFRESH_KEY).ok().flatten(),
        Some(serde_json::Value::Bool(true))
    );
    let cadence_minutes = repository
        .get_setting(REFRESH_CADENCE_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .filter(|minutes| ALLOWED_CADENCE_MINUTES.contains(minutes))
        .unwrap_or(0);
    RefreshSettingsView {
        adaptive,
        cadence_minutes,
    }
}

/// The cadence ceiling and fallback: the user's explicit choice, else the
/// freshness policy's own target (30 minutes by default).
pub(super) fn cadence(fallback: Duration, settings: RefreshSettingsView) -> Duration {
    if settings.cadence_minutes == 0 {
        return fallback.max(MIN_REFRESH_INTERVAL);
    }
    let seconds = u64::from(settings.cadence_minutes) * 60;
    Duration::from_secs(seconds).clamp(MIN_REFRESH_INTERVAL, fallback.max(MIN_REFRESH_INTERVAL))
}

#[tauri::command]
pub fn refresh_settings_get(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<RefreshSettingsView, CommandError> {
    Ok(refresh_settings(&state.repository()?))
}

/// Persists the cadence preference. Validation happens here rather than in
/// the webview so a hand-crafted IPC call cannot request a hot loop.
#[tauri::command]
pub fn refresh_settings_set(
    adaptive: bool,
    cadence_minutes: u32,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    if !ALLOWED_CADENCE_MINUTES.contains(&cadence_minutes) {
        return Err(CommandError::InvalidInput(format!(
            "refresh cadence must be one of {ALLOWED_CADENCE_MINUTES:?} minutes"
        )));
    }
    let repository = state.repository()?;
    repository.set_setting(ADAPTIVE_REFRESH_KEY, &serde_json::Value::Bool(adaptive))?;
    repository.set_setting(
        REFRESH_CADENCE_KEY,
        &serde_json::Value::from(cadence_minutes),
    )?;
    Ok(())
}

/// One account's current scheduling interval, clamped to the configured
/// bounds. `base` is the provider's declared default (or the cadence when it
/// declares none); adaptive mode only moves within `[base/2, base*2]`.
pub(super) fn account_interval(
    base: Duration,
    ceiling: Duration,
    adaptive: bool,
    signals: AdaptiveSignals,
) -> Duration {
    let ceiling = ceiling.max(MIN_REFRESH_INTERVAL);
    let base = base.clamp(MIN_REFRESH_INTERVAL, ceiling);
    if !adaptive {
        return base;
    }
    // A reset inside RESET_HORIZON is worth catching: the next refresh is
    // what turns the window's counters back to a fresh period.
    if signals
        .reset_in
        .is_some_and(|remaining| remaining <= RESET_HORIZON)
    {
        return (base / 2).max(MIN_REFRESH_INTERVAL);
    }
    // Usage in the trailing window plus a nearly empty quota is the "cannot
    // wait for the default interval" case.
    if signals.had_activity && signals.remaining_ratio.is_some_and(|ratio| ratio <= 0.10) {
        return (base / 2).max(MIN_REFRESH_INTERVAL);
    }
    if !signals.had_activity {
        return (base * 2).min(ceiling).max(base);
    }
    base
}

/// Inputs to [`account_interval`] gathered from stored snapshots. Every field
/// is optional-by-absence: no history is not "idle", it is unknown, and
/// unknown leaves the provider default alone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct AdaptiveSignals {
    /// At least one trusted positive token/cost increment in the trailing
    /// activity window.
    pub had_activity: bool,
    /// The tightest remaining/limit ratio across the account's live
    /// snapshots, when both sides are known.
    pub remaining_ratio: Option<f64>,
    /// Time until the nearest window reset, when a snapshot reports one.
    pub reset_in: Option<Duration>,
}

/// Collects [`AdaptiveSignals`] for one account. Reads only local snapshots;
/// a failure on any facet degrades to "unknown" and never to a fabricated
/// idle or exhausted reading.
pub(super) fn adaptive_signals(
    repository: &crate::storage::LocalRepository,
    account_id: &str,
    now: u64,
    policy: crate::domain::state::FreshnessPolicy,
) -> AdaptiveSignals {
    const ACTIVITY_WINDOW: u64 = 2 * 3600;
    let from = now.saturating_sub(ACTIVITY_WINDOW);
    let samples = repository
        .history_samples(from, now.saturating_add(1))
        .unwrap_or_default()
        .into_iter()
        .filter(|sample| sample.account_id == account_id)
        .map(|sample| crate::domain::history::DailySample {
            account_id: sample.account_id,
            metric_key: sample.metric_key,
            observed_at: sample.observed_at,
            used: sample.used,
            remaining: sample.remaining,
            unit: sample.unit,
            currency: sample.currency,
        })
        .collect::<Vec<_>>();
    let usage = crate::domain::history::range_usage(&samples, from, now.saturating_add(1));
    let had_activity = usage.tokens > 0.0 || usage.cost.is_some_and(|cost| cost > 0.0);

    let views = repository
        .latest_snapshot_views(account_id, now, policy)
        .unwrap_or_default();
    let remaining_ratio = views
        .iter()
        .filter_map(
            |view| match (view.snapshot.remaining, view.snapshot.limit) {
                (Some(remaining), Some(limit)) if limit > 0.0 && remaining.is_finite() => {
                    Some(remaining / limit)
                }
                _ => None,
            },
        )
        .reduce(f64::min);
    let reset_in = views
        .iter()
        .filter_map(|view| {
            view.snapshot
                .reset_at
                .as_deref()
                .or_else(|| {
                    view.snapshot
                        .window
                        .as_ref()
                        .and_then(|window| window.reset_at.as_deref())
                })
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .and_then(|instant| u64::try_from(instant.timestamp()).ok())
                .filter(|reset_at| *reset_at > now)
                .map(|reset_at| Duration::from_secs(reset_at - now))
        })
        .min();
    AdaptiveSignals {
        had_activity,
        remaining_ratio,
        reset_in,
    }
}

/// One background-loop decision: which accounts are due now, and how long to
/// sleep before re-evaluating. Re-evaluating at least every 30 seconds keeps
/// a newly added account or a settings change responsive without a wakeup
/// per account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundPlan {
    pub due: Vec<String>,
    pub sleep: Duration,
}

/// Per-account next-attempt bookkeeping shared by the background plan and the
/// account view's "next scheduled refresh" line.
fn next_attempt_at(state: &crate::domain::state::RefreshState, interval: Duration) -> Option<u64> {
    if state.status == RefreshStatus::Failed {
        return state.next_attempt_at;
    }
    state
        .last_success_at
        .map(|last| last.saturating_add(interval.as_secs()))
}

/// Builds the background loop's next decision. Never refreshes an account
/// that is mid-flight elsewhere; failure backoff always wins over the normal
/// cadence.
pub(crate) fn background_plan(
    runtime: &AppRuntime,
    now: u64,
) -> Result<BackgroundPlan, CommandError> {
    let repository = runtime.repository()?;
    let ceiling = runtime.refresh_cadence();
    let accounts = repository.accounts()?;
    if accounts.is_empty() {
        return Ok(BackgroundPlan {
            due: Vec::new(),
            sleep: ceiling,
        });
    }
    let mut due = Vec::new();
    let mut earliest: Option<u64> = None;
    for account in &accounts {
        let state = repository.refresh_state(&account.id)?;
        if state.status == RefreshStatus::Refreshing {
            // Another entrance owns the account; look again soon without
            // queueing a second request.
            earliest = Some(earliest.map_or(now + 60, |current| current.min(now + 60)));
            continue;
        }
        let interval =
            runtime.account_interval(&repository, &account.id, &account.provider_id, now);
        let next = next_attempt_at(&state, interval).unwrap_or(0);
        if next <= now {
            due.push(account.id.clone());
        }
        earliest = Some(earliest.map_or(next, |current| current.min(next)));
    }
    let sleep = if due.is_empty() {
        let wait = earliest.unwrap_or(now).saturating_sub(now);
        Duration::from_secs(wait.max(30)).min(ceiling)
    } else {
        Duration::from_secs(30)
    };
    Ok(BackgroundPlan { due, sleep })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> (AppRuntime, std::path::PathBuf) {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tokenowl-refresh-{}-{}.sqlite3",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        (AppRuntime::new(&path).expect("runtime"), path)
    }

    #[test]
    fn missing_settings_mean_provider_defaults_and_adaptive_off() {
        let (runtime, path) = runtime();
        let settings = refresh_settings(&runtime.repository().expect("db"));
        assert_eq!(
            settings,
            RefreshSettingsView {
                adaptive: false,
                cadence_minutes: 0
            }
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn malformed_or_unknown_cadence_falls_back_to_automatic() {
        let (runtime, path) = runtime();
        let repository = runtime.repository().expect("db");
        repository
            .set_setting(REFRESH_CADENCE_KEY, &serde_json::json!(7))
            .expect("store");
        assert_eq!(
            refresh_settings(&repository),
            RefreshSettingsView {
                adaptive: false,
                cadence_minutes: 0
            }
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn adaptive_shortens_only_for_imminent_reset_or_nearly_empty_quota() {
        let base = Duration::from_secs(300);
        let ceiling = Duration::from_secs(1_800);
        let idle = AdaptiveSignals::default();
        assert_eq!(
            account_interval(base, ceiling, true, idle),
            Duration::from_secs(600),
            "an idle account waits twice as long, within the ceiling"
        );
        let active = AdaptiveSignals {
            had_activity: true,
            remaining_ratio: Some(0.5),
            reset_in: None,
        };
        assert_eq!(
            account_interval(base, ceiling, true, active),
            base,
            "normal activity keeps the provider cadence"
        );
        let low = AdaptiveSignals {
            had_activity: true,
            remaining_ratio: Some(0.05),
            reset_in: None,
        };
        assert_eq!(
            account_interval(base, ceiling, true, low),
            Duration::from_secs(150)
        );
        let resetting = AdaptiveSignals {
            had_activity: true,
            remaining_ratio: Some(0.5),
            reset_in: Some(Duration::from_secs(300)),
        };
        assert_eq!(
            account_interval(base, ceiling, true, resetting),
            Duration::from_secs(150)
        );
        assert_eq!(
            account_interval(base, ceiling, false, low),
            base,
            "adaptive off ignores every signal"
        );
    }

    #[test]
    fn adaptive_never_crosses_the_floor_or_ceiling() {
        let floor_base = Duration::from_secs(60);
        let low = AdaptiveSignals {
            had_activity: true,
            remaining_ratio: Some(0.0),
            reset_in: None,
        };
        assert_eq!(
            account_interval(floor_base, Duration::from_secs(1_800), true, low),
            Duration::from_secs(60)
        );
        let idle = AdaptiveSignals::default();
        assert_eq!(
            account_interval(
                Duration::from_secs(1_200),
                Duration::from_secs(1_800),
                true,
                idle
            ),
            Duration::from_secs(1_800),
            "doubling stops at the cadence ceiling"
        );
    }

    #[test]
    fn cadence_preference_caps_at_the_freshness_policy() {
        let fallback = Duration::from_secs(1_800);
        assert_eq!(
            cadence(
                fallback,
                RefreshSettingsView {
                    adaptive: false,
                    cadence_minutes: 5
                }
            ),
            Duration::from_secs(300)
        );
        assert_eq!(
            cadence(
                fallback,
                RefreshSettingsView {
                    adaptive: false,
                    cadence_minutes: 30
                }
            ),
            fallback,
            "a requested 30 minutes does not exceed the policy's own target"
        );
        assert_eq!(
            cadence(
                fallback,
                RefreshSettingsView {
                    adaptive: false,
                    cadence_minutes: 0
                }
            ),
            fallback
        );
    }
}
