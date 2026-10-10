//! Application state primitives shared by persistence and background refresh.
//!
//! This module deliberately contains no network or credential-store code. Secret inputs
//! belong in the OS credential store; [`AccountMetadata`] only carries its reference.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

fn is_sensitive_metadata_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    matches!(
        normalized.as_slice(),
        b"apikey"
            | b"apisecretkey"
            | b"secretkey"
            | b"auth_token"
            | b"accesstoken"
            | b"accesstokenkey"
            | b"refreshtoken"
            | b"refreshtokenkey"
            | b"clientsecret"
            | b"password"
            | b"secret"
            | b"sessiontoken"
            | b"sessiontokenkey"
            | b"privatekey"
            | b"api_token"
            | b"authorization"
            | b"bearer"
            | b"cookie"
            | b"setcookie"
            | b"token"
            | b"credential"
    )
}

fn sanitize_metadata(mut metadata: HashMap<String, String>) -> HashMap<String, String> {
    metadata.retain(|key, _| !is_sensitive_metadata_key(key));
    metadata
}
/// Non-secret account information persisted by the application.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountMetadata {
    pub id: String,
    pub provider_id: String,
    pub label: String,
    pub account_type: String,
    pub credential_ref: Option<String>,
    pub metadata: HashMap<String, String>,
    pub enabled: bool,
}

impl AccountMetadata {
    /// Builds metadata without retaining the supplied secret input.
    #[allow(clippy::too_many_arguments)]
    pub fn from_auth_input(
        id: impl Into<String>,
        provider_id: impl Into<String>,
        label: impl Into<String>,
        account_type: impl Into<String>,
        credential_ref: Option<String>,
        metadata: HashMap<String, String>,
        enabled: bool,
        _secret_input: Option<&str>,
    ) -> Self {
        Self {
            id: id.into(),
            provider_id: provider_id.into(),
            label: label.into(),
            account_type: account_type.into(),
            credential_ref,
            metadata: sanitize_metadata(metadata),
            enabled,
        }
    }
}

