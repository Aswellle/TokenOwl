//! Local AI-coding-agent usage sources.
//!
//! This is a separate data domain from the remote API quota pipeline: local
//! session records describe what coding agents consumed on this machine,
//! never what a provider's key-level API reports. The two never merge into
//! one number.
//!
//! Privacy contract (docs/05, goal spec): every source is opt-in, read-only,
//! restricted to its known directories, scanned incrementally with a
//! resumable cursor, and deduplicated idempotently. Prompt bodies, source
//! snippets, environment values, and raw log files are never read into
//! persistence, IPC, or logs — only token counts, model names, timestamps,
//! and session identifiers. Nothing here uploads anything anywhere.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read as _, Seek as _};
use std::path::Path;

/// One normalized local usage observation. All token fields stay separate —
/// cache tokens are never folded into input or output — and a missing value
/// stays `None`, never zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUsageEvent {
    /// Deterministic dedup key within the source (the record's own uuid or
    /// an equivalent stable identity). Never a hash of message content.
    pub source_event_id: String,
    /// RFC 3339 UTC instant of the record.
    pub occurred_at: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    /// The tool's own project identifier (its encoded project directory
    /// name); never an absolute filesystem path.
    pub project_ref: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    /// No local price table ships with the app, so cost is always unknown
    /// until a real price source exists — never estimated into existence.
    pub cost_estimate: Option<f64>,
}

/// A scan cursor: byte offset into one file, keyed by a stable hash of the
/// file's path, a fingerprint of the file's head, and a fingerprint of the
/// bytes immediately before the offset. Appending leaves all three
/// unchanged, so the next scan resumes; replacing or rewriting the file
/// changes at least one, so the scan restarts instead of misreading from a
/// stale offset. The path itself is deliberately not persisted: the cursor
/// only needs identity, and a full local path would leak the user's directory
/// layout into SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCursor {
    pub file_id: String,
    pub offset: u64,
    /// Fingerprint of the first [`HEAD_SIGNATURE_BYTES`] bytes.
    pub signature: String,
    /// Fingerprint of the last [`ANCHOR_BYTES`] bytes before `offset`.
    pub anchor: String,
}

/// Bytes that identify a file's head. Any replacement or truncation at the
/// start changes this, while appends leave it alone.
const HEAD_SIGNATURE_BYTES: usize = 4 * 1024;
/// Bytes immediately before the cursor that prove the file's tail still
/// matches what was already read.
const ANCHOR_BYTES: u64 = 256;

/// Stable identity of one source file for cursor matching. FNV-1a is used
/// because it never changes between releases, unlike a randomized hasher.
pub fn file_identity(file: &Path) -> String {
    format!("{:016x}", fnv1a64(file.to_string_lossy().as_bytes()))
}

/// Fingerprint of the first `min(offset, HEAD_SIGNATURE_BYTES)` bytes, or
/// `None` when the file cannot be read. Limiting the range to bytes the scan
/// already consumed keeps an append from changing the fingerprint.
fn head_signature(path: &Path, offset: u64) -> Option<String> {
    let length = offset.min(u64::try_from(HEAD_SIGNATURE_BYTES).unwrap_or(u64::MAX));
    let handle = std::fs::File::open(path).ok()?;
    let mut head = Vec::new();
    handle.take(length).read_to_end(&mut head).ok()?;
    Some(format!("{:016x}:{}", fnv1a64(&head), head.len()))
}

/// Fingerprint of the bytes immediately before `offset`. `offset == 0` hashes
/// the empty slice, which is a valid anchor for a fresh file.
fn anchor_at(path: &Path, offset: u64) -> Option<String> {
    let start = offset.saturating_sub(ANCHOR_BYTES);
    let length = offset - start;
    let mut handle = std::fs::File::open(path).ok()?;
    handle.seek(std::io::SeekFrom::Start(start)).ok()?;
    let mut tail = Vec::new();
    handle.take(length).read_to_end(&mut tail).ok()?;
    if u64::try_from(tail.len()).unwrap_or(0) != length {
        return None;
    }
    Some(format!("{:016x}", fnv1a64(&tail)))
}

