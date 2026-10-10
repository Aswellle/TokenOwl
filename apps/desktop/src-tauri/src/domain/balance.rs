//! Balance display for prepaid and subscription accounts.
//!
//! Balance is independent from quota: it represents the remaining account
//! value without a reset window. Quota windows (5h/7d/monthly) are handled
//! by the reset calculator.

use crate::domain::provider::{DataSource, SnapshotStatus};
use serde::{Deserialize, Serialize};

/// Account balance information.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Balance {
    /// Balance amount.
    pub amount: Option<f64>,
    /// Currency code (e.g., "USD", "CNY").
    pub currency: Option<String>,
    /// Data source.
    pub source: DataSource,
    /// Confidence level.
    pub confidence: crate::domain::provider::Confidence,
    /// Snapshot status.
    pub status: SnapshotStatus,
    /// When the balance was observed.
    pub observed_at: Option<String>,
}

impl Balance {
    /// Create a balance from a quota snapshot.
    pub fn from_snapshot(
        amount: Option<f64>,
        currency: Option<String>,
        source: DataSource,
        observed_at: Option<String>,
    ) -> Self {
        let (confidence, status) = match (amount, &source) {
            (Some(_), DataSource::OfficialApi) => (
                crate::domain::provider::Confidence::Exact,
                SnapshotStatus::Available,
            ),
            (Some(_), _) => (
                crate::domain::provider::Confidence::Estimated,
                SnapshotStatus::Available,
            ),
            (None, _) => (
                crate::domain::provider::Confidence::Unknown,
                SnapshotStatus::Unavailable,
            ),
        };
        Self {
            amount,
            currency,
            source,
            confidence,
            status,
            observed_at,
        }
    }

    /// Returns true if the balance has a valid amount.
    pub fn is_available(&self) -> bool {
        self.amount.is_some()
    }

    /// Returns the balance amount or 0.0 if unavailable.
    pub fn amount_or_zero(&self) -> f64 {
        self.amount.unwrap_or(0.0)
    }

    /// Returns the currency code or empty string.
    pub fn currency_or_empty(&self) -> &str {
        self.currency.as_deref().unwrap_or("")
    }
}

/// Multiple balance buckets (e.g., paid balance, free grant, bonus credits).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BalanceBuckets {
    /// Paid balance.
    pub paid: Option<Balance>,
    /// Free grant balance.
    pub free_grant: Option<Balance>,
    /// Bonus credits.
    pub bonus: Option<Balance>,
    /// Subscription quota.
    pub subscription: Option<Balance>,
    /// Promo credits.
    pub promo: Option<Balance>,
    /// Team pool.
    pub team_pool: Option<Balance>,
}

impl BalanceBuckets {
    /// Returns true if any bucket has a balance.
    pub fn has_any(&self) -> bool {
        self.paid.is_some()
            || self.free_grant.is_some()
            || self.bonus.is_some()
            || self.subscription.is_some()
            || self.promo.is_some()
            || self.team_pool.is_some()
    }

    /// Returns the total number of buckets with data.
    pub fn len(&self) -> usize {
        let mut count = 0;
        if self.paid.is_some() {
            count += 1;
        }
        if self.free_grant.is_some() {
            count += 1;
        }
        if self.bonus.is_some() {
            count += 1;
        }
        if self.subscription.is_some() {
            count += 1;
        }
        if self.promo.is_some() {
            count += 1;
        }
        if self.team_pool.is_some() {
            count += 1;
        }
        count
    }

    /// Returns true if no buckets have data.
    pub fn is_empty(&self) -> bool {
        !self.has_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::DataSource;

    #[test]
    fn balance_from_snapshot_sets_fields() {
        let balance = Balance::from_snapshot(
            Some(12.40),
            Some("USD".to_owned()),
            DataSource::OfficialApi,
            Some("2026-10-10T00:00:00Z".to_owned()),
        );
        assert!((balance.amount.unwrap() - 12.40).abs() < f64::EPSILON);
        assert_eq!(balance.currency.as_deref(), Some("USD"));
        assert!(balance.is_available());
    }

    #[test]
    fn balance_amount_or_zero_returns_zero_when_none() {
        let balance = Balance::default();
        assert!(balance.amount_or_zero().abs() < f64::EPSILON);
        assert!(!balance.is_available());
    }

    #[test]
    fn balance_currency_or_empty_returns_empty_when_none() {
        let balance = Balance::default();
        assert_eq!(balance.currency_or_empty(), "");
    }

    #[test]
    fn balance_buckets_has_any_false_when_empty() {
        let buckets = BalanceBuckets::default();
        assert!(!buckets.has_any());
        assert!(buckets.is_empty());
        assert_eq!(buckets.len(), 0);
    }

    #[test]
    fn balance_buckets_len_counts_non_empty_buckets() {
        let buckets = BalanceBuckets {
            paid: Some(Balance::from_snapshot(
                Some(10.0),
                Some("USD".to_owned()),
                DataSource::OfficialApi,
                None,
            )),
            free_grant: Some(Balance::from_snapshot(
                Some(100.0),
                Some("USD".to_owned()),
                DataSource::OfficialApi,
                None,
            )),
            ..BalanceBuckets::default()
        };
        assert!(buckets.has_any());
        assert!(!buckets.is_empty());
        assert_eq!(buckets.len(), 2);
    }
}
