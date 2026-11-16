//! History command surface, split out of the former single-file module.

use std::fmt::Write as _;

use super::{
    AppRuntime, Arc, CommandError, HashMap, Serialize, State, aggregate_daily_usage, range_usage,
    unix_seconds,
};

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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryExportResult {
    /// Absolute path of the written file, shown to the user so they can
    /// collect it. Only the export directory under the app data dir is ever
    /// written.
    pub path: String,
    pub rows: usize,
}

/// Escapes one CSV field per RFC 4180: quotes double up, fields carrying
/// separators, quotes, or newlines are wrapped in quotes. A leading `=`,
/// `+`, `-`, or `@` is additionally prefixed with `'` so a spreadsheet
/// application cannot execute the cell as a formula when the export is
/// opened — the exported text fields are user or provider authored, and the
/// guard costs nothing for ordinary values.
pub(super) fn csv_field(value: &str) -> String {
    let needs_quoting =
        value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r');
    let neutralized = match value.as_bytes().first() {
        Some(b'=' | b'+' | b'-' | b'@') => format!("'{value}"),
        _ => value.to_owned(),
    };
    if needs_quoting {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
    }
}

/// One exported row: local-midnight day bucket plus the derived figures.
pub(super) type ExportRow = (u64, String, f64, Option<f64>, Option<String>);

