-- Local coding-agent usage events. A separate data domain from the remote
-- quota pipeline: no account, no provider foreign keys, no snapshot
-- semantics. The unique (source_id, source_event_id) pair makes every scan
-- idempotent — re-running a scan cannot duplicate a record.

CREATE TABLE local_usage_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    source_event_id TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    occurred_at_unix INTEGER NOT NULL,
    session_id TEXT,
    model TEXT,
    project_ref TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cache_read_tokens INTEGER,
    cache_write_tokens INTEGER,
    cost_estimate REAL,
    parser_version INTEGER NOT NULL DEFAULT 1,
    scanned_at INTEGER NOT NULL,
    UNIQUE (source_id, source_event_id)
);

CREATE INDEX local_usage_events_time_idx
    ON local_usage_events(occurred_at_unix);
CREATE INDEX local_usage_events_source_idx
    ON local_usage_events(source_id, occurred_at_unix);