/// A normalized quota observation. Raw provider responses are intentionally absent.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Snapshot {
    pub id: String,
    pub account_id: String,
    pub metric_id: String,
    pub metric_name: String,
    pub unit: String,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub observed_at: u64,
    pub source: String,
    pub confidence: String,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Freshness {
    Fresh,
    Stale,
    Expired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FreshnessPolicy {
    pub fresh_for: Duration,
    pub stale_after: Duration,
    pub hard_expire_after: Duration,
}

impl FreshnessPolicy {
    pub fn classify(&self, observed_at: u64, now: u64) -> Freshness {
        let age = Duration::from_secs(now.saturating_sub(observed_at));
        if age < self.fresh_for {
            Freshness::Fresh
        } else if age < self.hard_expire_after {
            Freshness::Stale
        } else {
            Freshness::Expired
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SnapshotView {
    pub snapshot: Snapshot,
    pub freshness: Freshness,
}

#[derive(Clone, Debug, Default)]
pub struct SnapshotStore {
    latest: HashMap<String, Snapshot>,
}

impl SnapshotStore {
    pub fn insert(&mut self, snapshot: Snapshot) {
        let replace = self
            .latest
            .get(&snapshot.account_id)
            .is_none_or(|old| snapshot.observed_at >= old.observed_at);
        if replace {
            self.latest.insert(snapshot.account_id.clone(), snapshot);
        }
    }

    pub fn latest(&self, account_id: &str) -> Option<&Snapshot> {
        self.latest.get(account_id)
    }

    /// Returns the last known snapshot, including a stale one, but never an expired one.
    pub fn view(
        &self,
        account_id: &str,
        now: u64,
        policy: FreshnessPolicy,
    ) -> Option<SnapshotView> {
        let snapshot = self.latest(account_id)?.clone();
        let freshness = policy.classify(snapshot.observed_at, now);
        (freshness != Freshness::Expired).then_some(SnapshotView {
            snapshot,
            freshness,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PositionSettings {
    pub position: Position,
    pub x: i32,
    pub y: i32,
}

impl PositionSettings {
    /// Clamps coordinates to the available work area, preserving the chosen corner.
    pub fn normalized(
        self,
        work_width: i32,
        work_height: i32,
        window_width: i32,
        window_height: i32,
    ) -> Self {
        let max_x = (work_width - window_width).max(0);
        let max_y = (work_height - window_height).max(0);
        let x = self.x.clamp(0, max_x);
        let y = self.y.clamp(0, max_y);
        Self {
            position: self.position,
            x,
            y,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RefreshStatus {
    Idle,
    Refreshing,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RefreshState {
    pub status: RefreshStatus,
    pub consecutive_failures: u32,
    pub last_started_at: Option<u64>,
    pub last_completed_at: Option<u64>,
    pub next_attempt_at: Option<u64>,
    pub error_code: Option<String>,
}

impl Default for RefreshState {
    fn default() -> Self {
        Self {
            status: RefreshStatus::Idle,
            consecutive_failures: 0,
            last_started_at: None,
            last_completed_at: None,
            next_attempt_at: None,
            error_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub cap: Duration,
    pub jitter: Duration,
}

impl BackoffPolicy {
    /// Computes deterministic retry delay from a caller-provided jitter sample in `[0, 1]`.
    pub fn delay(&self, consecutive_failures: u32, jitter_sample: f64) -> Duration {
        if self.cap.is_zero() {
            return Duration::ZERO;
        }
        let exponent = consecutive_failures.saturating_sub(1).min(31);
        let multiplier = 1u32 << exponent;
        let exponential = self.base.saturating_mul(multiplier).min(self.cap);
        let jitter = self.jitter.mul_f64(jitter_sample.clamp(0.0, 1.0));
        exponential.saturating_add(jitter).min(self.cap)
    }

    /// Compute delay for a specific retry attempt (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        self.delay(attempt, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> FreshnessPolicy {
        FreshnessPolicy {
            fresh_for: Duration::from_secs(600),
            stale_after: Duration::from_secs(1800),
            hard_expire_after: Duration::from_secs(86400),
        }
    }

    fn snapshot(observed_at: u64) -> Snapshot {
        Snapshot {
            id: "s".into(),
            account_id: "a".into(),
            metric_id: "m".into(),
            metric_name: "Metric".into(),
            unit: "count".into(),
            used: None,
            limit: None,
            remaining: None,
            observed_at,
            source: "fixture".into(),
            confidence: "high".into(),
            status: "ok".into(),
        }
    }

    #[test]
    fn freshness_boundaries_are_explicit() {
        assert_eq!(policy().classify(1_000, 1_599), Freshness::Fresh);
        assert_eq!(policy().classify(1_000, 1_600), Freshness::Stale);
        assert_eq!(policy().classify(1_000, 2_800), Freshness::Stale);
        assert_eq!(policy().classify(1_000, 87_400), Freshness::Expired);
    }

    #[test]
    fn stale_snapshot_is_fallback_until_hard_expiry() {
        let mut store = SnapshotStore::default();
        store.insert(snapshot(1_000));
        assert_eq!(
            store.view("a", 2_000, policy()).unwrap().freshness,
            Freshness::Stale
        );
        assert!(store.view("a", 90_000, policy()).is_none());
    }

    #[test]
    fn backoff_is_exponential_capped_and_jitter_bounded() {
        let policy = BackoffPolicy {
            base: Duration::from_secs(300),
            cap: Duration::from_secs(3600),
            jitter: Duration::from_secs(30),
        };
        assert_eq!(policy.delay(1, 0.0), Duration::from_secs(300));
        assert_eq!(policy.delay(2, 0.0), Duration::from_secs(600));
        assert_eq!(policy.delay(3, 1.0), Duration::from_secs(1230));
        assert_eq!(policy.delay(20, 0.0), Duration::from_secs(3600));
        assert!(policy.delay(3, 0.0) <= policy.delay(3, 1.0));
        assert!(policy.delay(3, 1.0) <= policy.cap);
    }

    #[test]
    fn position_normalization_clamps_to_work_area() {
        let settings = PositionSettings {
            position: Position::BottomRight,
            x: -5,
            y: 10_000,
        };
        assert_eq!(
            settings.normalized(1_920, 1_080, 200, 100),
            PositionSettings {
                position: Position::BottomRight,
                x: 0,
                y: 980
            }
        );
    }

    #[test]
    fn auth_input_drops_secret_metadata_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("region".into(), "us".into());
        metadata.insert("api_key".into(), "not-retained".into());
        let account = AccountMetadata::from_auth_input(
            "a",
            "p",
            "label",
            "api",
            None,
            metadata,
            true,
            Some("secret"),
        );
        assert_eq!(account.metadata.get("region"), Some(&"us".to_string()));
        assert!(!account.metadata.contains_key("api_key"));
    }
}
