//! Local coding-agent usage commands. A separate data domain from the
//! remote quota pipeline: opt-in, read-only scans of known directories,
//! idempotent persistence, no upload path.

use super::{AppRuntime, Arc, CommandError, State, unix_seconds};
use crate::usage::{ScanCursor, ScanOutcome, claude_code, codex};
use serde::Serialize;
use std::collections::BTreeMap;

/// Settings key carrying the per-source opt-in map. Absent or malformed
/// means everything is off — the safe default for a privacy-sensitive
/// source.
pub(super) const LOCAL_SOURCES_KEY: &str = "local_usage_sources";

/// The known sources with their human-facing descriptions. The directories
/// column is what the user is shown before opting in — the whole privacy
/// contract in one row.
pub fn known_sources() -> Vec<KnownSource> {
    vec![
        KnownSource {
            id: claude_code::SOURCE_ID.to_owned(),
            name: "Claude Code".to_owned(),
            directories: vec!["~/.claude/projects".to_owned()],
            file_types: vec!["*.jsonl".to_owned()],
            collected: vec![
                "时间戳".to_owned(),
                "会话 ID".to_owned(),
                "模型名".to_owned(),
                "项目标识（工具自身的编码目录名）".to_owned(),
                "四类 Token 计数".to_owned(),
            ],
            never_collected: vec![
                "提示词正文".to_owned(),
                "回复正文".to_owned(),
                "源码片段".to_owned(),
            ],
            verified: true,
        },
        KnownSource {
            id: codex::SOURCE_ID.to_owned(),
            name: "Codex CLI".to_owned(),
            directories: vec!["~/.codex/sessions".to_owned()],
            file_types: vec!["*.jsonl".to_owned()],
            collected: vec![
                "时间戳".to_owned(),
                "会话 ID".to_owned(),
                "模型名".to_owned(),
                "三类 Token 计数".to_owned(),
            ],
            never_collected: vec![
                "提示词正文".to_owned(),
                "回复正文".to_owned(),
                "工作目录路径".to_owned(),
            ],
            // The rollout format is documented but not yet verified against
            // a real installation on the development machine.
            verified: false,
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownSource {
    pub id: String,
    pub name: String,
    pub directories: Vec<String>,
    pub file_types: Vec<String>,
    pub collected: Vec<String>,
    pub never_collected: Vec<String>,
    /// Whether the parser has been verified against real on-disk data.
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSourcesView {
    pub sources: Vec<KnownSource>,
    /// Per-source enabled state; an unknown id reads as disabled.
    pub enabled: Vec<String>,
}

fn enabled_sources(state: &AppRuntime) -> Result<Vec<String>, CommandError> {
    let repository = state.repository()?;
    Ok(match repository.get_setting(LOCAL_SOURCES_KEY)? {
        Some(serde_json::Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    })
}

/// Settings key holding per-source scan cursors. A cursor contains a hashed
/// file identity, an offset, and a size+mtime signature — never a path.
pub(super) const LOCAL_CURSORS_KEY: &str = "local_usage_cursors";

/// Reads the persisted cursor set; malformed storage means "scan from the
/// start", which is safe because event insertion is idempotent.
pub(super) fn load_cursors(
    repository: &crate::storage::LocalRepository,
) -> Result<BTreeMap<String, Vec<ScanCursor>>, CommandError> {
    match repository.get_setting(LOCAL_CURSORS_KEY)? {
        Some(value) => Ok(serde_json::from_value(value).unwrap_or_default()),
        None => Ok(BTreeMap::new()),
    }
}

/// Stores one source's cursors, replacing the previous set. The settings
/// table is JSON, so this needs no migration.
pub(super) fn save_cursors(
    repository: &crate::storage::LocalRepository,
    source_id: &str,
    cursors: &[ScanCursor],
) -> Result<(), CommandError> {
    let mut all = load_cursors(repository)?;
    all.insert(source_id.to_owned(), cursors.to_vec());
    let value = serde_json::to_value(&all)
        .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
    repository.set_setting(LOCAL_CURSORS_KEY, &value)?;
    Ok(())
}

/// Drops cursors for one source, or all cursors when `source_id` is `None`.
/// Called whenever stored events are deleted so a later scan re-reads the
/// files that produced them.
pub(super) fn clear_cursors(
    repository: &crate::storage::LocalRepository,
    source_id: Option<&str>,
) -> Result<(), CommandError> {
    match source_id {
        Some(id) => {
            let mut all = load_cursors(repository)?;
            if all.remove(id).is_some() {
                let value = serde_json::to_value(&all)
                    .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
                repository.set_setting(LOCAL_CURSORS_KEY, &value)?;
            }
        }
        None => {
            repository.set_setting(LOCAL_CURSORS_KEY, &serde_json::Value::Array(Vec::new()))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn local_usage_sources_get(
    state: State<'_, Arc<AppRuntime>>,
) -> Result<LocalSourcesView, CommandError> {
    Ok(LocalSourcesView {
        sources: known_sources(),
        enabled: enabled_sources(&state)?,
    })
}

/// Enables or disables one source. Disabling also deletes that source's
/// stored events — "off" means the data is gone, not hidden.
#[tauri::command]
pub fn local_usage_source_set(
    source_id: String,
    enabled: bool,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    if !known_sources().iter().any(|source| source.id == source_id) {
        return Err(CommandError::InvalidInput(format!(
            "unknown local usage source `{source_id}`"
        )));
    }
    let repository = state.repository()?;
    let mut enabled_ids: Vec<String> = enabled_sources(&state)?
        .into_iter()
        .filter(|id| *id != source_id)
        .collect();
    if enabled {
        enabled_ids.push(source_id.clone());
    } else {
        repository.clear_local_usage_events(Some(&source_id))?;
        // Re-enabling must re-read the files: the deleted events cannot be
        // reconstructed from a cursor that already points past them.
        clear_cursors(&repository, Some(&source_id))?;
    }
    let value = serde_json::to_value(&enabled_ids)
        .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
    repository.set_setting(LOCAL_SOURCES_KEY, &value)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalScanView {
    pub source_id: String,
    pub scanned_files: usize,
    pub new_events: usize,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// Scans every enabled source (or the one named) and stores only new
/// events. Disabled sources are never touched, whatever the caller asks.
#[tauri::command]
pub fn local_usage_scan(
    state: State<'_, Arc<AppRuntime>>,
    source_id: Option<String>,
) -> Result<Vec<LocalScanView>, CommandError> {
    let enabled: Vec<String> = match source_id {
        Some(id) => {
            let enabled = enabled_sources(&state)?;
            if !enabled.contains(&id) {
                return Err(CommandError::InvalidInput(format!(
                    "local usage source `{id}` is not enabled"
                )));
            }
            vec![id]
        }
        None => enabled_sources(&state)?,
    };
    let repository = state.repository()?;
    let cursors = load_cursors(&repository)?;
    let mut views = Vec::with_capacity(enabled.len());
    for id in enabled {
        let outcome = scan_source(&id, cursors.get(&id).map_or(&[][..], Vec::as_slice));
        let new_events =
            repository.insert_local_usage_events(&id, &outcome.events, unix_seconds())?;
        // Persist the cursor only after the events it produced are durable;
        // a failed insert leaves the old cursor so the next scan retries the
        // same bytes instead of skipping them forever.
        save_cursors(&repository, &id, &outcome.cursors)?;
        views.push(LocalScanView {
            source_id: id,
            scanned_files: outcome.scanned_files,
            new_events,
            skipped: outcome.skipped,
            errors: outcome.errors,
        });
    }
    Ok(views)
}

/// Dispatch to the one parser a source id names. Unknown ids cannot happen
/// through the command path (validated above); this is still exhaustive
/// rather than stringly-typed.
fn scan_source(source_id: &str, cursors: &[ScanCursor]) -> ScanOutcome {
    let home = dirs_home();
    match source_id {
        claude_code::SOURCE_ID => claude_code::scan(&home, cursors),
        codex::SOURCE_ID => codex::scan(&home, cursors),
        other => ScanOutcome {
            source_id: other.to_owned(),
            scanned_files: 0,
            events: Vec::new(),
            cursors: Vec::new(),
            skipped: Vec::new(),
            errors: vec!["unknown source".to_owned()],
        },
    }
}

/// The home directory the known directories expand against.
fn dirs_home() -> std::path::PathBuf {
    match std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        Some(home) => std::path::PathBuf::from(home),
        None => std::path::PathBuf::from("."),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUsageDayView {
    pub day_start_unix: i64,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub event_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUsageSummaryView {
    pub days: Vec<LocalUsageDayView>,
    pub totals: LocalUsageDayView,
    pub source_counts: Vec<(String, i64)>,
}

/// Aggregated local usage for the trailing `days` (1..=365). Cost is
/// deliberately absent: no price table ships with the app, so any number
/// here would be invented.
#[tauri::command]
pub fn local_usage_summary(
    state: State<'_, Arc<AppRuntime>>,
    days: Option<u32>,
) -> Result<LocalUsageSummaryView, CommandError> {
    let days = days.unwrap_or(30).clamp(1, 365);
    let now = unix_seconds();
    let from = now.saturating_sub(u64::from(days) * 86_400);
    let repository = state.repository()?;
    let rows = repository.local_usage_daily(from, now)?;
    let mut totals = LocalUsageDayView {
        day_start_unix: 0,
        model: "total".to_owned(),
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        event_count: 0,
    };
    let view_rows = rows
        .into_iter()
        .map(|row| {
            totals.input_tokens += row.input_tokens;
            totals.output_tokens += row.output_tokens;
            totals.cache_read_tokens += row.cache_read_tokens;
            totals.cache_write_tokens += row.cache_write_tokens;
            totals.event_count += row.event_count;
            LocalUsageDayView {
                day_start_unix: row.day_start_unix,
                model: row.model,
                input_tokens: row.input_tokens,
                output_tokens: row.output_tokens,
                cache_read_tokens: row.cache_read_tokens,
                cache_write_tokens: row.cache_write_tokens,
                event_count: row.event_count,
            }
        })
        .collect();
    Ok(LocalUsageSummaryView {
        days: view_rows,
        totals,
        source_counts: repository.local_usage_counts()?,
    })
}

/// Explicit data removal: one source or everything. Local-only by design;
/// there is nothing to revoke remotely.
#[tauri::command]
pub fn local_usage_clear(
    state: State<'_, Arc<AppRuntime>>,
    source_id: Option<String>,
) -> Result<usize, CommandError> {
    let repository = state.repository()?;
    let removed = repository.clear_local_usage_events(source_id.as_deref())?;
    // A cleared source must be scannable from scratch again; otherwise its
    // files stay "already read" and the summary would remain empty until the
    // files change.
    clear_cursors(&repository, source_id.as_deref())?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::LocalUsageEvent;

    #[test]
    fn disabling_a_source_clears_only_its_events() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tokenowl-usage-cmd-{}-{}.sqlite3",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        let runtime = AppRuntime::new(&path).expect("runtime");
        let repository = runtime.repository().expect("database");
        let event = |uuid: &str| LocalUsageEvent {
            source_event_id: uuid.to_owned(),
            occurred_at: "2026-11-10T09:00:00Z".to_owned(),
            session_id: Some("s".to_owned()),
            model: Some("m".to_owned()),
            project_ref: None,
            input_tokens: Some(10),
            output_tokens: Some(5),
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_estimate: None,
        };
        repository
            .insert_local_usage_events("claude-code", &[event("a"), event("b")], 1)
            .expect("insert");
        repository
            .insert_local_usage_events("codex", &[event("c")], 1)
            .expect("insert");
        assert_eq!(repository.local_usage_counts().expect("counts").len(), 2);
        assert_eq!(
            repository
                .clear_local_usage_events(Some("claude-code"))
                .expect("clear"),
            2,
            "only the named source's events go"
        );
        assert_eq!(
            repository.local_usage_counts().expect("counts"),
            vec![("codex".to_owned(), 1)]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn inserts_are_idempotent_per_source_event_id() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tokenowl-usage-idem-{}-{}.sqlite3",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        let runtime = AppRuntime::new(&path).expect("runtime");
        let repository = runtime.repository().expect("database");
        let event = LocalUsageEvent {
            source_event_id: "same-uuid".to_owned(),
            occurred_at: "2026-11-10T09:00:00Z".to_owned(),
            session_id: None,
            model: Some("m".to_owned()),
            project_ref: None,
            input_tokens: Some(10),
            output_tokens: Some(5),
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_estimate: None,
        };
        let first = repository
            .insert_local_usage_events("claude-code", std::slice::from_ref(&event), 1)
            .expect("insert");
        let second = repository
            .insert_local_usage_events("claude-code", &[event], 2)
            .expect("re-insert");
        assert_eq!((first, second), (1, 0), "the replay inserts nothing");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cursors_round_trip_and_can_be_cleared_per_source() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tokenowl-usage-cursors-{}-{}.sqlite3",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        let runtime = AppRuntime::new(&path).expect("runtime");
        let repository = runtime.repository().expect("database");
        let cursor = ScanCursor {
            file_id: crate::usage::file_identity(std::path::Path::new(
                "C:/private/place/session.jsonl",
            )),
            offset: 128,
            signature: "head".to_owned(),
            anchor: "tail".to_owned(),
        };
        save_cursors(&repository, "codex", std::slice::from_ref(&cursor)).expect("save");
        save_cursors(&repository, "claude-code", &[]).expect("save");
        let loaded = load_cursors(&repository).expect("load");
        assert_eq!(loaded.get("codex"), Some(&vec![cursor]));
        assert!(loaded.get("claude-code").is_some_and(Vec::is_empty));

        // The persisted identity is a hash, not the filesystem path.
        let raw = repository
            .get_setting(LOCAL_CURSORS_KEY)
            .expect("setting")
            .expect("value");
        assert!(
            !raw.to_string().contains("private"),
            "cursor storage must not carry the source path"
        );

        clear_cursors(&repository, Some("codex")).expect("clear one");
        assert!(
            !load_cursors(&repository)
                .expect("load")
                .contains_key("codex")
        );
        clear_cursors(&repository, None).expect("clear all");
        assert!(load_cursors(&repository).expect("load").is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unparsable_timestamps_are_rejected_not_silently_zeroed() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "tokenowl-usage-ts-{}-{}.sqlite3",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        let runtime = AppRuntime::new(&path).expect("runtime");
        let repository = runtime.repository().expect("database");
        let event = LocalUsageEvent {
            source_event_id: "bad-ts".to_owned(),
            occurred_at: "next tuesday".to_owned(),
            session_id: None,
            model: None,
            project_ref: None,
            input_tokens: Some(1),
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_estimate: None,
        };
        assert!(
            repository
                .insert_local_usage_events("claude-code", &[event], 1)
                .is_err()
        );
        let _ = std::fs::remove_file(&path);
    }
}
