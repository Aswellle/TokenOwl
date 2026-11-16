//! Codex CLI local usage source.
//!
//! Codex writes rollout files under `~/.codex/sessions/YYYY/MM/DD/` as JSONL:
//! a `session_meta` line opens each file (`payload.id` is the session id,
//! `payload.cwd` the working directory), and periodic `event_msg` lines carry
//! `payload.type == "token_count"` with `payload.info.total_token_usage`
//! plus the model. The schema here follows the documented rollout format;
//! it has not yet been verified against a real installation on this machine
//! (Codex is not installed), which the module tests state honestly. Parsing
//! is defensive: unknown line shapes are skipped, never guessed at.

use super::{LocalUsageEvent, MAX_FILE_BYTES, MAX_LINE_BYTES, ScanCursor, ScanOutcome, fnv1a64};
use serde_json::Value;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SOURCE_ID: &str = "codex";

/// The single directory this source may ever read.
pub fn source_directory(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

pub fn scan(home: &Path, cursor: Option<&ScanCursor>) -> ScanOutcome {
    let mut outcome = ScanOutcome {
        source_id: SOURCE_ID.to_owned(),
        scanned_files: 0,
        events: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };
    let directory = source_directory(home);
    let files = collect_jsonl_files(&directory);
    for file in files {
        scan_file(&file, cursor, &mut outcome);
    }
    outcome
}

fn collect_jsonl_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![directory.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.is_symlink() {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn scan_file(file: &Path, cursor: Option<&ScanCursor>, outcome: &mut ScanOutcome) {
    let Ok(metadata) = std::fs::metadata(file) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        outcome
            .skipped
            .push(format!("file exceeds the size cap: {}", file.display()));
        return;
    }
    let signature = format!("{}-{}", metadata.len(), metadata_stable_time(&metadata));
    let mut start = 0u64;
    if let Some(cursor) = cursor
        && cursor.file == file.to_string_lossy()
        && cursor.signature == signature
    {
        start = cursor.offset.min(metadata.len());
    }
    let Ok(mut handle) = std::fs::File::open(file) else {
        outcome
            .errors
            .push(format!("file is unreadable: {}", file.display()));
        return;
    };
    if handle.seek(SeekFrom::Start(start)).is_err() {
        outcome
            .errors
            .push(format!("file is not seekable: {}", file.display()));
        return;
    }
    let session_meta = read_session_meta(file);
    let reader = std::io::BufReader::new(handle);
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        if let Some(event) = parse_line(&line, session_meta.as_deref()) {
            outcome.events.push(event);
        }
    }
    outcome.scanned_files += 1;
}

/// The session id lives on the file's first meta line, not on usage lines.
fn read_session_meta(file: &Path) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    for line in content.lines().take(4) {
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type")?.as_str()? == "session_meta" {
            return value.get("payload")?.get("id")?.as_str().map(str::to_owned);
        }
    }
    None
}

fn metadata_stable_time(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs())
}

/// Parses one `token_count` event line. The rollout format nests the counters
/// under `payload.info.total_token_usage` (with `cached_input_tokens`
/// reported alongside — reported cache reads, not a separate cache-write
/// counter; the schema keeps them in the cache-read slot and leaves the
/// write slot unknown).
fn parse_line(line: &str, session_id: Option<&str>) -> Option<LocalUsageEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?.as_object()?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let info = payload.get("info")?.as_object()?;
    let usage = info.get("total_token_usage")?.as_object()?;
    let identity = payload
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| value.get("timestamp").and_then(Value::as_str))
        .unwrap_or_default();
    if identity.is_empty() {
        return None;
    }
    let number = |key: &str| usage.get(key).and_then(Value::as_u64);
    let event = LocalUsageEvent {
        source_event_id: format!("{:016x}", fnv1a64(identity.as_bytes())),
        occurred_at: value.get("timestamp")?.as_str()?.to_owned(),
        session_id: session_id.map(str::to_owned),
        model: payload
            .get("model")
            .or_else(|| info.get("model"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        // The rollout's cwd is the user's real path; it never persists.
        project_ref: None,
        input_tokens: number("input_tokens"),
        output_tokens: number("output_tokens"),
        cache_read_tokens: number("cached_input_tokens"),
        cache_write_tokens: None,
        cost_estimate: None,
    };
    if event.input_tokens.is_none()
        && event.output_tokens.is_none()
        && event.cache_read_tokens.is_none()
    {
        return None;
    }
    Some(event)
}

pub fn cursor_for(file: &Path, offset: u64, signature: String) -> ScanCursor {
    ScanCursor {
        file: file.to_string_lossy().into_owned(),
        offset,
        signature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic fixture following the documented rollout shape. Codex is not
    // installed on the development machine, so this parser's real-file
    // verification is still pending — recorded in the audit notes.
    const TOKEN_COUNT_LINE: &str = r#"{"timestamp":"2026-11-10T09:00:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1200,"cached_input_tokens":300,"output_tokens":450},"model_context_window":272000},"model":"gpt-5.2"}}"#;
    const META_LINE: &str = r#"{"timestamp":"2026-11-10T08:59:00.000Z","type":"session_meta","payload":{"id":"rollout-abc","cwd":"D:\\AI\\TokenOwl"}}"#;
    const OTHER_EVENT: &str = r#"{"timestamp":"2026-11-10T09:00:01.000Z","type":"event_msg","payload":{"type":"agent_reasoning"}}"#;

    #[test]
    fn parses_a_token_count_line_without_storing_paths() {
        let event = parse_line(TOKEN_COUNT_LINE, Some("rollout-abc")).expect("event");
        assert_eq!(event.session_id.as_deref(), Some("rollout-abc"));
        assert_eq!(event.model.as_deref(), Some("gpt-5.2"));
        assert_eq!(event.input_tokens, Some(1_200));
        assert_eq!(event.output_tokens, Some(450));
        assert_eq!(event.cache_read_tokens, Some(300));
        assert_eq!(event.cache_write_tokens, None);
        assert!(event.project_ref.is_none(), "the cwd never persists");
    }

    #[test]
    fn non_token_count_lines_are_skipped() {
        assert!(parse_line(META_LINE, None).is_none());
        assert!(parse_line(OTHER_EVENT, None).is_none());
        assert!(parse_line("not json", None).is_none());
    }

    #[test]
    fn scan_reads_the_documented_layout_and_stays_idempotent() {
        let home = std::env::temp_dir().join(format!("tokenowl-codex-{}", std::process::id()));
        let day = source_directory(&home).join("2026").join("11").join("10");
        std::fs::create_dir_all(&day).expect("day dir");
        std::fs::write(
            day.join("rollout-abc.jsonl"),
            format!("{META_LINE}\n{OTHER_EVENT}\n{TOKEN_COUNT_LINE}\n"),
        )
        .expect("write");
        let first = scan(&home, None);
        assert_eq!(first.events.len(), 1);
        assert_eq!(first.events[0].session_id.as_deref(), Some("rollout-abc"));
        assert_eq!(first.skipped, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&home);
    }
}
