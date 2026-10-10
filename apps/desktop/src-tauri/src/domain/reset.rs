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
    /// Seconds until reset (0 if already reset).
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
        let Some(window) = snapshot.window.as_ref() else {
            return ResetInfo::unknown();
        };

        let window_type = window.window_type.unwrap_or(WindowType::Fixed);
        let now = unix_timestamp();

        // Provider-supplied reset_at takes priority
        if let Some(reset_at) = &snapshot.reset_at {
            let remaining = parse_timestamp(reset_at).map_or(0, |ts| ts.saturating_sub(now));
            return ResetInfo {
                reset_at: Some(reset_at.clone()),
                calculated_reset_at: Some(reset_at.clone()),
                remaining_seconds: Some(remaining),
                window_type: Some(window_type),
            };
        }

        // Calculate from window rules
        let calculated = Self::calculate_from_window(window, window_type, now);
        let remaining = calculated.map_or(0, |ts| ts.saturating_sub(now));
        let calculated_reset_at = calculated.map_or_else(
            || None,
            |ts| {
                let datetime =
                    chrono::DateTime::from_timestamp(i64::try_from(ts).unwrap_or(i64::MAX), 0);
                Some(datetime.map_or_else(
                    || ts.to_string(),
                    |dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                ))
            },
        );

        ResetInfo {
            reset_at: snapshot.reset_at.clone(),
            calculated_reset_at,
            remaining_seconds: Some(remaining),
            window_type: Some(window_type),
        }
    }

    fn calculate_from_window(
        window: &QuotaWindow,
        window_type: WindowType,
        now: u64,
    ) -> Option<u64> {
        match window_type {
            WindowType::Rolling => {
                let start = window
                    .starts_at
                    .as_deref()
                    .and_then(parse_timestamp)
                    .unwrap_or(now);
                let duration = window
                    .duration_seconds
                    .and_then(|d| u64::try_from(d).ok())
                    .unwrap_or(0);
                Some(start.saturating_add(duration))
            }
            WindowType::Fixed => {
                if let Some(ends_at) = &window.ends_at {
                    return parse_timestamp(ends_at);
                }
                let start = window
                    .starts_at
                    .as_deref()
                    .and_then(parse_timestamp)
                    .unwrap_or(now);
                let duration = window
                    .duration_seconds
                    .and_then(|d| u64::try_from(d).ok())
                    .unwrap_or(0);
                Some(start.saturating_add(duration))
            }
            WindowType::Calendar | WindowType::Subscription => {
                window.ends_at.as_deref().and_then(parse_timestamp)
            }
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
}
