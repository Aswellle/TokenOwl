-- Historical usage observations, one row per (account, metric, observation).
-- `metric_key` reuses the snapshot key so a metric keeps a stable identity
-- across refreshes; values come only from snapshots the parser trusted.
-- Raw points are the source for daily token/cost aggregation; nothing here
-- is ever fabricated — days without observations simply have no rows.

CREATE TABLE IF NOT EXISTS history_points (
    account_id TEXT NOT NULL,
    metric_key TEXT NOT NULL,
    observed_at INTEGER NOT NULL CHECK (observed_at >= 0),
    used REAL,
    remaining REAL,
    quota_limit REAL,
    unit TEXT NOT NULL,
    currency TEXT,
    PRIMARY KEY (account_id, metric_key, observed_at),
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS history_points_observed_idx ON history_points(observed_at);
