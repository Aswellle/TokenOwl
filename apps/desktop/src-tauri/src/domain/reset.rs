//! Reset window calculation for quota snapshots.
//!
//! Supports 5h/7d rolling windows, daily/monthly calendar windows,
//! fixed-duration windows, and provider-supplied reset timestamps.

use crate::domain::provider::{QuotaSnapshot, QuotaWindow, WindowType};
use std::time::{SystemTime, UNIX_EPOCH};

/// Reset calculation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetInfo {
    /// Provider-supplied reset timestamp (if available).
    pub reset_at: Option<String>,
    /// Calculated reset timestamp based on window rules.
    pub calculated_reset_at: Option<String>,
    /// Seconds until reset. `Some(0)` means a trusted reset time has already
    /// passed; `None` means the reset time is unknown — never a fabricated
    /// zero.
    pub remaining_seconds: Option<u64>,
    /// Window type used for calculation.
    pub window_type: Option<WindowType>,
}

impl ResetInfo {
    /// No reset information available.
    pub fn unknown() -> Self {
        Self {
            reset_at: None,
            calculated_reset_at: None,
            remaining_seconds: None,
            window_type: None,
        }
    }
}

/// Calculates reset times for quota snapshots.
pub struct ResetCalculator;

impl ResetCalculator {
    /// Calculate reset info for a snapshot.
    pub fn calculate(snapshot: &QuotaSnapshot) -> ResetInfo {
        Self::calculate_with_now(snapshot, unix_timestamp())
    }

    /// Pure variant of [`Self::calculate`] with an injected clock, so tests
    /// can pin boundary behavior instead of racing the wall clock.
    pub fn calculate_with_now(snapshot: &QuotaSnapshot, now: u64) -> ResetInfo {
        let Some(window) = snapshot.window.as_ref() else {
            return ResetInfo::unknown();
        };

        let window_type = window.window_type.unwrap_or(WindowType::Fixed);

        // A trusted, parseable provider timestamp wins. An unparseable one
        // must never read as "already reset" (`Some(0)`); the best available
        // fallback is deriving the reset from complete window fields, and
        // when even that fails the answer is unknown.
        if let Some(reset_at) = &snapshot.reset_at {
            let trusted = parse_timestamp(reset_at).map(|ts| Self::from_timestamp(ts, now));
            let (calculated_reset_at, remaining) = match trusted {
                Some(info) => info,
                None => match Self::calculate_from_window(window, window_type) {
                    Some(ts) => Self::from_timestamp(ts, now),
                    None => (None, None),
                },
            };
            return ResetInfo {
                reset_at: Some(reset_at.clone()),
                calculated_reset_at,
                remaining_seconds: remaining,
                window_type: Some(window_type),
            };
        }

        let calculated = Self::calculate_from_window(window, window_type);
        let (calculated_reset_at, remaining) =
            calculated.map_or((None, None), |ts| Self::from_timestamp(ts, now));

        ResetInfo {
            reset_at: snapshot.reset_at.clone(),
            calculated_reset_at,
            remaining_seconds: remaining,
            window_type: Some(window_type),
        }
    }

    /// Formats a trusted reset timestamp and its remaining seconds.
    ///
    /// A timestamp at or before `now` is genuinely elapsed: that is the only
    /// case that reports `Some(0)`.
    fn from_timestamp(ts: u64, now: u64) -> (Option<String>, Option<u64>) {
        let datetime = chrono::DateTime::from_timestamp(i64::try_from(ts).unwrap_or(i64::MAX), 0);
        let calculated_reset_at = Some(datetime.map_or_else(
            || ts.to_string(),
            |dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ));
        (calculated_reset_at, Some(ts.saturating_sub(now)))
    }