/// The cursor a completed file scan leaves behind, when the file is still
/// readable at that offset.
fn cursor_after_scan(file: &Path, offset: u64) -> Option<ScanCursor> {
    Some(ScanCursor {
        file_id: file_identity(file),
        offset,
        signature: head_signature(file, offset)?,
        anchor: anchor_at(file, offset)?,
    })
}

/// Outcome of scanning one source.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOutcome {
    pub source_id: String,
    pub scanned_files: usize,
    pub events: Vec<LocalUsageEvent>,
    /// Cursors after this scan, one per complete file read. Persisting these
    /// makes the next scan resume at the last complete line instead of
    /// re-reading every file.
    pub cursors: Vec<ScanCursor>,
    /// Files skipped for being too large, unreadable, or malformed — with a
    /// stable reason, never file content and never an absolute path.
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// Hard caps protecting the scanner (and the user) from runaway files.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// Lines longer than this cannot be a usage record worth keeping.
const MAX_LINE_BYTES: usize = 256 * 1024;

/// Stable, dependency-free FNV-1a 64: the dedup and identity hashes must not
/// change between app versions, which a randomized hasher would not honor.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Reads one line, keeping at most [`MAX_LINE_BYTES`] payload bytes in
/// `buffer` while still consuming the whole line. Returns
/// `(bytes_consumed, complete, truncated)`: `complete` is false when the
/// file ended without a trailing newline, which is how a scan avoids
/// parsing — and then skipping past — a half-written record, and `truncated`
/// is true when payload bytes had to be dropped because the line is longer
/// than the cap.
fn read_bounded_line(
    reader: &mut impl BufRead,
    buffer: &mut Vec<u8>,
) -> std::io::Result<(usize, bool, bool)> {
    buffer.clear();
    let mut total = 0usize;
    let mut truncated = false;
    loop {
        let (consumed, newline) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok((total, false, truncated));
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            // The payload in this chunk is everything before the newline (or
            // the whole chunk while the line continues). Only payload bytes
            // count against the cap, and only their loss truncates — a line
            // whose payload exactly fills the cap is complete, not oversized.
            let payload = newline.unwrap_or(available.len());
            let keep = payload.min(MAX_LINE_BYTES.saturating_sub(buffer.len()));
            if keep > 0 {
                buffer.extend_from_slice(&available[..keep]);
            }
            truncated = truncated || payload > keep;
            let consumed = newline.map_or(available.len(), |position| position + 1);
            (consumed, newline.is_some())
        };
        total += consumed;
        reader.consume(consumed);
        if newline {
            return Ok((total, true, truncated));
        }
    }
}

/// Where a scan of this file would resume: zero unless a cursor matches the
/// file identity, its head fingerprint, its length bound, and the bytes just
/// before the recorded offset. Also used by parsers that need to know whether
/// they are seeing the whole session or only its tail.
pub(crate) fn resume_start(file: &Path, cursors: &[ScanCursor]) -> u64 {
    let Ok(metadata) = std::fs::metadata(file) else {
        return 0;
    };
    let identity = file_identity(file);
    cursors
        .iter()
        .find(|cursor| {
            cursor.file_id == identity
                && cursor.offset <= metadata.len()
                && head_signature(file, cursor.offset).as_deref() == Some(cursor.signature.as_str())
                && anchor_at(file, cursor.offset).as_deref() == Some(cursor.anchor.as_str())
        })
        .map_or(0, |cursor| cursor.offset)
}

