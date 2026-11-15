//! Claude Code local usage source.
//!
//! Claude Code appends one JSON object per line to
//! `~/.claude/projects/<encoded-project>/<session-uuid>.jsonl`. Assistant
//! lines carry `message.usage` with the four token counters and a per-line
//! `uuid` that serves as the dedup identity. Schema verified against a real
//! 2026-11 installation; unknown lines are skipped, never guessed at.

use super::{LocalUsageEvent, MAX_FILE_BYTES, MAX_LINE_BYTES, ScanCursor, ScanOutcome, fnv1a64};
use serde_json::Value;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SOURCE_ID: &str = "claude-code";

/// The single directory this source may ever read.
pub fn source_directory(home: &Path) -> PathBuf {
    home.join(".claude").join("projects")
}

/// Scans every `*.jsonl` under the source directory, resuming from the
/// cursor when the file has not changed. `events` are new records only;
/// re-running the scan without new data yields nothing (idempotent).
pub fn scan(home: &Path, cursor: Option<&ScanCursor>) -> ScanOutcome {
    let mut outcome = ScanOutcome {
        source_id: SOURCE_ID.to_owned(),
        scanned_files: 0,
        events: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };
    let directory = source_directory(home);
    // An absent directory means Claude Code is not installed: an empty
    // scan, not an error.
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
                // Refuse to follow symlinks out of the allowed tree.
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
    let project_ref = file
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let reader = std::io::BufReader::new(handle);
    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        if let Some(event) = parse_line(&line, project_ref.as_deref()) {
            outcome.events.push(event);
        }
    }
    outcome.scanned_files += 1;
}

/// mtime truncated to seconds — enough to detect rewrite-vs-append together
/// with the size.
fn metadata_stable_time(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs())
}

/// Parses one JSONL line into an event. Only assistant lines carrying a
/// usage object qualify; everything else (user prompts, tool results,
/// summaries) is skipped without being interpreted.
fn parse_line(line: &str, project_ref: Option<&str>) -> Option<LocalUsageEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let message = value.get("message")?.as_object()?;
    let usage = message.get("usage")?.as_object()?;
    let uuid = value.get("uuid")?.as_str()?;
    let number = |key: &str| usage.get(key).and_then(Value::as_u64);
    let event = LocalUsageEvent {
        source_event_id: format!("{:016x}", fnv1a64(uuid.as_bytes())),
        occurred_at: value.get("timestamp")?.as_str()?.to_owned(),
        session_id: value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        model: message
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        project_ref: project_ref.map(str::to_owned),
        input_tokens: number("input_tokens"),
        output_tokens: number("output_tokens"),
        cache_read_tokens: number("cache_read_input_tokens"),
        cache_write_tokens: number("cache_creation_input_tokens"),
        cost_estimate: None,
    };
    // A record without any counter or timestamp is not a usage observation.
    if event.input_tokens.is_none()
        && event.output_tokens.is_none()
        && event.cache_read_tokens.is_none()
        && event.cache_write_tokens.is_none()
    {
        return None;
    }
    Some(event)
}

/// Continues reading after `offset` for incremental scans; the file's
/// size+mtime signature guards against reading a rewritten file from a
/// stale offset.
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

    const REAL_SHAPED_LINE: &str = r#"{"parentUuid":null,"isSidechain":false,"userType":"external","cwd":"D:\\AI\\TokenOwl","sessionId":"s-1","version":"2.0.76","type":"assistant","message":{"id":"msg_01","model":"deepseek-v4.1-flash","role":"assistant","content":[{"type":"text","text":"..."}],"stop_reason":null,"usage":{"input_tokens":32029,"cache_creation_input_tokens":5,"cache_read_input_tokens":384,"output_tokens":1155}},"uuid":"u-abc","timestamp":"2026-10-18T18:41:37.650Z"}"#;
    const USER_LINE: &str = r#"{"type":"user","sessionId":"s-1","message":{"role":"user","content":"the prompt text stays unread"},"uuid":"u-1","timestamp":"2026-10-18T18:40:00.000Z"}"#;

    #[test]
    fn parses_an_assistant_usage_line_without_touching_content() {
        let event = parse_line(REAL_SHAPED_LINE, Some("D--AI-TokenOwl")).expect("event");
        assert_eq!(event.source_event_id, format!("{:016x}", fnv1a64(b"u-abc")));
        assert_eq!(event.session_id.as_deref(), Some("s-1"));
        assert_eq!(event.model.as_deref(), Some("deepseek-v4.1-flash"));
        assert_eq!(event.input_tokens, Some(32_029));
        assert_eq!(event.output_tokens, Some(1_155));
        assert_eq!(event.cache_read_tokens, Some(384));
        assert_eq!(event.cache_write_tokens, Some(5));
        assert_eq!(event.project_ref.as_deref(), Some("D--AI-TokenOwl"));
        assert!(event.cost_estimate.is_none(), "no local price table exists");
    }

    #[test]
    fn user_lines_and_usageless_assistant_lines_are_skipped() {
        assert!(parse_line(USER_LINE, None).is_none());
        let empty_usage = r#"{"type":"assistant","message":{"model":"m","usage":{}},"uuid":"u-2","timestamp":"t"}"#;
        assert!(parse_line(empty_usage, None).is_none());
        assert!(parse_line("not json", None).is_none());
    }

    #[test]
    fn scan_is_incremental_and_idempotent_over_a_temp_tree() {
        let home = std::env::temp_dir().join(format!("tokenowl-usage-{}", std::process::id()));
        let project = source_directory(&home).join("D--AI-TokenOwl");
        std::fs::create_dir_all(&project).expect("project dir");
        let file = project.join("session-1.jsonl");
        std::fs::write(&file, format!("{USER_LINE}\n{REAL_SHAPED_LINE}\n")).expect("write");

        let first = scan(&home, None);
        assert_eq!(first.events.len(), 1, "only the assistant usage line");
        assert_eq!(first.scanned_files, 1);
        assert!(first.skipped.is_empty());

        // Re-scan from the previous cursor: the same line must not be
        // emitted again.
        let signature = format!(
            "{}-{}",
            std::fs::metadata(&file).expect("meta").len(),
            metadata_stable_time(&std::fs::metadata(&file).expect("meta"))
        );
        let cursor = cursor_for(
            &file,
            u64::try_from(REAL_SHAPED_LINE.len() + USER_LINE.len() + 2).expect("fits"),
            signature,
        );
        let second = scan(&home, Some(&cursor));
        assert!(second.events.is_empty(), "no re-emitted events");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_absent_installation_scans_to_nothing() {
        let home =
            std::env::temp_dir().join(format!("tokenowl-usage-absent-{}", std::process::id()));
        let outcome = scan(&home, None);
        assert!(outcome.events.is_empty());
        assert!(outcome.errors.is_empty());
        assert!(outcome.scanned_files == 0);
    }
}