/// Builds the export rows from the recorded history: one row per observed
/// day, dropping days without observations. The account column stays in the
/// schema (as "all-accounts") so per-account exports keep a stable shape.
pub(super) fn collect_export_rows(
    repository: &crate::storage::LocalRepository,
    from: u64,
    to: u64,
    account_id: Option<&str>,
) -> Result<Vec<ExportRow>, CommandError> {
    let mut samples = repository.history_samples(from, to)?;
    if let Some(id) = account_id {
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
    let mut rows: Vec<ExportRow> = Vec::new();
    for day in &daily {
        if !day.has_data || (day.tokens_used <= 0.0 && day.cost.is_none()) {
            continue;
        }
        rows.push((
            day.day_start,
            "all-accounts".to_owned(),
            day.tokens_used,
            day.cost,
            day.currency.clone(),
        ));
    }
    Ok(rows)
}

/// Renders the rows as CSV (RFC 4180 escaping) or pretty JSON. `local_offset`
/// is the same offset the daily aggregation bucketed with: each row's
/// `day_start` is a local-midnight instant, so rendering it in UTC would
/// label every exported day one calendar day early for timezones ahead of
/// UTC. The offset is sampled at the day's own instant.
pub(super) fn render_export(
    format: &str,
    rows: &[ExportRow],
    local_offset: impl Fn(i64) -> i32,
) -> String {
    let day_text = |day: u64| {
        let seconds = i64::try_from(day).unwrap_or(0);
        let offset = i64::from(local_offset(seconds));
        chrono::DateTime::from_timestamp(seconds.saturating_add(offset), 0)
            .map(|instant| instant.format("%Y-%m-%d").to_string())
    };
    if format == "json" {
        return serde_json::to_string_pretty(
            &rows
                .iter()
                .map(|(day, account, tokens, cost, currency)| {
                    serde_json::json!({
                        "day": day_text(*day),
                        "account": account,
                        "tokens": tokens,
                        "cost": cost,
                        "currency": currency,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_owned());
    }
    let mut out = String::from(
        "day,account,tokens,cost,currency
",
    );
    for (day, account, tokens, cost, currency) in rows {
        let day_text = day_text(*day).unwrap_or_default();
        let cost_text = cost.map_or_else(String::new, |cost| format!("{cost}"));
        let _ = writeln!(
            out,
            "{},{},{},{},{}",
            day_text,
            csv_field(account),
            tokens,
            cost_text,
            csv_field(currency.as_deref().unwrap_or_default()),
        );
    }
    out
}

/// Exports the aggregated daily usage (same aggregation as the trend view)
/// as CSV or JSON into `<app data>/exports/`. The export carries account
/// labels and derived usage only — never credentials, credential refs, or
/// keyring material. Days outside the retention window are absent by
/// construction; the file is a snapshot of what the local database holds.
#[tauri::command]
pub fn history_export(
    state: State<'_, Arc<AppRuntime>>,
    format: String,
    days: Option<u32>,
    account_id: Option<String>,
) -> Result<HistoryExportResult, CommandError> {
    let days = days.unwrap_or(182).clamp(7, 365);
    let format: &str = match format.as_str() {
        "csv" | "json" => format.as_str(),
        other => {
            return Err(CommandError::InvalidInput(format!(
                "unsupported export format `{other}` (csv or json)"
            )));
        }
    };
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(days) * 86_400);
    let repository = state.repository()?;
    let rows = collect_export_rows(&repository, from, now, account_id.as_deref())?;

    let written_rows = rows.len();
    let body = render_export(format, &rows, local_offset_at);
    let path = super::exports::write_export(&state.data_dir, "history", format, &body)?;
    Ok(HistoryExportResult {
        path: path.to_string_lossy().into_owned(),
        rows: written_rows,
    })
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
    let labels: HashMap<String, String> = {
        let repository = state.repository()?;
        repository
            .accounts()?
            .into_iter()
            .map(|account| (account.id, account.label))
            .collect()
    };
    let range_days = days.unwrap_or(30).clamp(7, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(range_days) * 86_400);
    let repository = state.repository()?;
    let samples = repository.history_samples(from, now)?;

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

#[cfg(test)]
mod tests {
    use super::super::exports::{MAX_EXPORT_FILES, prune_exports};
    use super::*;

    #[test]
    fn old_export_files_are_pruned_after_a_new_one_lands() {
        let dir = std::env::temp_dir().join(format!(
            "tokenowl-exports-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        std::fs::create_dir_all(&dir).expect("export dir");
        for index in 0..(MAX_EXPORT_FILES + 5) {
            std::fs::write(
                dir.join(format!("tokenowl-history-2026010{index:02}-000000.csv")),
                "x",
            )
            .expect("seed file");
        }
        std::fs::write(dir.join("unrelated.txt"), "keep").expect("unrelated file");
        prune_exports(&dir);
        let mut exports: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("tokenowl-history-"))
            .collect();
        exports.sort();
        assert_eq!(exports.len(), MAX_EXPORT_FILES, "the cap holds");
        assert!(
            exports[0].contains("202601005"),
            "the oldest files went first"
        );
        assert!(
            dir.join("unrelated.txt").exists(),
            "non-export files are never touched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn csv_fields_escape_letting_separators_survive() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(
            csv_field(
                "line
break"
            ),
            "\"line
break\""
        );
    }

    /// A text cell that could be executed as a spreadsheet formula is
    /// neutralized with a `'` prefix; ordinary values are untouched.
    #[test]
    fn csv_fields_neutralize_formula_prefixes() {
        assert_eq!(csv_field("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(csv_field("+1"), "'+1");
        assert_eq!(csv_field("-cmd"), "'-cmd");
        assert_eq!(csv_field("@x"), "'@x");
        assert_eq!(csv_field("-5.0"), "'-5.0");
        // Quoting and neutralization compose.
        assert_eq!(csv_field("=a,\"b\""), "\"'=a,\"\"b\"\"\"");
        // Ordinary text and numbers keep their spelling.
        assert_eq!(csv_field("USD"), "USD");
        assert_eq!(csv_field("all-accounts"), "all-accounts");
        assert_eq!(csv_field("2026-03-02"), "2026-03-02");
    }

    /// The day column is the row's local calendar date. `day_start` is a
    /// local-midnight instant; formatting that instant in UTC shifts every
    /// positive-offset timezone (UTC+8 here) one calendar day early.
    #[test]
    fn exported_days_carry_the_local_calendar_date() {
        // 2026-03-02 00:00 +08:00 == 2026-03-01 16:00 UTC.
        let day_start: u64 = 1_772_380_800;
        let rows: Vec<ExportRow> = vec![(day_start, "all-accounts".to_owned(), 1200.0, None, None)];
        let offset = |_: i64| 8 * 3600;

        let csv = render_export("csv", &rows, offset);
        assert!(
            csv.contains("2026-03-02"),
            "csv day is the local date: {csv}"
        );
        assert!(
            !csv.contains("2026-03-01"),
            "the UTC instant must not leak: {csv}"
        );

        let json = render_export("json", &rows, offset);
        assert!(
            json.contains("2026-03-02"),
            "json day is the local date: {json}"
        );
        assert!(
            !json.contains("2026-03-01"),
            "the UTC instant must not leak: {json}"
        );
    }

    /// A negative-offset timezone is the mirror case: the local date is one
    /// calendar day *later* than the UTC rendering of the midnight instant.
    #[test]
    fn exported_days_follow_the_offset_in_both_directions() {
        // 2026-03-01 00:00 -05:00 == 2026-03-01 05:00 UTC.
        let day_start: u64 = 1_772_341_200;
        let rows: Vec<ExportRow> = vec![(day_start, "all-accounts".to_owned(), 1200.0, None, None)];
        let offset = |_: i64| -5 * 3600;

        let csv = render_export("csv", &rows, offset);
        assert!(csv.contains("2026-03-01"), "the local date holds: {csv}");
        assert!(!csv.contains("2026-03-02"), "no UTC drift forward: {csv}");
    }
}
