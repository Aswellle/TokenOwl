//! Historical usage data and trend analysis.
//!
//! History points are derived from quota snapshots over time. They enable
//! burn rate calculation, cost tracking, and usage trend visualization.

use crate::domain::provider::{DataSource, QuotaUnit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One recorded observation handed to the daily aggregator. `unit` is the
/// lowercase string form of `QuotaUnit` as stored by the persistence layer.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySample {
    pub account_id: String,
    pub metric_key: String,
    pub observed_at: u64,
    pub used: Option<f64>,
    pub remaining: Option<f64>,
    pub unit: String,
    pub currency: Option<String>,
}

/// Token and cost totals for one local calendar day. `cost` is the dominant
/// currency's total when a day mixes currencies — the full per-currency split
/// stays visible in per-metric data, not in the heatmap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsage {
    /// UTC timestamp of local midnight for this day bucket.
    pub day_start: u64,
    pub tokens_used: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    /// True when at least one observation falls in this day. Days without
    /// observations are unknown, not zero — the heatmap renders them as empty.
    pub has_data: bool,
}

/// Aggregates recorded observations into per-day token and cost totals.
///
/// Consumption between two observations is the positive increment of `used`
/// (a decreasing value means the provider window reset, and the gap before a
/// reset cannot be reconstructed, so it is never invented). Currency metrics
/// contribute cost through `used` increments; balance-style metrics that only
/// report `remaining` contribute through remaining decreases. A top-up
/// (remaining increase) is not a cost. Other units mark the day as measured
/// but add nothing. `local_offset(timestamp)` supplies the machine's UTC
/// offset in seconds so day boundaries follow the user's calendar.
struct Bucket {
    day_start: u64,
    tokens: f64,
    costs: BTreeMap<String, f64>,
    has_data: bool,
}

pub fn aggregate_daily_usage(
    samples: &[DailySample],
    local_offset: impl Fn(i64) -> i32,
) -> Vec<DailyUsage> {
    // Consecutive deltas are only meaningful within one metric of one account.
    let mut series: BTreeMap<(&str, &str), Vec<&DailySample>> = BTreeMap::new();
    for sample in samples {
        series
            .entry((sample.account_id.as_str(), sample.metric_key.as_str()))
            .or_default()
            .push(sample);
    }

    let mut buckets: BTreeMap<i64, Bucket> = BTreeMap::new();

    for metric_samples in series.values_mut() {
        metric_samples.sort_by_key(|sample| sample.observed_at);
        let mut previous: Option<&DailySample> = None;
        for sample in metric_samples {
            let timestamp = i64::try_from(sample.observed_at).unwrap_or(i64::MAX);
            let offset = i64::from(local_offset(timestamp));
            let day_key = (timestamp + offset).div_euclid(86_400);
            let day_start = u64::try_from(day_key * 86_400 - offset).unwrap_or(0);
            let bucket = buckets.entry(day_key).or_insert(Bucket {
                day_start,
                tokens: 0.0,
                costs: BTreeMap::new(),
                has_data: false,
            });
            bucket.has_data = true;

            if let Some(previous) = previous {
                let positive = |delta: f64| delta.is_finite() && delta > 0.0;
                if let (Some(prev_used), Some(used)) = (previous.used, sample.used) {
                    let delta = used - prev_used;
                    if positive(delta) {
                        match sample.unit.as_str() {
                            "token" => bucket.tokens += delta,
                            "currency" => {
                                let key = sample.currency.clone().unwrap_or_default();
                                *bucket.costs.entry(key).or_insert(0.0) += delta;
                            }
                            _ => {}
                        }
                    }
                } else if sample.unit == "currency" {
                    // Balance-style metric: a remaining decrease is spend, a
                    // remaining increase is a top-up and costs nothing.
                    if let (Some(prev_remaining), Some(remaining)) =
                        (previous.remaining, sample.remaining)
                    {
                        let delta = prev_remaining - remaining;
                        if positive(delta) {
                            let key = sample.currency.clone().unwrap_or_default();
                            *bucket.costs.entry(key).or_insert(0.0) += delta;
                        }
                    }
                }
            }
            previous = Some(sample);
        }
    }

    buckets
        .into_values()
        .map(|bucket| {
            let mut dominant_key = String::new();
            let mut dominant_total = f64::MIN;
            for (key, total) in &bucket.costs {
                if *total > dominant_total {
                    dominant_total = *total;
                    dominant_key.clone_from(key);
                }
            }
            let cost = bucket.costs.get(&dominant_key).copied();
            DailyUsage {
                day_start: bucket.day_start,
                tokens_used: bucket.tokens,
                cost: cost.filter(|total| *total > 0.0),
                currency: if dominant_key.is_empty() {
                    None
                } else {
                    Some(dominant_key)
                },
                has_data: bucket.has_data,
            }
        })
        .collect()
}

