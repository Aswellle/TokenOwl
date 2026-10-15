-- Model dimension for history: the column stays NULL for account-level
-- metrics and is populated once adapters start tagging model-scoped usage.
ALTER TABLE history_points ADD COLUMN model TEXT;

CREATE INDEX IF NOT EXISTS history_points_model_idx
    ON history_points(account_id, model, observed_at);