    /// Derives the reset instant from window fields alone.
    ///
    /// Every rule requires the fields it needs to be present and sane; a
    /// window that cannot honestly produce an instant yields `None` (unknown)
    /// instead of silently anchoring on the current time or a zero duration.
    fn calculate_from_window(window: &QuotaWindow, window_type: WindowType) -> Option<u64> {
        let sane_duration = || {
            window
                .duration_seconds
                .and_then(|d| u64::try_from(d).ok())
                .filter(|d| d > &0)
        };
        let parse_starts_at = || window.starts_at.as_deref().and_then(parse_timestamp);

        match window_type {
            // A rolling window needs both its start and a positive length.
            WindowType::Rolling => {
                let start = parse_starts_at()?;
                Some(start.saturating_add(sane_duration()?))
            }
            // A fixed window prefers an explicit end; otherwise it needs a
            // complete start+duration pair.
            WindowType::Fixed => {
                if let Some(ends_at) = &window.ends_at {
                    return parse_timestamp(ends_at);
                }
                let start = parse_starts_at()?;
                Some(start.saturating_add(sane_duration()?))
            }
            // Calendar-like windows only end when the provider says so.
            WindowType::Calendar | WindowType::Subscription => {
                window.ends_at.as_deref().and_then(parse_timestamp)
            }
            // No custom calculator exists, so no reset can be derived.
            WindowType::Custom => None,
        }
    }

    /// Calculate remaining seconds until reset.
    pub fn remaining_seconds(snapshot: &QuotaSnapshot) -> Option<u64> {
        Self::calculate(snapshot).remaining_seconds
    }