/// Scans one JSONL file incrementally: resumes from the cursor that matches
/// the file identity and signature, parses only complete lines, and records
/// the resulting cursor. A file larger than [`MAX_FILE_BYTES`] is skipped
/// without being opened.
///
/// `on_line` receives the line text (without its terminator) and the line's
/// byte offset; the offset is stable across scans and is what per-line dedup
/// identities should use.
pub(crate) fn scan_jsonl_file(
    file: &Path,
    cursors: &[ScanCursor],
    outcome: &mut ScanOutcome,
    mut on_line: impl FnMut(&str, u64),
) {
    let Ok(metadata) = std::fs::metadata(file) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        outcome
            .skipped
            .push(format!("{}: file exceeds the size cap", file_label(file)));
        return;
    }
    let start = resume_start(file, cursors);
    let Ok(mut handle) = std::fs::File::open(file) else {
        outcome
            .errors
            .push(format!("{}: file is unreadable", file_label(file)));
        return;
    };
    if handle.seek(std::io::SeekFrom::Start(start)).is_err() {
        outcome
            .errors
            .push(format!("{}: file is not seekable", file_label(file)));
        return;
    }
    let mut reader = std::io::BufReader::new(handle);
    let mut buffer = Vec::new();
    let mut offset = start;
    loop {
        let Ok((total, complete, truncated)) = read_bounded_line(&mut reader, &mut buffer) else {
            outcome
                .errors
                .push(format!("{}: read failed", file_label(file)));
            break;
        };
        if total == 0 {
            break;
        }
        if !complete {
            // A half-written trailing record stays for the next scan; its
            // bytes are not consumed because the cursor stops before it.
            break;
        }
        let line_start = offset;
        offset += u64::try_from(total).unwrap_or(u64::MAX);
        if truncated {
            // Oversized line: consume it, never parse a truncated record.
            outcome
                .skipped
                .push(format!("{}: line exceeds the size cap", file_label(file)));
            continue;
        }
        let text = String::from_utf8_lossy(&buffer);
        let text = text.trim_end_matches(['\n', '\r']);
        if !text.is_empty() {
            on_line(text, line_start);
        }
    }
    if let Some(cursor) = cursor_after_scan(file, offset) {
        outcome.cursors.push(cursor);
    }
    outcome.scanned_files += 1;
}

/// A file name safe to show in UI skip/error summaries. Absolute paths never
/// leave the scanner: the source contract promises the user's directory
/// layout is not collected.
pub(crate) fn file_label(file: &Path) -> String {
    file.file_name().map_or_else(
        || "unknown file".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub mod claude_code;
pub mod codex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_is_stable_and_collision_free_for_our_keys() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
    }

    /// A line whose payload exactly fills the cap is a complete record, not
    /// an oversized one: only lines *longer* than the cap are skipped, and
    /// the scan keeps reading the lines behind a skipped one.
    #[test]
    fn a_line_exactly_the_cap_is_parsed_and_longer_ones_are_skipped() {
        let dir = std::env::temp_dir().join(format!(
            "tokenowl-usageline-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        std::fs::create_dir_all(&dir).expect("dir");

        let mut outcome = ScanOutcome {
            source_id: "test".to_owned(),
            scanned_files: 0,
            events: Vec::new(),
            cursors: Vec::new(),
            skipped: Vec::new(),
            errors: Vec::new(),
        };
        let mut seen: Vec<String> = Vec::new();

        let exact = dir.join("exact.jsonl");
        let payload = "x".repeat(MAX_LINE_BYTES);
        std::fs::write(&exact, format!("{payload}\n{{\"tail\":true}}\n")).expect("write");
        scan_jsonl_file(&exact, &[], &mut outcome, |line, _offset| {
            seen.push(line.to_owned());
        });
        assert_eq!(
            seen.len(),
            2,
            "the cap-filling line and its tail both parse"
        );
        assert_eq!(seen[0].len(), MAX_LINE_BYTES);
        assert!(outcome.skipped.is_empty(), "nothing was oversized");
        assert_eq!(outcome.errors, Vec::<String>::new());

        seen.clear();
        outcome.skipped.clear();
        let oversized = dir.join("oversized.jsonl");
        std::fs::write(&oversized, format!("{payload}y\n{{\"tail\":true}}\n")).expect("write");
        scan_jsonl_file(&oversized, &[], &mut outcome, |line, _offset| {
            seen.push(line.to_owned());
        });
        assert_eq!(seen.len(), 1, "only the tail parses");
        assert_eq!(seen[0], "{\"tail\":true}");
        assert_eq!(outcome.skipped.len(), 1);
        assert!(outcome.skipped[0].contains("line exceeds the size cap"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
