//! Codex CLI local usage source.
//!
//! Codex writes rollout files under `~/.codex/sessions/YYYY/MM/DD/` as JSONL:
//! a `session_meta` line opens each file (`payload.id` is the session id,
//! `payload.cwd` the working directory — never persisted here), a
//! `turn_context` line names the model for the following turns, and
//! `event_msg` lines with `payload.type == "token_count"` carry usage.
//!
//! The counters come in two shapes: `info.last_token_usage` is the delta for
//! the turn just completed, `info.total_token_usage` is the session's running
//! total. Per-turn records use the delta — summing the cumulative totals
//! repeatedly would over-count a session — and cumulative-only streams are
//! differenced here. The schema follows the public rollout format; it has not
//! been verified against a real installation on this machine (Codex is not
//! installed), which the source list states honestly. Unknown line shapes are
//! skipped, never guessed at.

use super::{
    LocalUsageEvent, ScanCursor, ScanOutcome, file_identity, file_label, fnv1a64, resume_start,
    scan_jsonl_file,
};
use serde_json::Value;
use std::io::Read as _;
use std::path::{Path, PathBuf};

pub const SOURCE_ID: &str = "codex";

/// The single directory this source may ever read.
pub fn source_directory(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

/// Header bytes read to discover the session id and the first turn's model.
/// Rollout headers are a few hundred bytes; this is generous without reading
/// a large file twice.
const HEAD_BYTES: u64 = 64 * 1024;

pub fn scan(home: &Path, cursors: &[ScanCursor]) -> ScanOutcome {
    let mut outcome = ScanOutcome {
        source_id: SOURCE_ID.to_owned(),
        scanned_files: 0,
        events: Vec::new(),
        cursors: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };
    let directory = source_directory(home);
    let files = collect_jsonl_files(&directory);
    for file in files {
        scan_file(&file, cursors, &mut outcome);
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

fn scan_file(file: &Path, cursors: &[ScanCursor], outcome: &mut ScanOutcome) {
    let mut parser = LineParser::new(file, cursors);
    let mut events = Vec::new();
    scan_jsonl_file(file, cursors, outcome, |line, line_start| {
        if let Some(event) = parser.parse(line, line_start) {
            events.push(event);
        }
    });
    outcome.events.extend(events);
    outcome.skipped.extend(parser.skips);
}

/// per-file parser state: the session identity from the header, the model in
/// force, and the last cumulative total so cumulative-only streams can be
/// differenced without re-reading the file.
struct LineParser {
    file_key: String,
    file_label: String,
    session_id: Option<String>,
    model: Option<String>,
    previous_total: Option<TokenUsage>,
    resumed: bool,
    saw_token_count: bool,
    skipped_baseline: bool,
    skips: Vec<String>,
}

impl LineParser {
    fn new(file: &Path, cursors: &[ScanCursor]) -> Self {
        let head = read_head_context(file);
        Self {
            file_key: file_identity(file),
            file_label: file_label(file),
            session_id: head.session_id,
            model: head.model,
            previous_total: None,
            resumed: resume_start(file, cursors) > 0,
            saw_token_count: false,
            skipped_baseline: false,
            skips: Vec::new(),
        }
    }

    fn parse(&mut self, line: &str, line_start: u64) -> Option<LocalUsageEvent> {
        let value: Value = serde_json::from_str(line).ok()?;
        match value.get("type").and_then(Value::as_str)? {
            // The model can change between turns; the most recent context
            // line applies to the token counts that follow it.
            "turn_context" => {
                if let Some(next) = value
                    .get("payload")
                    .and_then(|payload| payload.get("model"))
                    .and_then(Value::as_str)
                {
                    self.model = Some(next.to_owned());
                }
                return None;
            }
            "session_meta" => {
                if let Some(id) = value
                    .get("payload")
                    .and_then(|payload| payload.get("id"))
                    .and_then(Value::as_str)
                {
                    self.session_id = Some(id.to_owned());
                }
                return None;
            }
            "event_msg" => {}
            _ => return None,
        }
        let payload = value.get("payload")?.as_object()?;
        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            return None;
        }
        let info = payload.get("info")?.as_object()?;
        let first_token_count = !self.saw_token_count;
        self.saw_token_count = true;
        let total = info
            .get("total_token_usage")
            .and_then(Value::as_object)
            .map(TokenUsage::from_object);
        let last = info
            .get("last_token_usage")
            .and_then(Value::as_object)
            .map(TokenUsage::from_object);
        let usage = self.resolve_usage(last, total.as_ref(), first_token_count)?;
        if let Some(total) = total {
            self.previous_total = Some(total);
        }
        if !usage.has_billable_tokens() {
            return None;
        }
        let occurred_at = value.get("timestamp").and_then(Value::as_str)?;
        let identity = format!("{}:{line_start}", self.file_key);
        let model_for_event = payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.model.clone());
        Some(LocalUsageEvent {
            source_event_id: format!("{:016x}", fnv1a64(identity.as_bytes())),
            occurred_at: occurred_at.to_owned(),
            session_id: self.session_id.clone(),
            model: model_for_event,
            // The rollout's cwd is the user's real path; it never persists.
            project_ref: None,
            input_tokens: usage.input,
            output_tokens: usage.output,
            cache_read_tokens: usage.cache_read,
            cache_write_tokens: None,
            cost_estimate: None,
        })
    }

    /// Chooses the per-turn delta. On a full scan the session starts at zero,
    /// so the first cumulative total is its own delta. When the scan resumes,
    /// the first visible total has an unseen baseline; its delta stays unknown
    /// instead of being invented as a full-session total. The baseline is
    /// still recorded, so later cumulative events difference correctly.
    fn resolve_usage(
        &mut self,
        last: Option<TokenUsage>,
        total: Option<&TokenUsage>,
        first_token_count: bool,
    ) -> Option<TokenUsage> {
        match (last, total) {
            (Some(last), _) => last.has_billable_tokens().then_some(last),
            (None, Some(total)) => {
                if let Some(baseline) = self.previous_total.as_ref() {
                    return Some(total.difference_from(baseline));
                }
                self.previous_total = Some(total.clone());
                if self.resumed && first_token_count {
                    if !self.skipped_baseline {
                        self.skipped_baseline = true;
                        let label = self.file_label.clone();
                        self.skips.push(format!(
                            "{label}: cumulative token_count after resume has no baseline"
                        ));
                    }
                    None
                } else {
                    Some(total.clone())
                }
            }
            (None, None) => None,
        }
    }
}

/// The fields this source records from one `TokenUsage` object. Cache writes
/// have no separate counter in the rollout format, so that slot stays
/// unknown rather than duplicating a read count into it.
#[derive(Debug, Clone, Default)]
struct TokenUsage {
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
}

impl TokenUsage {
    fn from_object(object: &serde_json::Map<String, Value>) -> Self {
        let number = |key: &str| object.get(key).and_then(Value::as_u64);
        Self {
            input: number("input_tokens"),
            output: number("output_tokens"),
            cache_read: number("cached_input_tokens"),
        }
    }

    fn has_billable_tokens(&self) -> bool {
        [self.input, self.output, self.cache_read]
            .into_iter()
            .flatten()
            .any(|value| value > 0)
    }

    /// Per-field difference from an earlier cumulative total. A field missing
    /// from either side stays unknown; a counter that moves backwards is a
    /// reset or a rewritten file, not negative usage, so it clamps to zero.
    fn difference_from(&self, earlier: &Self) -> Self {
        let subtract = |now: Option<u64>, before: Option<u64>| match (now, before) {
            (Some(now), Some(before)) => Some(now.saturating_sub(before)),
            _ => now,
        };
        Self {
            input: subtract(self.input, earlier.input),
            output: subtract(self.output, earlier.output),
            cache_read: subtract(self.cache_read, earlier.cache_read),
        }
    }
}

struct HeadContext {
    session_id: Option<String>,
    model: Option<String>,
}

/// Reads the first [`HEAD_BYTES`] of a rollout and extracts the session id
/// and the most recent turn model there. The full file is never read into
/// memory just to find its header.
fn read_head_context(file: &Path) -> HeadContext {
    let mut context = HeadContext {
        session_id: None,
        model: None,
    };
    let Ok(handle) = std::fs::File::open(file) else {
        return context;
    };
    let mut head = Vec::new();
    if handle.take(HEAD_BYTES).read_to_end(&mut head).is_err() {
        return context;
    }
    let head = String::from_utf8_lossy(&head);
    for line in head.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                if let Some(id) = value
                    .get("payload")
                    .and_then(|payload| payload.get("id"))
                    .and_then(Value::as_str)
                {
                    context.session_id = Some(id.to_owned());
                }
            }
            Some("turn_context") => {
                if let Some(model) = value
                    .get("payload")
                    .and_then(|payload| payload.get("model"))
                    .and_then(Value::as_str)
                {
                    context.model = Some(model.to_owned());
                }
            }
            _ => {}
        }
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic fixtures following the public rollout shape. Codex is not
    // installed on the development machine, so real-file verification is
    // still pending — recorded in the audit notes and the source-list UI.
    const META_LINE: &str = r#"{"timestamp":"2026-11-10T08:59:00.000Z","type":"session_meta","payload":{"id":"rollout-abc","cwd":"D:\\AI\\TokenOwl"}}"#;
    const TURN_CONTEXT: &str = r#"{"timestamp":"2026-11-10T08:59:30.000Z","type":"turn_context","payload":{"model":"gpt-5.2-codex","cwd":"D:\\AI\\TokenOwl"}}"#;
    const LAST_LINE: &str = r#"{"timestamp":"2026-11-10T09:00:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1200,"cached_input_tokens":300,"output_tokens":450},"total_token_usage":{"input_tokens":1200,"cached_input_tokens":300,"output_tokens":450}}}}"#;
    const LAST_LINE_2: &str = r#"{"timestamp":"2026-11-10T09:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":50},"total_token_usage":{"input_tokens":1300,"cached_input_tokens":320,"output_tokens":500}}}}"#;
    const CUMULATIVE_1: &str = r#"{"timestamp":"2026-11-10T09:02:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":2000,"cached_input_tokens":400,"output_tokens":700}}}}"#;
    const CUMULATIVE_2: &str = r#"{"timestamp":"2026-11-10T09:03:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":2300,"cached_input_tokens":450,"output_tokens":760}}}}"#;
    const OTHER_EVENT: &str = r#"{"timestamp":"2026-11-10T09:00:01.000Z","type":"event_msg","payload":{"type":"agent_reasoning"}}"#;

    fn home(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tokenowl-codex-{name}-{}-{}",
            std::process::id(),
            nanoid::nanoid!(4)
        ))
    }

    fn write_session(home: &Path, body: &str) -> std::path::PathBuf {
        let day = source_directory(home).join("2026").join("11").join("10");
        std::fs::create_dir_all(&day).expect("day dir");
        let file = day.join("rollout-abc.jsonl");
        std::fs::write(&file, body).expect("write");
        file
    }

    #[test]
    fn per_turn_deltas_are_summed_not_cumulative_totals() {
        let home = home("deltas");
        write_session(
            &home,
            &format!("{META_LINE}\n{TURN_CONTEXT}\n{OTHER_EVENT}\n{LAST_LINE}\n{LAST_LINE_2}\n"),
        );
        let outcome = scan(&home, &[]);
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].session_id.as_deref(), Some("rollout-abc"));
        assert_eq!(outcome.events[0].model.as_deref(), Some("gpt-5.2-codex"));
        assert_eq!(outcome.events[0].input_tokens, Some(1_200));
        assert_eq!(outcome.events[0].output_tokens, Some(450));
        assert_eq!(outcome.events[0].cache_read_tokens, Some(300));
        assert_eq!(outcome.events[0].cache_write_tokens, None);
        assert!(
            outcome.events[0].project_ref.is_none(),
            "the cwd never persists"
        );
        // The second event is 100/50, not the 1300/500 cumulative total: a
        // naive total-based parser over-counts every multi-turn session.
        assert_eq!(outcome.events[1].input_tokens, Some(100));
        assert_eq!(outcome.events[1].output_tokens, Some(50));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cumulative_only_streams_are_differenced() {
        let home = home("cumulative");
        write_session(
            &home,
            &format!("{META_LINE}\n{CUMULATIVE_1}\n{CUMULATIVE_2}\n"),
        );
        let outcome = scan(&home, &[]);
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].input_tokens, Some(2_000));
        assert_eq!(outcome.events[1].input_tokens, Some(300));
        assert_eq!(outcome.events[1].output_tokens, Some(60));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn non_usage_lines_are_skipped() {
        let home = home("skip");
        write_session(&home, &format!("{META_LINE}\n{OTHER_EVENT}\nnot json\n"));
        let outcome = scan(&home, &[]);
        assert_eq!(outcome.events, Vec::new());
        assert_eq!(outcome.errors, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_complete_scan_resumes_without_re_emitting_or_double_counting() {
        let home = home("resume");
        let file = write_session(
            &home,
            &format!("{META_LINE}\n{TURN_CONTEXT}\n{LAST_LINE}\n"),
        );
        let first = scan(&home, &[]);
        assert_eq!(first.events.len(), 1);
        let second = scan(&home, &first.cursors);
        assert!(second.events.is_empty(), "nothing new was appended");

        // Append one more turn. The resumed scan sees only the new bytes and
        // uses the per-turn delta from that record.
        let mut body = std::fs::read_to_string(&file).expect("read");
        body.push_str(LAST_LINE_2);
        body.push('\n');
        std::fs::write(&file, body).expect("append");
        let third = scan(&home, &second.cursors);
        assert_eq!(third.events.len(), 1);
        assert_eq!(third.events[0].input_tokens, Some(100));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A half-written trailing line must not be parsed or skipped past: the
    /// cursor stays before it, so completing the line later picks it up.
    #[test]
    fn a_partial_trailing_line_waits_for_its_newline() {
        let home = home("partial");
        let file = write_session(&home, &format!("{META_LINE}\n"));
        let first = scan(&home, &[]);
        assert_eq!(first.events, Vec::new());

        let mut body = std::fs::read_to_string(&file).expect("read");
        let split = LAST_LINE.len() / 2;
        body.push_str(&LAST_LINE[..split]);
        std::fs::write(&file, &body).expect("write partial");
        let second = scan(&home, &first.cursors);
        assert!(second.events.is_empty(), "a truncated record is not usage");
        assert_eq!(second.cursors, first.cursors, "the cursor stops before it");

        body.push_str(&LAST_LINE[split..]);
        body.push('\n');
        std::fs::write(&file, body).expect("complete");
        let third = scan(&home, &second.cursors);
        assert_eq!(third.events.len(), 1);
        assert_eq!(third.events[0].input_tokens, Some(1_200));
        let _ = std::fs::remove_dir_all(&home);
    }
}