    /// Check if the snapshot has a valid reset time.
    pub fn has_reset(snapshot: &QuotaSnapshot) -> bool {
        Self::calculate(snapshot).remaining_seconds.is_some()
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn parse_timestamp(value: &str) -> Option<u64> {
    if let Ok(ts) = value.parse::<u64>() {
        return Some(if ts < 1_000_000_000_000 {
            ts
        } else {
            ts / 1000
        });
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return u64::try_from(dt.timestamp()).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::{Confidence, DataSource, SnapshotStatus};

    fn snapshot_with_window(window: QuotaWindow) -> QuotaSnapshot {
        QuotaSnapshot {
            id: Some("snap-1".to_owned()),
            account_id: Some("acct-1".to_owned()),
            metric_id: Some("quota".to_owned()),
            metric_name: Some("5h quota".to_owned()),
            unit: Some(crate::domain::provider::QuotaUnit::Token),
            currency: None,
            used: Some(100.0),
            limit: Some(1000.0),
            remaining: Some(900.0),
            window: Some(window),
            reset_at: None,
            observed_at: Some("2026-10-10T00:00:00Z".to_owned()),
            source: DataSource::OfficialApi,
            confidence: Confidence::Exact,
            status: SnapshotStatus::Available,
        }
    }

    #[test]
    fn provider_reset_at_takes_priority() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: Some("2026-10-10T12:00:00Z".to_owned()),
        });
        snap.reset_at = Some("2026-10-10T12:00:00Z".to_owned());
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.reset_at.as_deref(), Some("2026-10-10T12:00:00Z"));
        assert!(info.remaining_seconds.is_some());
    }

    #[test]
    fn rolling_window_calculates_from_start_and_duration() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(18000),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Rolling));
        assert!(info.calculated_reset_at.is_some());
    }

    #[test]
    fn fixed_window_uses_ends_at_when_available() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: Some("2026-10-10T01:00:00Z".to_owned()),
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(
            info.calculated_reset_at.as_deref(),
            Some("2026-10-10T01:00:00Z")
        );
    }

    #[test]
    fn no_window_returns_unknown() {
        let mut snap = snapshot_with_window(QuotaWindow::default());
        snap.window = None;
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.remaining_seconds, None);
    }

    #[test]
    fn remaining_seconds_is_none_without_reset_info() {
        let mut snap = snapshot_with_window(QuotaWindow::default());
        snap.window = None;
        assert_eq!(ResetCalculator::remaining_seconds(&snap), None);
    }

    #[test]
    fn has_reset_false_without_window() {
        let mut snap = snapshot_with_window(QuotaWindow::default());
        snap.window = None;
        assert!(!ResetCalculator::has_reset(&snap));
    }

    #[test]
    fn rolling_5h_window_calculates_correctly() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(18000),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Rolling));
        let calculated = info.calculated_reset_at.expect("calculated reset");
        assert!(calculated.contains("2026-10-10T05:00:00"));
    }

    #[test]
    fn rolling_7d_window_calculates_correctly() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(604_800),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Rolling));
        let calculated = info.calculated_reset_at.expect("calculated reset");
        assert!(calculated.contains("2026-10-17T00:00:00"));
    }

    #[test]
    fn fixed_window_without_ends_at_uses_duration() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: Some("2026-10-10T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        let calculated = info.calculated_reset_at.expect("calculated reset");
        assert!(calculated.contains("2026-10-10T01:00:00"));
    }

    #[test]
    fn calendar_window_uses_ends_at() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Calendar),
            duration_seconds: None,
            starts_at: Some("2026-10-01T00:00:00Z".to_owned()),
            ends_at: Some("2026-10-31T23:59:59Z".to_owned()),
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Calendar));
        assert_eq!(
            info.calculated_reset_at.as_deref(),
            Some("2026-10-31T23:59:59Z")
        );
    }

    #[test]
    fn subscription_window_uses_ends_at() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Subscription),
            duration_seconds: None,
            starts_at: Some("2026-09-01T00:00:00Z".to_owned()),
            ends_at: Some("2026-11-01T00:00:00Z".to_owned()),
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Subscription));
        assert_eq!(
            info.calculated_reset_at.as_deref(),
            Some("2026-11-01T00:00:00Z")
        );
    }

    #[test]
    fn custom_window_without_ends_at_returns_none() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Custom),
            duration_seconds: None,
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.window_type, Some(WindowType::Custom));
        assert_eq!(info.calculated_reset_at, None);
    }

    #[test]
    fn parse_timestamp_handles_milliseconds() {
        let ts = 1_700_000_000_000u64;
        let result = parse_timestamp(&ts.to_string());
        assert_eq!(result, Some(1_700_000_000));
    }

    #[test]
    fn parse_timestamp_handles_iso_with_timezone() {
        let result = parse_timestamp("2026-10-10T12:00:00+08:00");
        assert!(result.is_some());
    }

    #[test]
    fn parse_timestamp_returns_none_for_invalid() {
        assert_eq!(parse_timestamp("not-a-timestamp"), None);
        assert_eq!(parse_timestamp(""), None);
    }

    #[test]
    fn remaining_seconds_clamps_to_zero_when_past() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: Some("2020-01-01T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        snap.reset_at = Some("2020-01-01T01:00:00Z".to_owned());
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.remaining_seconds, Some(0));
    }

    // ---- Unknown-vs-zero regression matrix (TO-001 contract) ----
    //
    // Every case below must yield `remaining_seconds: None` / `has_reset()`
    // false: a missing, malformed, or underivable reset time is unknown, and
    // `Some(0)` is reserved for a trusted timestamp that has already passed.

    #[test]
    fn custom_window_without_rule_is_unknown_not_zero() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Custom),
            duration_seconds: None,
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(
            info.remaining_seconds, None,
            "Custom with no rule is unknown"
        );
        assert_eq!(info.calculated_reset_at, None);
        assert!(!ResetCalculator::has_reset(&snap));
    }

    #[test]
    fn calendar_window_without_ends_at_is_unknown_not_zero() {
        for window_type in [WindowType::Calendar, WindowType::Subscription] {
            let snap = snapshot_with_window(QuotaWindow {
                window_type: Some(window_type),
                duration_seconds: Some(3600),
                starts_at: Some("2026-10-01T00:00:00Z".to_owned()),
                ends_at: None,
                reset_at: None,
            });
            let info = ResetCalculator::calculate(&snap);
            assert_eq!(
                info.remaining_seconds, None,
                "{window_type:?} without ends_at is unknown"
            );
            assert!(!ResetCalculator::has_reset(&snap));
        }
    }

    #[test]
    fn rolling_window_without_start_is_unknown_not_zero() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(18_000),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        // The old code fabricated `now + duration` from the wall clock.
        assert_eq!(info.remaining_seconds, None);
        assert_eq!(info.calculated_reset_at, None);
    }

    #[test]
    fn window_with_zero_duration_is_unknown_not_zero() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(0),
            starts_at: Some("2026-10-01T00:00:00Z".to_owned()),
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(
            info.remaining_seconds, None,
            "a zero-length window is unknown"
        );
    }

    #[test]
    fn window_with_negative_duration_is_unknown_not_zero() {
        for window_type in [WindowType::Rolling, WindowType::Fixed] {
            let snap = snapshot_with_window(QuotaWindow {
                window_type: Some(window_type),
                duration_seconds: Some(-3_600),
                starts_at: Some("2026-10-01T00:00:00Z".to_owned()),
                ends_at: None,
                reset_at: None,
            });
            let info = ResetCalculator::calculate(&snap);
            assert_eq!(
                info.remaining_seconds, None,
                "{window_type:?} with a negative duration is unknown"
            );
        }
    }

    #[test]
    fn fixed_window_without_any_derivable_field_is_unknown() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: None,
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.remaining_seconds, None);
    }

    #[test]
    fn invalid_reset_at_string_is_not_reported_as_expired() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        snap.reset_at = Some("not-a-timestamp".to_owned());
        let info = ResetCalculator::calculate(&snap);
        // No window fallback is derivable either, so the result is unknown —
        // never `Some(0)`, which would read as "already reset".
        assert_eq!(info.remaining_seconds, None);
        assert_eq!(info.calculated_reset_at, None);
        assert!(!ResetCalculator::has_reset(&snap));
    }

    #[test]
    fn invalid_reset_at_falls_back_to_a_valid_window_end() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: None,
            starts_at: None,
            ends_at: Some("2030-01-01T00:00:00Z".to_owned()),
            reset_at: None,
        });
        snap.reset_at = Some("garbage".to_owned());
        let info = ResetCalculator::calculate(&snap);
        let remaining = info.remaining_seconds.expect("window fallback is trusted");
        assert!(
            remaining > 0,
            "fallback must derive a future reset, got {remaining}"
        );
        let calculated = info.calculated_reset_at.expect("fallback timestamp");
        assert!(calculated.contains("2030-01-01"), "got {calculated}");
    }

    #[test]
    fn valid_future_reset_at_reports_positive_remaining() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        snap.reset_at = Some("2030-01-01T00:00:00Z".to_owned());
        let info = ResetCalculator::calculate(&snap);
        let remaining = info.remaining_seconds.expect("valid future reset");
        assert!(remaining > 0);
    }

    #[test]
    fn epoch_seconds_and_milliseconds_reset_at_are_normalized() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        // 2030-01-01T00:00:00Z as epoch seconds and as milliseconds.
        for raw in ["1_897_324_800", "1897324800000"] {
            let raw = raw.replace('_', "");
            snap.reset_at = Some(raw);
            let info = ResetCalculator::calculate(&snap);
            let remaining = info.remaining_seconds.expect("epoch reset parses");
            assert!(remaining > 0, "2030 reset must still be in the future");
        }
    }

    #[test]
    fn reset_at_equal_to_now_reports_zero_remaining() {
        let mut snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: Some(3600),
            starts_at: None,
            ends_at: None,
            reset_at: None,
        });
        // The current second, ±: a trusted timestamp that has effectively
        // arrived. `Some(0)` (or a sub-second remainder) is the honest answer.
        let now = unix_timestamp();
        snap.reset_at = Some(now.to_string());
        let info = ResetCalculator::calculate(&snap);
        assert_eq!(info.remaining_seconds, Some(0));
    }

    #[test]
    fn injected_now_pins_remaining_boundaries() {
        let snap = snapshot_with_window(QuotaWindow {
            window_type: Some(WindowType::Fixed),
            duration_seconds: None,
            starts_at: None,
            ends_at: Some("2026-10-10T12:00:00Z".to_owned()),
            reset_at: None,
        });
        let ends_at = parse_timestamp("2026-10-10T12:00:00Z").expect("valid fixture");
        // Exactly at the end: the trusted window has elapsed → Some(0).
        assert_eq!(
            ResetCalculator::calculate_with_now(&snap, ends_at).remaining_seconds,
            Some(0)
        );
        // One second before: one second left.
        assert_eq!(
            ResetCalculator::calculate_with_now(&snap, ends_at - 1).remaining_seconds,
            Some(1)
        );
        // Long after: clamped to zero, still known.
        assert_eq!(
            ResetCalculator::calculate_with_now(&snap, ends_at + 86_400).remaining_seconds,
            Some(0)
        );
    }
}
