//! Historical usage data and trend analysis.
//!
//! History points are derived from quota snapshots over time. They enable
//! burn rate calculation, cost tracking, and usage trend visualization.

use crate::domain::provider::{DataSource, QuotaUnit};
use serde::{Deserialize, Serialize};

/// A single historical usage data point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    /// When this data point was observed.
    pub observed_at: String,
    /// Usage amount at this point.
    pub used: Option<f64>,
    /// Limit at this point.
    pub limit: Option<f64>,
    /// Remaining at this point.
    pub remaining: Option<f64>,
    /// Unit of measurement.
    pub unit: Option<QuotaUnit>,
    /// Currency (if applicable).
    pub currency: Option<String>,
    /// Data source.
    pub source: DataSource,
}

impl HistoryPoint {
    /// Create a history point from a snapshot.
    pub fn from_snapshot(
        observed_at: impl Into<String>,
        used: Option<f64>,
        limit: Option<f64>,
        remaining: Option<f64>,
        unit: Option<QuotaUnit>,
        currency: Option<String>,
        source: DataSource,
    ) -> Self {
        Self {
            observed_at: observed_at.into(),
            used,
            limit,
            remaining,
            unit,
            currency,
            source,
        }
    }

    /// Returns true if this point has a valid usage value.
    pub fn has_usage(&self) -> bool {
        self.used.is_some()
    }

    /// Returns the usage percentage (0-100) or None if limit is missing.
    pub fn usage_percent(&self) -> Option<f64> {
        match (self.used, self.limit) {
            (Some(used), Some(limit)) if limit > 0.0 => Some((used / limit) * 100.0),
            _ => None,
        }
    }
}

/// A collection of history points for trend analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySeries {
    /// The history points, ordered by `observed_at`.
    pub points: Vec<HistoryPoint>,
}

impl Default for HistorySeries {
    fn default() -> Self {
        Self::new()
    }
}

impl HistorySeries {
    /// Create a new empty history series.
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    /// Add a point to the series.
    pub fn push(&mut self, point: HistoryPoint) {
        self.points.push(point);
    }

    /// Returns the number of points in the series.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns true if the series is empty.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Calculate the burn rate (usage per day) over the series.
    pub fn burn_rate(&self) -> Option<f64> {
        if self.points.len() < 2 {
            return None;
        }
        let first = self.points.first()?;
        let last = self.points.last()?;
        let first_ts = crate::domain::reset::parse_timestamp(&first.observed_at)?;
        let last_ts = crate::domain::reset::parse_timestamp(&last.observed_at)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let days = (last_ts - first_ts) as f64 / 86400.0;
        if days <= 0.0 {
            return None;
        }
        let usage_diff = last.used? - first.used?;
        Some(usage_diff / days)
    }

    /// Calculate the average usage percentage across all points.
    pub fn average_usage_percent(&self) -> Option<f64> {
        let percents: Vec<f64> = self
            .points
            .iter()
            .filter_map(HistoryPoint::usage_percent)
            .collect();
        if percents.is_empty() {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let avg = percents.iter().sum::<f64>() / percents.len() as f64;
        Some(avg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::DataSource;

    fn history_point(used: f64, limit: f64, observed_at: &str) -> HistoryPoint {
        HistoryPoint::from_snapshot(
            observed_at,
            Some(used),
            Some(limit),
            Some(limit - used),
            Some(QuotaUnit::Token),
            None,
            DataSource::OfficialApi,
        )
    }

    #[test]
    fn history_point_usage_percent_calculates_correctly() {
        let point = history_point(250.0, 1000.0, "2026-10-10T00:00:00Z");
        assert!((point.usage_percent().unwrap() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn history_point_usage_percent_none_without_limit() {
        let point = HistoryPoint::from_snapshot(
            "2026-10-10T00:00:00Z",
            Some(100.0),
            None,
            None,
            Some(QuotaUnit::Token),
            None,
            DataSource::OfficialApi,
        );
        assert!(point.usage_percent().is_none());
    }

    #[test]
    fn history_series_burn_rate_none_with_fewer_than_2_points() {
        let series = HistorySeries::new();
        assert!(series.burn_rate().is_none());
    }

    #[test]
    fn history_series_average_usage_percent() {
        let mut series = HistorySeries::new();
        series.push(history_point(100.0, 1000.0, "2026-10-10T00:00:00Z"));
        series.push(history_point(200.0, 1000.0, "2026-10-11T00:00:00Z"));
        let avg = series.average_usage_percent().unwrap();
        assert!((avg - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn history_series_len_and_is_empty() {
        let mut series = HistorySeries::new();
        assert!(series.is_empty());
        assert_eq!(series.len(), 0);
        series.push(history_point(100.0, 1000.0, "2026-10-10T00:00:00Z"));
        assert!(!series.is_empty());
        assert_eq!(series.len(), 1);
    }
}
