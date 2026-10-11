-- Notification history. Rows are rendered, normalized text — never raw
-- provider payloads or secrets. `account_id`/`provider_id` keep their history
-- when the referenced records disappear, so deleting an account preserves the
-- audit trail while clearing the link.

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT,
    provider_id TEXT,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    dedup_key TEXT NOT NULL,
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    read_at INTEGER,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE SET NULL,
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS notifications_created_at_idx ON notifications(created_at);
CREATE INDEX IF NOT EXISTS notifications_dedup_idx ON notifications(dedup_key, created_at);
