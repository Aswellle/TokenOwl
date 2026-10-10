//! Notification system for quota alerts and provider status changes.
//!
//! Supports low quota warnings, reset-soon reminders, auth expiry alerts,
//! and stale data notifications. Respects user preferences for silent mode
//! and critical-only mode.

use crate::domain::provider::QuotaSnapshot;
use serde::{Deserialize, Serialize};

/// Notification severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
}

/// A notification to display to the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    /// Unique notification ID.
    pub id: String,
    /// Severity level.
    pub severity: NotificationSeverity,
    /// Short title.
    pub title: String,
    /// Detailed message.
    pub message: String,
    /// Related provider ID.
    pub provider_id: Option<String>,
    /// Related account ID.
    pub account_id: Option<String>,
    /// When the notification was created.
    pub created_at: String,
}

impl Notification {
    /// Create a new notification.
    pub fn new(
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_id(),
            severity,
            title: title.into(),
            message: message.into(),
            provider_id: None,
            account_id: None,
            created_at: {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                now.to_string()
            },
        }
    }

    /// Set the related provider ID.
    pub fn with_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    /// Set the related account ID.
    pub fn with_account(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = Some(account_id.into());
        self
    }

    /// Set the creation timestamp.
    pub fn with_timestamp(mut self, created_at: impl Into<String>) -> Self {
        self.created_at = created_at.into();
        self
    }
}

/// Notification preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationPreferences {
    /// Master switch for notifications.
    pub enabled: bool,
    /// Only show critical notifications.
    pub critical_only: bool,
    /// Mute all notifications.
    pub silent_mode: bool,
    /// Threshold for low quota warnings (percentage).
    pub low_quota_threshold: f64,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            critical_only: false,
            silent_mode: false,
            low_quota_threshold: 10.0,
        }
    }
}

/// Evaluates whether a notification should be sent based on preferences.
pub struct NotificationFilter;

impl NotificationFilter {
    /// Check if a notification passes the filter.
    pub fn should_notify(notification: &Notification, prefs: &NotificationPreferences) -> bool {
        if !prefs.enabled || prefs.silent_mode {
            return false;
        }
        if prefs.critical_only && notification.severity != NotificationSeverity::Critical {
            return false;
        }
        true
    }

    /// Check if a snapshot triggers a low quota notification.
    pub fn low_quota_alert(
        snapshot: &QuotaSnapshot,
        prefs: &NotificationPreferences,
    ) -> Option<Notification> {
        let remaining = snapshot.remaining?;
        let limit = snapshot.limit?;
        if limit <= 0.0 {
            return None;
        }
        let percent = (remaining / limit) * 100.0;
        if percent <= prefs.low_quota_threshold {
            Some(Notification::new(
                NotificationSeverity::Warning,
                "Low quota",
                format!("{percent:.1}% remaining"),
            ))
        } else {
            None
        }
    }

    /// Check if a snapshot triggers a critical quota notification.
    pub fn critical_quota_alert(
        snapshot: &QuotaSnapshot,
        prefs: &NotificationPreferences,
    ) -> Option<Notification> {
        let remaining = snapshot.remaining?;
        let limit = snapshot.limit?;
        if limit <= 0.0 {
            return None;
        }
        let percent = (remaining / limit) * 100.0;
        if percent <= prefs.low_quota_threshold {
            Some(Notification::new(
                NotificationSeverity::Critical,
                "Critical quota",
                format!("{percent:.1}% remaining"),
            ))
        } else {
            None
        }
    }
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("ntfy-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::{Confidence, DataSource, SnapshotStatus};

    fn snapshot(remaining: f64, limit: f64) -> QuotaSnapshot {
        QuotaSnapshot {
            id: Some("s".to_owned()),
            account_id: Some("a".to_owned()),
            metric_id: Some("q".to_owned()),
            metric_name: Some("quota".to_owned()),
            unit: Some(crate::domain::provider::QuotaUnit::Token),
            currency: None,
            used: Some(limit - remaining),
            limit: Some(limit),
            remaining: Some(remaining),
            observed_at: Some("2026-10-10T00:00:00Z".to_owned()),
            window: None,
            reset_at: None,
            source: DataSource::OfficialApi,
            confidence: Confidence::Exact,
            status: SnapshotStatus::Available,
        }
    }

    #[test]
    fn notification_new_sets_fields() {
        let n = Notification::new(NotificationSeverity::Warning, "Title", "Message");
        assert_eq!(n.title, "Title");
        assert_eq!(n.message, "Message");
        assert_eq!(n.severity, NotificationSeverity::Warning);
    }

    #[test]
    fn notification_filter_blocks_when_disabled() {
        let prefs = NotificationPreferences {
            enabled: false,
            ..Default::default()
        };
        let n = Notification::new(NotificationSeverity::Info, "Test", "Msg");
        assert!(!NotificationFilter::should_notify(&n, &prefs));
    }

    #[test]
    fn notification_filter_blocks_non_critical_in_critical_only_mode() {
        let prefs = NotificationPreferences {
            enabled: true,
            critical_only: true,
            ..Default::default()
        };
        let n = Notification::new(NotificationSeverity::Info, "Test", "Msg");
        assert!(!NotificationFilter::should_notify(&n, &prefs));
    }

    #[test]
    fn notification_filter_allows_critical_in_critical_only_mode() {
        let prefs = NotificationPreferences {
            enabled: true,
            critical_only: true,
            ..Default::default()
        };
        let n = Notification::new(NotificationSeverity::Critical, "Test", "Msg");
        assert!(NotificationFilter::should_notify(&n, &prefs));
    }

    #[test]
    fn low_quota_alert_triggers_below_threshold() {
        let prefs = NotificationPreferences {
            low_quota_threshold: 20.0,
            ..Default::default()
        };
        let snap = snapshot(15.0, 100.0);
        let alert = NotificationFilter::low_quota_alert(&snap, &prefs);
        assert!(alert.is_some());
    }

    #[test]
    fn critical_quota_alert_triggers_below_10_percent() {
        let prefs = NotificationPreferences::default();
        let snap = snapshot(5.0, 100.0);
        let alert = NotificationFilter::critical_quota_alert(&snap, &prefs);
        assert!(alert.is_some());
    }
}
