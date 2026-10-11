// SQLite persistence layer
// Original content moved from src-tauri/src/persistence.rs
//! Local `SQLite` persistence for provider configuration, accounts, snapshots, and refresh state.
//!
//! The database deliberately stores normalized data only. Provider response bodies and
//! credential values never cross this module's API or appear in its schema.

use crate::domain::provider::{
    OpenAiCompatibleConfig, QuotaSnapshot, QuotaSnapshotSet, SnapshotStatus,
};
use crate::domain::state::{
    AccountMetadata, Freshness, FreshnessPolicy, RefreshState, RefreshStatus,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

mod migrations;

pub use migrations::SCHEMA_VERSION;

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
    #[error("local database failed integrity check: {messages}")]
    Integrity { messages: String },
    #[error("schema migration {version:04} ({name}) failed: {source}")]
    Migration {
        version: i32,
        name: &'static str,
        #[source]
        source: rusqlite::Error,
    },
    #[error("local database backup could not be written: {0}")]
    Backup(#[from] std::io::Error),
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

/// One stored notification. `kind`/`severity` are the stable string forms of
/// the domain enums so this layer stays independent of the notifications
/// module; the conversion back is owned there.
#[derive(Clone, Debug, PartialEq)]
pub struct NotificationRecord {
    pub id: String,
    pub account_id: Option<String>,
    pub provider_id: Option<String>,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub dedup_key: String,
    pub created_at: u64,
    pub read_at: Option<u64>,
}

/// One recorded usage observation. `unit` is the lowercase string form of
/// `QuotaUnit` (`token`/`currency`/…), kept as text so this layer stays
/// independent of the daily-aggregation policy.
#[derive(Clone, Debug, PartialEq)]
pub struct HistorySample {
    pub account_id: String,
    pub metric_key: String,
    pub observed_at: u64,
    pub used: Option<f64>,
    pub remaining: Option<f64>,
    pub limit: Option<f64>,
    pub unit: String,
    pub currency: Option<String>,
}

/// History observations older than this are pruned whenever new ones land.
pub const HISTORY_RETENTION_SECS: u64 = 90 * 86_400;

/// Synchronous repository. Tauri callers can place it behind `Arc<Mutex<_>>`.
pub struct LocalRepository {
    connection: Connection,
}

impl LocalRepository {
    /// Opens or creates a file-backed database and applies all migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let connection = Connection::open(path)?;
        Self::from_connection(connection, true, Some(path))
    }

    /// Opens an isolated database for tests or ephemeral sessions.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection, false, None)
    }

    fn from_connection(
        connection: Connection,
        enable_wal: bool,
        database_path: Option<&Path>,
    ) -> Result<Self, StorageError> {
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
        migrations::apply_migrations(&connection, database_path)?;
        Ok(Self { connection })
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
    /// Trusted snapshots are also recorded as history observations in the same
    /// transaction (idempotent per timestamp), so the usage timeline grows with
    /// every refresh without a separate write path.
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
            if matches!(
                snapshot.status,
                SnapshotStatus::Available | SnapshotStatus::Partial
            ) {
                let unit = snapshot.unit.as_ref().map_or("unknown".to_owned(), |unit| {
                    format!("{unit:?}").to_lowercase()
                });
                transaction.execute(
                    "INSERT OR IGNORE INTO history_points
                        (account_id, metric_key, observed_at, used, remaining, quota_limit, unit, currency)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        account_id,
                        key,
                        observed_at,
                        snapshot.used,
                        snapshot.remaining,
                        snapshot.limit,
                        unit,
                        snapshot.currency,
                    ],
                )?;
            }
        }
        transaction.execute(
            "DELETE FROM history_points WHERE observed_at < ?1",
            [observed_at.saturating_sub(HISTORY_RETENTION_SECS)],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// History observations inside `[from, to)`, ordered so consecutive points
    /// of one metric are adjacent (the shape the daily aggregator expects).
    pub fn history_samples(&self, from: u64, to: u64) -> Result<Vec<HistorySample>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT account_id, metric_key, observed_at, used, remaining, quota_limit, unit, currency
               FROM history_points
              WHERE observed_at >= ?1 AND observed_at < ?2
              ORDER BY account_id, metric_key, observed_at",
        )?;
        let rows = statement.query_map(params![from, to], |row| {
            Ok(HistorySample {
                account_id: row.get(0)?,
                metric_key: row.get(1)?,
                observed_at: row.get(2)?,
                used: row.get(3)?,
                remaining: row.get(4)?,
                limit: row.get(5)?,
                unit: row.get(6)?,
                currency: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
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

    /// Inserts a notification unless the same `dedup_key` was already recorded
    /// within `cooldown_secs`. Returns `true` when the row was written, i.e.
    /// the caller may raise the banner.
    pub fn insert_notification(
        &self,
        record: NotificationRecord,
        cooldown_secs: u64,
        now: u64,
    ) -> Result<bool, StorageError> {
        let cooldown_start = now.saturating_sub(cooldown_secs);
        let recent: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM notifications WHERE dedup_key = ?1 AND created_at > ?2",
            params![record.dedup_key, cooldown_start],
            |row| row.get(0),
        )?;
        if recent > 0 {
            return Ok(false);
        }
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO notifications
                (id, account_id, provider_id, kind, severity, title, message, dedup_key, created_at, read_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.id,
                record.account_id,
                record.provider_id,
                record.kind,
                record.severity,
                record.title,
                record.message,
                record.dedup_key,
                record.created_at,
                record.read_at,
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// Most recent notifications, newest first.
    pub fn notifications(&self, limit: u32) -> Result<Vec<NotificationRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, account_id, provider_id, kind, severity, title, message, dedup_key, created_at, read_at
               FROM notifications ORDER BY created_at DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([i64::from(limit.max(1))], |row| {
            Ok(NotificationRecord {
                id: row.get(0)?,
                account_id: row.get(1)?,
                provider_id: row.get(2)?,
                kind: row.get(3)?,
                severity: row.get(4)?,
                title: row.get(5)?,
                message: row.get(6)?,
                dedup_key: row.get(7)?,
                created_at: row.get(8)?,
                read_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// Marks the given notifications read; unknown ids are ignored.
    pub fn mark_notifications_read(&self, ids: &[&str], now: u64) -> Result<usize, StorageError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = ids
            .iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL AND id IN ({placeholders})"
        );
        let mut parameters: Vec<&dyn rusqlite::ToSql> = vec![&now];
        parameters.extend(ids.iter().map(|id| id as &dyn rusqlite::ToSql));
        Ok(self.connection.execute(&sql, parameters.as_slice())?)
    }

    /// Marks every unread notification read; returns how many changed.
    pub fn mark_all_notifications_read(&self, now: u64) -> Result<usize, StorageError> {
        Ok(self.connection.execute(
            "UPDATE notifications SET read_at = ?1 WHERE read_at IS NULL",
            [now],
        )?)
    }

    /// Bounds history growth: drops rows older than `retention_secs`, then
    /// keeps at most `max_rows` newest.
    pub fn prune_notifications(
        &self,
        now: u64,
        retention_secs: u64,
        max_rows: i64,
    ) -> Result<(), StorageError> {
        let cutoff = now.saturating_sub(retention_secs);
        self.connection
            .execute("DELETE FROM notifications WHERE created_at < ?1", [cutoff])?;
        self.connection.execute(
            "DELETE FROM notifications WHERE id NOT IN (
                 SELECT id FROM notifications ORDER BY created_at DESC, rowid DESC LIMIT ?1
             )",
            [max_rows],
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
    use crate::domain::provider::{Confidence, DataSource, QuotaUnit, SnapshotStatus};
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
            unit: Some(QuotaUnit::Token),
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

    fn notification_record(id: &str, dedup_key: &str, created_at: u64) -> NotificationRecord {
        NotificationRecord {
            id: id.to_owned(),
            account_id: Some("account-1".to_owned()),
            provider_id: Some("generic".to_owned()),
            kind: "low_quota".to_owned(),
            severity: "WARNING".to_owned(),
            title: "额度紧张".to_owned(),
            message: "仅剩 5%。".to_owned(),
            dedup_key: dedup_key.to_owned(),
            created_at,
            read_at: None,
        }
    }

    /// Notifications carry foreign keys, so standalone inserts need the
    /// referenced provider and account to exist — same as the live enqueue path.
    fn seed_account(repository: &LocalRepository) {
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
    }

    #[test]
    fn notifications_dedup_within_cooldown_and_repeat_afterwards() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let first = repository
            .insert_notification(
                notification_record("n1", "low-quota:a1", 1_000),
                86_400,
                1_000,
            )
            .expect("insert");
        assert!(first, "first occurrence must be stored");
        let suppressed = repository
            .insert_notification(
                notification_record("n2", "low-quota:a1", 2_000),
                86_400,
                2_000,
            )
            .expect("insert");
        assert!(!suppressed, "cooldown must suppress the repeat");
        let later = repository
            .insert_notification(
                notification_record("n3", "low-quota:a1", 1_000 + 86_400 + 1),
                86_400,
                1_000 + 86_400 + 1,
            )
            .expect("insert");
        assert!(later, "after the cooldown the condition may fire again");
    }

    #[test]
    fn notifications_list_is_newest_first_and_mark_read_only_touches_unread() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        for index in 0..3_u64 {
            repository
                .insert_notification(
                    notification_record(&format!("n{index}"), &format!("k{index}"), 1_000 + index),
                    86_400,
                    1_000 + index,
                )
                .expect("insert");
        }
        let listed = repository.notifications(2).expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "n2", "newest first");

        let changed = repository
            .mark_notifications_read(&["n1", "unknown"], 5_000)
            .expect("mark");
        assert_eq!(changed, 1, "unknown ids are ignored");
        let changed = repository
            .mark_notifications_read(&["n1"], 5_001)
            .expect("mark");
        assert_eq!(changed, 0, "already-read rows stay untouched");
        let all = repository
            .mark_all_notifications_read(5_002)
            .expect("mark all");
        assert_eq!(all, 2, "only the still-unread rows change");
        let listed = repository.notifications(10).expect("list");
        assert!(listed.iter().all(|record| record.read_at.is_some()));
        assert_eq!(
            listed
                .iter()
                .find(|record| record.id == "n1")
                .and_then(|record| record.read_at),
            Some(5_000),
            "earlier read timestamp is preserved"
        );
    }

    #[test]
    fn notifications_prune_by_age_and_cap() {
        let repository = LocalRepository::open_in_memory().expect("database");
        seed_account(&repository);
        let now = 5_000_000_u64;
        for index in 0..10_u64 {
            let created_at = now - (9 - index) * 100;
            repository
                .insert_notification(
                    notification_record(&format!("n{index}"), &format!("k{index}"), created_at),
                    0,
                    created_at,
                )
                .expect("insert");
        }
        // Everything is recent relative to now, so only the cap can drop rows.
        repository
            .prune_notifications(now, 30 * 86_400, 100)
            .expect("prune");
        assert_eq!(repository.notifications(100).expect("list").len(), 10);
        // Cap keeps the newest rows only.
        repository
            .prune_notifications(now, 30 * 86_400, 4)
            .expect("prune");
        let remaining = repository.notifications(100).expect("list");
        assert_eq!(remaining.len(), 4);
        assert_eq!(remaining[0].id, "n9");
    }

    #[test]
    fn notifications_survive_account_deletion_with_cleared_link() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        repository
            .insert_notification(
                notification_record("n1", "low-quota:account-1", 1_000),
                0,
                1_000,
            )
            .expect("insert");
        repository.delete_account("account-1").expect("delete");
        let remaining = repository.notifications(10).expect("list");
        assert_eq!(remaining.len(), 1, "history is kept");
        assert_eq!(
            remaining[0].account_id, None,
            "link cleared by ON DELETE SET NULL"
        );
    }

    #[test]
    fn history_records_trusted_snapshots_idempotently_and_respects_monotonic_guard() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), 1_000)
            .expect("first record");
        // Same observation again is idempotent; an older set is ignored.
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), 1_000)
            .expect("repeat record");
        let mut older = snapshot();
        older.used = Some(5.0);
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![older]), 999)
            .expect("older ignored");
        let samples = repository.history_samples(0, 2_000).expect("samples");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].used, Some(10.0));
        assert_eq!(samples[0].unit, "token");
        assert_eq!(samples[0].metric_key, "quota-0-Token");

        // Untrusted snapshots carry no history values.
        let mut untrusted = snapshot();
        untrusted.status = SnapshotStatus::Unavailable;
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![untrusted]), 1_001)
            .expect("untrusted upsert");
        assert_eq!(
            repository.history_samples(0, 2_000).expect("samples").len(),
            1,
            "untrusted status must not add a history row"
        );
    }

    #[test]
    fn history_prunes_beyond_retention_and_cascades_on_account_delete() {
        let repository = LocalRepository::open_in_memory().expect("database");
        repository.upsert_provider(&config(), 1).expect("provider");
        repository.upsert_account(&account(), 1).expect("account");
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), 1_000)
            .expect("old record");
        // A much later refresh prunes everything older than the retention window.
        let late = 1_000 + HISTORY_RETENTION_SECS + 60;
        repository
            .upsert_snapshot_set("account-1", &QuotaSnapshotSet::new(vec![snapshot()]), late)
            .expect("late record");
        let samples = repository.history_samples(0, late + 1).expect("samples");
        assert_eq!(samples.len(), 1, "only the fresh observation survives");
        assert_eq!(samples[0].observed_at, late);

        repository.delete_account("account-1").expect("delete");
        assert_eq!(
            repository.history_samples(0, late + 1).expect("samples"),
            [],
            "history rows cascade with the account"
        );
    }
}
