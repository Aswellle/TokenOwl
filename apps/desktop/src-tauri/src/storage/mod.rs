// SQLite persistence layer
// Original content moved from src-tauri/src/persistence.rs
//! Local `SQLite` persistence for provider configuration, accounts, snapshots, and refresh state.
//!
//! The database deliberately stores normalized data only. Provider response bodies and
//! credential values never cross this module's API or appear in its schema.

use crate::domain::provider::{OpenAiCompatibleConfig, QuotaSnapshot, QuotaSnapshotSet};
use crate::domain::state::{
    AccountMetadata, Freshness, FreshnessPolicy, RefreshState, RefreshStatus,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

pub const SCHEMA_VERSION: i32 = 1;

const SCHEMA_V1: &str = r"
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
";

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite storage error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialized local data is invalid: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("local database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i32, supported: i32 },
    #[error("SQLite pragma {name} did not take effect: {value}")]
    Pragma { name: &'static str, value: String },
    #[error("local storage value is invalid: {0}")]
    InvalidData(String),
}

/// A normalized snapshot together with its local observation time and freshness.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotViewDto {
    pub account_id: String,
    pub snapshot: QuotaSnapshot,
    pub observed_at: u64,
    pub freshness: Freshness,
}

/// Serializable account view for later Tauri command integration.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountViewDto {
    pub account: AccountMetadata,
    pub provider: Option<OpenAiCompatibleConfig>,
    pub snapshots: Vec<SnapshotViewDto>,
    pub refresh: RefreshState,
}

/// Synchronous repository. Tauri callers can place it behind `Arc<Mutex<_>>`.
pub struct LocalRepository {
    connection: Connection,
}

