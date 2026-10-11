CREATE TABLE IF NOT EXISTS providers (
    id TEXT PRIMARY KEY NOT NULL,
    config_json TEXT NOT NULL,
    descriptor_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY NOT NULL,
    provider_id TEXT NOT NULL,
    label TEXT NOT NULL,
    account_type TEXT NOT NULL,
    credential_ref TEXT,
    metadata_json TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS accounts_provider_idx ON accounts(provider_id);

CREATE TABLE IF NOT EXISTS snapshots (
    account_id TEXT NOT NULL,
    snapshot_key TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    observed_at INTEGER NOT NULL CHECK (observed_at >= 0),
    PRIMARY KEY (account_id, snapshot_key),
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS refresh_states (
    account_id TEXT PRIMARY KEY NOT NULL,
    state_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
