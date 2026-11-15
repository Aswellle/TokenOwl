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

/// A scan cursor: byte offset into one file, keyed by the file's path and
/// size+mtime signature so replaced or truncated files restart from zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCursor {
    pub file: String,
    pub offset: u64,
    pub signature: String,
}

/// Outcome of scanning one source.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOutcome {
    pub source_id: String,
    pub scanned_files: usize,
    pub events: Vec<LocalUsageEvent>,
    /// Files skipped for being too large, unreadable, or malformed — with a
    /// stable reason, never file content.
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
}