impl LocalRepository {
    /// Opens or creates a file-backed database and applies all migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection, true)
    }

    /// Opens an isolated database for tests or ephemeral sessions.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection, false)
    }

    fn from_connection(connection: Connection, enable_wal: bool) -> Result<Self, StorageError> {
        connection.pragma_update(None, "foreign_keys", true)?;
        if enable_wal {
            let mode: String =
                connection
                    .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(StorageError::Pragma {
                    name: "journal_mode",
                    value: mode,
                });
            }
        }
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let repository = Self { connection };
        repository.migrate()?;
        Ok(repository)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        let version: i32 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema {
                found: version,
                supported: SCHEMA_VERSION,
            });
        }
        if version < 1 {
            self.connection.execute_batch("BEGIN IMMEDIATE;")?;
            let migration = self.connection.execute_batch(SCHEMA_V1);
            if let Err(error) = migration {
                let _ = self.connection.execute_batch("ROLLBACK;");
                return Err(error.into());
            }
            self.connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
            self.connection.execute_batch("COMMIT;")?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i32, StorageError> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn upsert_provider(
        &self,
        config: &OpenAiCompatibleConfig,
        updated_at: u64,
    ) -> Result<(), StorageError> {
        config
            .validate()
            .map_err(|error| StorageError::InvalidData(error.message))?;
        let config_json = serde_json::to_string(config)?;
        let descriptor_json = serde_json::to_string(&config.descriptor())?;
        self.connection.execute(
            "INSERT INTO providers (id, config_json, descriptor_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json,
                 descriptor_json = excluded.descriptor_json,
                 updated_at = excluded.updated_at",
            params![config.id, config_json, descriptor_json, updated_at],
        )?;
        Ok(())
    }

    pub fn provider(
        &self,
        provider_id: &str,
    ) -> Result<Option<OpenAiCompatibleConfig>, StorageError> {
        let serialized = self
            .connection
            .query_row(
                "SELECT config_json FROM providers WHERE id = ?1",
                [provider_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        serialized
            .map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
    }

    pub fn providers(&self) -> Result<Vec<OpenAiCompatibleConfig>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT config_json FROM providers ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let serialized = rows.collect::<Result<Vec<_>, _>>()?;
        serialized
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .collect()
    }

    pub fn upsert_account(
        &self,
        account: &AccountMetadata,
        updated_at: u64,
    ) -> Result<(), StorageError> {
        let metadata_json = serde_json::to_string(&account.metadata)?;
        self.connection.execute(
            "INSERT INTO accounts
                (id, provider_id, label, account_type, credential_ref, metadata_json, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET provider_id = excluded.provider_id,
                 label = excluded.label,
                 account_type = excluded.account_type,
                 credential_ref = excluded.credential_ref,
                 metadata_json = excluded.metadata_json,
                 enabled = excluded.enabled,
                 updated_at = excluded.updated_at",
            params![
                account.id,
                account.provider_id,
                account.label,
                account.account_type,
                account.credential_ref,
                metadata_json,
                i64::from(account.enabled),
                updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn account(&self, account_id: &str) -> Result<Option<AccountMetadata>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, provider_id, label, account_type, credential_ref,
                        metadata_json, enabled
                   FROM accounts WHERE id = ?1",
                [account_id],
                |row| {
                    let metadata_json: String = row.get(5)?;
                    let enabled: i64 = row.get(6)?;
                    Ok(AccountMetadata {
                        id: row.get(0)?,
                        provider_id: row.get(1)?,
                        label: row.get(2)?,
                        account_type: row.get(3)?,
                        credential_ref: row.get(4)?,
                        metadata: serde_json::from_str(&metadata_json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        enabled: enabled != 0,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn accounts(&self) -> Result<Vec<AccountMetadata>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, provider_id, label, account_type, credential_ref,
                    metadata_json, enabled
               FROM accounts ORDER BY label, id",
        )?;
        let rows = statement.query_map([], |row| {
            let metadata_json: String = row.get(5)?;
            let enabled: i64 = row.get(6)?;
            Ok(AccountMetadata {
                id: row.get(0)?,
                provider_id: row.get(1)?,
                label: row.get(2)?,
                account_type: row.get(3)?,
                credential_ref: row.get(4)?,
                metadata: serde_json::from_str(&metadata_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                enabled: enabled != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn delete_account(&self, account_id: &str) -> Result<(), StorageError> {
        self.connection
            .execute("DELETE FROM accounts WHERE id = ?1", [account_id])?;
        Ok(())
    }

    /// Persists only normalized snapshots and atomically replaces the prior set.
    pub fn upsert_snapshot_set(
        &self,
        account_id: &str,
        snapshots: &QuotaSnapshotSet,
        observed_at: u64,
    ) -> Result<(), StorageError> {
        let latest_observed_at = self.connection.query_row(
            "SELECT MAX(observed_at) FROM snapshots WHERE account_id = ?1",
            [account_id],
            |row| row.get::<_, Option<u64>>(0),
        )?;
        if latest_observed_at.is_some_and(|latest| observed_at < latest) {
            return Ok(());
        }
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM snapshots WHERE account_id = ?1", [account_id])?;
        for (index, snapshot) in snapshots.iter().enumerate() {
            let key = snapshot_key(snapshot, index);
            let snapshot_json = serde_json::to_string(snapshot)?;
            transaction.execute(
                "INSERT INTO snapshots (account_id, snapshot_key, snapshot_json, observed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![account_id, key, snapshot_json, observed_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn latest_snapshot_views(
        &self,
        account_id: &str,
        now: u64,
        policy: FreshnessPolicy,
    ) -> Result<Vec<SnapshotViewDto>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT snapshot_json, observed_at FROM snapshots
               WHERE account_id = ?1 ORDER BY snapshot_key",
        )?;
        let rows = statement.query_map([account_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        let mut views = Vec::new();
        for row in rows {
            let (serialized, observed_at) = row?;
            let snapshot: QuotaSnapshot = serde_json::from_str(&serialized)?;
            let freshness = policy.classify(observed_at, now);
            if freshness != Freshness::Expired {
                views.push(SnapshotViewDto {
                    account_id: account_id.to_owned(),
                    snapshot,
                    observed_at,
                    freshness,
                });
            }
        }
        Ok(views)
    }

    pub fn refresh_state(&self, account_id: &str) -> Result<RefreshState, StorageError> {
        let serialized = self
            .connection
            .query_row(
                "SELECT state_json FROM refresh_states WHERE account_id = ?1",
                [account_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        serialized
            .map(|value| serde_json::from_str(&value).map_err(StorageError::from))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn save_refresh_state(
        &self,
        account_id: &str,
        state: &RefreshState,
        updated_at: u64,
    ) -> Result<(), StorageError> {
        let state_json = serde_json::to_string(state)?;
        self.connection.execute(
            "INSERT INTO refresh_states (account_id, state_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET state_json = excluded.state_json,
                 updated_at = excluded.updated_at",
            params![account_id, state_json, updated_at],
        )?;
        Ok(())
    }

    pub fn mark_refresh_started(
        &self,
        account_id: &str,
        started_at: u64,
    ) -> Result<RefreshState, StorageError> {
        let mut state = self.refresh_state(account_id)?;
        state.status = RefreshStatus::Refreshing;
        state.last_started_at = Some(started_at);
        state.error_code = None;
        self.save_refresh_state(account_id, &state, started_at)?;
        Ok(state)
    }

    pub fn mark_refresh_succeeded(
        &self,
        account_id: &str,
        completed_at: u64,
    ) -> Result<RefreshState, StorageError> {
        let mut state = self.refresh_state(account_id)?;
        state.status = RefreshStatus::Succeeded;
        state.consecutive_failures = 0;
        state.last_completed_at = Some(completed_at);
        state.next_attempt_at = None;
        state.error_code = None;
        self.save_refresh_state(account_id, &state, completed_at)?;
        Ok(state)
    }

    pub fn mark_refresh_failed(
        &self,
        account_id: &str,
        failed_at: u64,
        error_code: impl Into<String>,
        next_attempt_at: Option<u64>,
    ) -> Result<RefreshState, StorageError> {
        let mut state = self.refresh_state(account_id)?;
        state.status = RefreshStatus::Failed;
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_completed_at = Some(failed_at);
        state.next_attempt_at = next_attempt_at;
        state.error_code = Some(error_code.into());
        self.save_refresh_state(account_id, &state, failed_at)?;
        Ok(state)
    }

    pub fn account_view(
        &self,
        account_id: &str,
        now: u64,
        policy: FreshnessPolicy,
    ) -> Result<Option<AccountViewDto>, StorageError> {
        let Some(account) = self.account(account_id)? else {
            return Ok(None);
        };
        let provider = self.provider(&account.provider_id)?;
        let snapshots = self.latest_snapshot_views(account_id, now, policy)?;
        let refresh = self.refresh_state(account_id)?;
        Ok(Some(AccountViewDto {
            account,
            provider,
            snapshots,
            refresh,
        }))
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let conn = &self.connection;
        let result = conn.query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), StorageError> {
        let conn = &self.connection;
        let json = serde_json::to_string(value)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![key, json, now],
        )?;
        Ok(())
    }
}

fn snapshot_key(snapshot: &QuotaSnapshot, index: usize) -> String {
    let base = snapshot
        .id
        .as_deref()
        .or(snapshot.metric_id.as_deref())
        .or(snapshot.metric_name.as_deref())
        .map_or_else(|| format!("snapshot-{index}"), ToOwned::to_owned);
    let suffix = snapshot
        .unit
        .as_ref()
        .map_or_else(|| index.to_string(), |u| format!("{index}-{u:?}"));
    format!("{base}-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::{Confidence, DataSource, SnapshotStatus};
    use std::collections::HashMap;
    use std::time::Duration;

    fn policy() -> FreshnessPolicy {
        FreshnessPolicy {
            fresh_for: Duration::from_secs(60),
            stale_after: Duration::from_secs(300),
            hard_expire_after: Duration::from_secs(600),
        }
    }

    fn config() -> OpenAiCompatibleConfig {
        OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1")
    }

    fn account() -> AccountMetadata {
        AccountMetadata::from_auth_input(
            "account-1",
            "generic",
            "Test account",
            "api_key",
            Some("credential-ref-1".to_owned()),
            HashMap::from([
                ("region".to_owned(), "test".to_owned()),
                ("api_key".to_owned(), "must-not-persist".to_owned()),
            ]),
            true,
            Some("must-not-persist"),
        )
    }

    fn snapshot() -> QuotaSnapshot {
        QuotaSnapshot {
            id: Some("quota".to_owned()),
            account_id: Some("account-1".to_owned()),
            metric_id: Some("tokens".to_owned()),
            metric_name: Some("Tokens".to_owned()),
            used: Some(10.0),
            limit: Some(100.0),
            remaining: Some(90.0),
            source: DataSource::CustomEndpoint,
            confidence: Confidence::Exact,
            status: SnapshotStatus::Available,
            ..QuotaSnapshot::default()
        }
    }

    #[test]
    fn migration_enables_foreign_keys_and_schema_version() {
        let repository = LocalRepository::open_in_memory().expect("database");
        assert_eq!(
            repository.schema_version().expect("version"),
            SCHEMA_VERSION
        );
        repository
            .upsert_account(&account(), 1)
            .expect_err("foreign key should reject an unknown provider");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
    }

    #[test]
    fn secrets_are_absent_from_serialized_account_and_database() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        let account_json =
            serde_json::to_string(&repository.account("account-1").expect("read")).expect("json");
        assert!(!account_json.contains("must-not-persist"));
        let mut statement = repository
            .connection
            .prepare("SELECT metadata_json FROM accounts")
            .expect("query");
        let stored: String = statement.query_row([], |row| row.get(0)).expect("stored");
        assert!(!stored.contains("must-not-persist"));
    }

    #[test]
    fn snapshots_are_monotonic_and_expire_at_hard_boundary() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), 100)
            .expect("snapshot");
        let mut older = snapshot();
        older.used = Some(1.0);
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![older]), 99)
            .expect("older snapshot ignored");
        let views = repository
            .latest_snapshot_views("account-1", 100 + 60, policy())
            .expect("freshness");
        assert_eq!(views[0].snapshot.used, Some(10.0));
        assert_eq!(views[0].freshness, Freshness::Stale);
        let expired = repository
            .latest_snapshot_views("account-1", 700, policy())
            .expect("expiry");
        assert_eq!(expired, Vec::<SnapshotViewDto>::new());
    }

    #[test]
    fn newer_empty_snapshot_sets_clear_previous_values() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), 100)
            .expect("snapshot");
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::default(), 101)
            .expect("empty snapshot set");
        let cleared = repository
            .latest_snapshot_views("account-1", 101, policy())
            .expect("views");
        assert_eq!(cleared, Vec::<SnapshotViewDto>::new());
    }
}
