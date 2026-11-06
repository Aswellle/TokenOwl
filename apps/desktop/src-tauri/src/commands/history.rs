//! History command surface, split out of the former single-file module.

use super::{Serialize, State, Arc, AppRuntime, CommandError, unix_seconds, aggregate_daily_usage, HashMap, range_usage};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayViewModel {
    /// UTC timestamp of local midnight, so the webview renders the user's own
    /// calendar day regardless of its own timezone handling.
    pub day_start: u64,
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    /// False when nothing was observed that day: unknown, not zero.
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummaryViewModel {
    pub days: Vec<HistoryDayViewModel>,
    pub total_tokens: f64,
    pub total_cost: Option<f64>,
    pub currency: Option<String>,
    pub active_days: u32,
    pub range_days: u32,
}

/// UTC offset in seconds at `timestamp`, for local-calendar day bucketing.
pub(super) fn local_offset_at(timestamp: i64) -> i32 {
    use chrono::{Offset as _, TimeZone as _};
    chrono::Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map_or(0, |dt| dt.offset().fix().local_minus_utc())
}

#[tauri::command]
pub fn history_summary(
    state: State<'_, Arc<AppRuntime>>,
    days: Option<u32>,
    account_id: Option<String>,
) -> Result<HistorySummaryViewModel, CommandError> {
    let range_days = days.unwrap_or(182).clamp(7, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(range_days) * 86_400);
    let repository = state.repository()?;
    let mut samples = repository.history_samples(from, now)?;
    if let Some(id) = account_id.as_deref() {
        samples.retain(|sample| sample.account_id == id);
    }
    let daily = aggregate_daily_usage(
        &samples
            .into_iter()
            .map(|sample| crate::domain::history::DailySample {
                account_id: sample.account_id,
                metric_key: sample.metric_key,
                observed_at: sample.observed_at,
                used: sample.used,
                remaining: sample.remaining,
                unit: sample.unit,
                currency: sample.currency,
            })
            .collect::<Vec<_>>(),
        local_offset_at,
    );

    let mut total_tokens = 0.0;
    let mut totals_by_currency: HashMap<String, f64> = HashMap::new();
    let mut active_days = 0_u32;
    let view_days = daily
        .into_iter()
        .map(|day| {
            total_tokens += day.tokens_used;
            if day.tokens_used > 0.0 || day.cost.is_some() {
                active_days += 1;
            }
            if let (Some(cost), currency) = (day.cost, day.currency.clone()) {
                *totals_by_currency
                    .entry(currency.unwrap_or_default())
                    .or_insert(0.0) += cost;
            }
            HistoryDayViewModel {
                day_start: day.day_start,
                tokens: day.tokens_used,
                cost: day.cost,
                currency: day.currency,
                has_data: day.has_data,
            }
        })
        .collect::<Vec<_>>();
    let (dominant_currency, total_cost) = totals_by_currency
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or((None, None), |(key, total)| {
            (
                if key.is_empty() {
                    None
                } else {
                    Some(key.clone())
                },
                Some(*total),
            )
        });

    Ok(HistorySummaryViewModel {
        days: view_days,
        total_tokens,
        total_cost: total_cost.filter(|total| *total > 0.0),
        currency: dominant_currency,
        active_days,
        range_days,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeUsageViewModel {
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindowsViewModel {
    /// Usage observed in the trailing 2 hours.
    pub hours2: RangeUsageViewModel,
    /// Usage observed in the trailing 24 hours.
    pub hours24: RangeUsageViewModel,
    /// Usage observed in the trailing 30 days.
    pub days30: RangeUsageViewModel,
    /// Token consumption velocity over the trailing 24 hours and how long
    /// the remaining quota lasts at that pace. `None` when the window holds
    /// no trusted rising token series — unknown, not zero.
    pub burn_rate: Option<crate::domain::history::BurnRate>,
}

/// Trailing-window usage for one account (or every account when `account_id`
/// is omitted), derived from recorded history. Windows without observations
/// report `has_data: false` — unknown, not zero.
#[tauri::command]
pub fn usage_windows(
    state: State<'_, Arc<AppRuntime>>,
    account_id: Option<String>,
) -> Result<UsageWindowsViewModel, CommandError> {
    let now = unix_seconds();
    let from = now.saturating_sub(30 * 86_400);
    let repository = state.repository()?;
    let mut samples = repository.history_samples(from, now)?;
    if let Some(id) = account_id.as_deref() {
        samples.retain(|sample| sample.account_id == id);
    }
    let view = |range: u64| {
        let usage = range_usage(
            &samples
                .iter()
                .map(|sample| crate::domain::history::DailySample {
                    account_id: sample.account_id.clone(),
                    metric_key: sample.metric_key.clone(),
                    observed_at: sample.observed_at,
                    used: sample.used,
                    remaining: sample.remaining,
                    unit: sample.unit.clone(),
                    currency: sample.currency.clone(),
                })
                .collect::<Vec<_>>(),
            now.saturating_sub(range),
            now,
        );
        RangeUsageViewModel {
            tokens: usage.tokens,
            cost: usage.cost,
            currency: usage.currency,
            has_data: usage.has_data,
        }
    };
    let daily: Vec<crate::domain::history::DailySample> = samples
        .iter()
        .map(|sample| crate::domain::history::DailySample {
            account_id: sample.account_id.clone(),
            metric_key: sample.metric_key.clone(),
            observed_at: sample.observed_at,
            used: sample.used,
            remaining: sample.remaining,
            unit: sample.unit.clone(),
            currency: sample.currency.clone(),
        })
        .collect();
    let burn_rate =
        crate::domain::history::burn_rate_hourly(&daily, now.saturating_sub(24 * 3_600), now);
    Ok(UsageWindowsViewModel {
        hours2: view(2 * 3_600),
        hours24: view(24 * 3_600),
        days30: view(30 * 86_400),
        burn_rate,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayAccountViewModel {
    pub account_id: String,
    pub label: String,
    pub tokens: f64,
    pub cost: Option<f64>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDayBreakdownViewModel {
    /// UTC timestamp of local midnight for the day bucket.
    pub day_start: u64,
    pub accounts: Vec<HistoryDayAccountViewModel>,
}

/// Per-provider token/cost breakdown for each recorded day — the data behind
/// the trend chart's hover details. Quota overviews are joined client-side
/// from the accounts list, which already carries the latest snapshots.
#[tauri::command]
pub fn history_breakdown(
    state: State<'_, Arc<AppRuntime>>,
    days: Option<u32>,
) -> Result<Vec<HistoryDayBreakdownViewModel>, CommandError> {
    let range_days = days.unwrap_or(30).clamp(7, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(range_days) * 86_400);
    let repository = state.repository()?;
    let samples = repository.history_samples(from, now)?;
    let labels: HashMap<String, String> = repository
        .accounts()?
        .into_iter()
        .map(|account| (account.id, account.label))
        .collect();

    let mut per_account: HashMap<String, Vec<crate::domain::history::DailySample>> = HashMap::new();
    for sample in samples {
        per_account
            .entry(sample.account_id.clone())
            .or_default()
            .push(crate::domain::history::DailySample {
                account_id: sample.account_id,
                metric_key: sample.metric_key,
                observed_at: sample.observed_at,
                used: sample.used,
                remaining: sample.remaining,
                unit: sample.unit,
                currency: sample.currency,
            });
    }

    let mut days_map: std::collections::BTreeMap<u64, Vec<HistoryDayAccountViewModel>> =
        std::collections::BTreeMap::new();
    for (account_id, account_samples) in per_account {
        let daily = aggregate_daily_usage(&account_samples, local_offset_at);
        for day in daily {
            if !day.has_data || (day.tokens_used <= 0.0 && day.cost.is_none()) {
                continue;
            }
            days_map
                .entry(day.day_start)
                .or_default()
                .push(HistoryDayAccountViewModel {
                    account_id: account_id.clone(),
                    label: labels
                        .get(&account_id)
                        .cloned()
                        .unwrap_or_else(|| account_id.clone()),
                    tokens: day.tokens_used,
                    cost: day.cost,
                    currency: day.currency,
                });
        }
    }
    Ok(days_map
        .into_iter()
        .rev()
        .map(|(day_start, accounts)| HistoryDayBreakdownViewModel {
            day_start,
            accounts,
        })
        .collect())
}