/// Token and cost totals for one observation window (e.g. last 2 hours).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeUsage {
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    /// True when at least one observation falls inside the window. A window
    /// with no observations is unknown, not zero — recording may not have
    /// reached back that far.
    pub has_data: bool,
}

/// Sums usage inside `[from, to)`. Consumption between two observations is the
/// positive increment of `used`, attributed to the window containing the later
/// observation (its baseline is the immediately previous sample, even when
/// that sample predates `from`). Currency metrics contribute cost through
/// `used` increments; balance-style metrics through remaining decreases. The
/// same reset/top-up rules as [`aggregate_daily_usage`] apply.
pub fn range_usage(samples: &[DailySample], from: u64, to: u64) -> RangeUsage {
    let mut series: BTreeMap<(&str, &str), Vec<&DailySample>> = BTreeMap::new();
    for sample in samples {
        series
            .entry((sample.account_id.as_str(), sample.metric_key.as_str()))
            .or_default()
            .push(sample);
    }

    let mut tokens = 0.0;
    let mut costs: BTreeMap<String, f64> = BTreeMap::new();
    let mut has_data = false;
    for metric_samples in series.values_mut() {
        metric_samples.sort_by_key(|sample| sample.observed_at);
        let mut previous: Option<&DailySample> = None;
        for sample in metric_samples {
            let in_window = sample.observed_at >= from && sample.observed_at < to;
            has_data |= in_window;
            if let (true, Some(previous)) = (in_window, previous) {
                let positive = |delta: f64| delta.is_finite() && delta > 0.0;
                if let (Some(prev_used), Some(used)) = (previous.used, sample.used) {
                    let delta = used - prev_used;
                    if positive(delta) {
                        match sample.unit.as_str() {
                            "token" => tokens += delta,
                            "currency" => {
                                let key = sample.currency.clone().unwrap_or_default();
                                *costs.entry(key).or_insert(0.0) += delta;
                            }
                            _ => {}
                        }
                    }
                } else if sample.unit == "currency" {
                    if let (Some(prev_remaining), Some(remaining)) =
                        (previous.remaining, sample.remaining)
                    {
                        let delta = prev_remaining - remaining;
                        if positive(delta) {
                            let key = sample.currency.clone().unwrap_or_default();
                            *costs.entry(key).or_insert(0.0) += delta;
                        }
                    }
                }
            }
            previous = Some(sample);
        }
    }

    let mut dominant_key = String::new();
    let mut dominant_total = f64::MIN;
    for (key, total) in &costs {
        if *total > dominant_total {
            dominant_total = *total;
            dominant_key.clone_from(key);
        }
    }
    let cost = costs.get(&dominant_key).copied();
    RangeUsage {
        tokens,
        cost: cost.filter(|total| *total > 0.0),
        currency: if dominant_key.is_empty() {
            None
        } else {
            Some(dominant_key)
        },
        has_data,
    }
}

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
        let days = (last_ts.saturating_sub(first_ts)) as f64 / 86400.0;
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

    // ---- daily aggregation ----

    fn sample(
        account: &str,
        key: &str,
        observed_at: u64,
        used: Option<f64>,
        remaining: Option<f64>,
        unit: &str,
        currency: Option<&str>,
    ) -> DailySample {
        DailySample {
            account_id: account.to_owned(),
            metric_key: key.to_owned(),
            observed_at,
            used,
            remaining,
            unit: unit.to_owned(),
            currency: currency.map(ToOwned::to_owned),
        }
    }

    fn aggregate(samples: &[DailySample]) -> Vec<DailyUsage> {
        aggregate_daily_usage(samples, |_| 8 * 3_600)
    }

    /// With a +8h offset, local day 1 spans UTC `57_600..144_000` and local
    /// day 2 starts at UTC `144_000`.
    #[test]
    fn token_deltas_bucket_by_local_day() {
        let usage = aggregate(&[
            sample("a", "m", 82_800, Some(100.0), None, "token", None),
            sample("a", "m", 90_000, Some(350.0), None, "token", None),
            sample("a", "m", 150_000, Some(350.0), None, "token", None),
            sample("a", "m", 160_000, Some(500.0), None, "token", None),
        ]);
        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].day_start, 57_600);
        assert!((usage[0].tokens_used - 250.0).abs() < 1e-9);
        assert!(usage[0].has_data);
        assert_eq!(usage[1].day_start, 144_000);
        assert!((usage[1].tokens_used - 150.0).abs() < 1e-9);
    }

    #[test]
    fn window_reset_never_counts_as_usage() {
        let usage = aggregate(&[
            sample("a", "m", 1_000, Some(500.0), None, "token", None),
            // Provider reset: used dropped back to a small value.
            sample("a", "m", 2_000, Some(50.0), None, "token", None),
            sample("a", "m", 3_000, Some(120.0), None, "token", None),
        ]);
        assert_eq!(usage.len(), 1);
        assert!((usage[0].tokens_used - 70.0).abs() < 1e-9);
    }

    #[test]
    fn balance_decrease_is_cost_and_top_up_is_not() {
        let usage = aggregate(&[
            sample(
                "a",
                "balance",
                1_000,
                None,
                Some(100.0),
                "currency",
                Some("¥"),
            ),
            sample(
                "a",
                "balance",
                2_000,
                None,
                Some(85.0),
                "currency",
                Some("¥"),
            ),
            // A top-up raises the remaining balance; it is not a cost.
            sample(
                "a",
                "balance",
                3_000,
                None,
                Some(95.0),
                "currency",
                Some("¥"),
            ),
        ]);
        assert_eq!(usage.len(), 1);
        assert!(usage[0].tokens_used.abs() < f64::EPSILON);
        assert_eq!(usage[0].currency.as_deref(), Some("¥"));
        assert!((usage[0].cost.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn currency_used_increments_are_cost_with_dominant_currency() {
        let usage = aggregate(&[
            sample("a", "usd", 1_000, Some(1.0), None, "currency", Some("$")),
            sample("a", "usd", 2_000, Some(3.0), None, "currency", Some("$")),
            sample("a", "cny", 1_000, Some(10.0), None, "currency", Some("¥")),
            sample("a", "cny", 2_000, Some(50.0), None, "currency", Some("¥")),
        ]);
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].currency.as_deref(), Some("¥"));
        assert!((usage[0].cost.unwrap() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn non_relevant_units_mark_the_day_without_contributing() {
        let usage = aggregate(&[
            sample("a", "req", 1_000, Some(10.0), None, "request", None),
            sample("a", "req", 2_000, Some(40.0), None, "request", None),
        ]);
        assert_eq!(usage.len(), 1);
        assert!(usage[0].has_data, "an observation still proves the day");
        assert!(usage[0].tokens_used.abs() < f64::EPSILON);
        assert_eq!(usage[0].cost, None);
    }

    #[test]
    fn single_observation_day_has_data_without_deltas() {
        let usage = aggregate(&[sample("a", "m", 82_800, Some(100.0), None, "token", None)]);
        assert_eq!(usage.len(), 1);
        assert!(usage[0].has_data);
        assert!(usage[0].tokens_used.abs() < f64::EPSILON);
    }

    #[test]
    fn empty_input_yields_no_days() {
        assert!(aggregate(&[]).is_empty());
    }

    #[test]
    fn accounts_aggregate_independently_then_sum() {
        let usage = aggregate(&[
            sample("a", "m", 82_800, Some(100.0), None, "token", None),
            sample("b", "m", 82_800, Some(200.0), None, "token", None),
            sample("a", "m", 90_000, Some(150.0), None, "token", None),
            sample("b", "m", 90_000, Some(300.0), None, "token", None),
        ]);
        assert_eq!(usage.len(), 1);
        assert!((usage[0].tokens_used - 150.0).abs() < 1e-9);
    }

    #[test]
    fn range_usage_counts_deltas_of_later_samples_inside_window() {
        let samples = [
            sample("a", "m", 1_000, Some(100.0), None, "token", None),
            sample("a", "m", 2_000, Some(400.0), None, "token", None),
            sample("a", "m", 3_000, Some(450.0), None, "token", None),
            sample("a", "m", 9_000, Some(900.0), None, "token", None),
        ];
        // Window [1500, 3500): baseline is the pre-window sample, the later
        // samples inside count their increments.
        let usage = range_usage(&samples, 1_500, 3_500);
        assert!((usage.tokens - 350.0).abs() < 1e-9);
        assert!(usage.has_data);
        // Everything outside yields no data, not zero.
        let outside = range_usage(&samples, 4_000, 5_000);
        assert!(!outside.has_data);
        assert!(outside.tokens.abs() < f64::EPSILON);
    }

    #[test]
    fn range_usage_keeps_reset_and_balance_rules() {
        let usage = range_usage(
            &[
                sample("a", "m", 1_000, Some(500.0), None, "token", None),
                // Reset between the samples: the drop is not usage.
                sample("a", "m", 2_000, Some(50.0), None, "token", None),
                sample("a", "m", 2_500, Some(120.0), None, "token", None),
                sample(
                    "a",
                    "balance",
                    1_000,
                    None,
                    Some(100.0),
                    "currency",
                    Some("¥"),
                ),
                sample(
                    "a",
                    "balance",
                    2_000,
                    None,
                    Some(85.0),
                    "currency",
                    Some("¥"),
                ),
            ],
            0,
            3_000,
        );
        assert!((usage.tokens - 70.0).abs() < 1e-9);
        assert_eq!(usage.currency.as_deref(), Some("¥"));
        assert!((usage.cost.unwrap() - 15.0).abs() < 1e-9);
    }
}
