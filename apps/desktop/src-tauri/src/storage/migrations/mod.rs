//! Versioned schema migrations with a pre-migration backup and integrity checks.
//!
//! Release process per `docs/06` §18: verify database integrity before touching
//! an existing database, keep a restorable copy while upgrading, apply every
//! pending migration in its own transaction, and refuse to open a database that
//! fails the post-migration check. A failed run leaves the database at its
//! previous version with user data intact.
//!
//! Adding a migration: drop a numbered `NNNN_name.sql` file next to this module
//! and append a [`Migration`] entry with the next version. Migrations must keep
//! user data intact; the list stays strictly ascending and starts at 1.

use super::StorageError;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

/// One schema version step. `version` must be `previous version + 1`.
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: include_str!("0001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "notifications_history",
        sql: include_str!("0002_notifications.sql"),
    },
    Migration {
        version: 3,
        name: "history_points",
        sql: include_str!("0003_history_points.sql"),
    },
    Migration {
        version: 4,
        name: "history_models",
        sql: include_str!("0004_history_models.sql"),
    },
];

pub const SCHEMA_VERSION: i32 = MIGRATIONS[MIGRATIONS.len() - 1].version;

const BACKUP_SUFFIX: &str = ".pre-migration.bak";
const BACKUP_STAGING_SUFFIX: &str = ".pre-migration.bak.new";

fn backup_path(database_path: &Path) -> PathBuf {
    suffixed(database_path, BACKUP_SUFFIX)
}

fn staging_path(database_path: &Path) -> PathBuf {
    suffixed(database_path, BACKUP_STAGING_SUFFIX)
}

fn suffixed(database_path: &Path, suffix: &str) -> PathBuf {
    let mut file_name = database_path.as_os_str().to_owned();
    file_name.push(suffix);
    PathBuf::from(file_name)
}

/// Applies every pending migration from [`MIGRATIONS`]. `database_path` enables
/// the pre-migration backup; in-memory databases skip it.
pub fn apply_migrations(
    connection: &Connection,
    database_path: Option<&Path>,
) -> Result<(), StorageError> {
    apply_migrations_with(connection, database_path, MIGRATIONS)
}

/// Testable core of [`apply_migrations`] that accepts an explicit migration list.
pub fn apply_migrations_with(
    connection: &Connection,
    database_path: Option<&Path>,
    migrations: &[Migration],
) -> Result<(), StorageError> {
    let current: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let target = latest_version(migrations);
    if current > target {
        return Err(StorageError::UnsupportedSchema {
            found: current,
            supported: target,
        });
    }
    if current == target {
        return Ok(());
    }
    integrity_check(connection)?;
    if let Some(path) = database_path {
        if has_user_tables(connection)? {
            create_backup(connection, path)?;
        }
    }
    for migration in migrations
        .iter()
        .filter(|migration| migration.version > current)
    {
        apply_one(connection, migration)?;
    }
    integrity_check(connection)?;
    Ok(())
}

fn latest_version(migrations: &[Migration]) -> i32 {
    migrations.last().map_or(0, |migration| migration.version)
}

/// Runs `PRAGMA integrity_check` and requires a single `ok` verdict. A corrupt
/// image can abort the check itself with `SQLITE_CORRUPT` instead of reporting
/// rows, so that outcome is normalized into the same integrity failure.
fn integrity_check(connection: &Connection) -> Result<(), StorageError> {
    let checked = connection
        .prepare("PRAGMA integrity_check")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        });
    match checked {
        Ok(messages) if messages.len() == 1 && messages[0].eq_ignore_ascii_case("ok") => Ok(()),
        Ok(messages) => Err(StorageError::Integrity {
            messages: messages.join("; "),
        }),
        Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseCorrupt) => {
            Err(StorageError::Integrity {
                messages: error.to_string(),
            })
        }
        Err(error) => Err(error.into()),
    }
}

/// True when the database already holds user tables, i.e. it carries data that
/// a migration must not lose. Fresh databases skip the backup.
fn has_user_tables(connection: &Connection) -> Result<bool, StorageError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Writes a consistent copy of the current database next to the original.
///
/// The copy is staged and verified before it replaces anything: `VACUUM INTO`
/// targets a temp sibling, the staged file is opened read-only and quick-
/// checked, and only then does it take the previous generation's place. A
/// failed or corrupt generation therefore leaves the last good backup and
/// the original database untouched — the previous behavior deleted the old
/// backup before writing the new one, so a failed `VACUUM` destroyed the
/// only restorable copy.
fn create_backup(connection: &Connection, database_path: &Path) -> Result<(), StorageError> {
    let backup_path = backup_path(database_path);
    let staging_path = staging_path(database_path);
    if staging_path.is_dir() {
        // Something unexpected occupies the staging name; abort loudly
        // instead of deleting an unknown directory.
        return Err(StorageError::Backup(std::io::Error::other(
            "backup staging path exists and is a directory",
        )));
    }
    // A stale staging file from an interrupted run would make VACUUM refuse
    // its target; clear it before generating.
    if staging_path.exists() {
        std::fs::remove_file(&staging_path)?;
    }
    connection.execute(
        "VACUUM INTO ?1",
        params![staging_path.display().to_string()],
    )?;
    verify_staged_backup(&staging_path)?;
    // The staged copy proved itself; now it may replace the old generation.
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    std::fs::rename(&staging_path, &backup_path)?;
    Ok(())
}

/// A staged backup only counts once it opens read-only and passes a quick
/// integrity check — a truncated or corrupt copy must never displace the
/// previous good backup.
fn verify_staged_backup(staging_path: &Path) -> Result<(), StorageError> {
    let connection =
        Connection::open_with_flags(staging_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let checked = connection
        .prepare("PRAGMA quick_check")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
        });
    match checked {
        Ok(messages) if messages.len() == 1 && messages[0].eq_ignore_ascii_case("ok") => Ok(()),
        Ok(messages) => Err(StorageError::Integrity {
            messages: format!("staged backup failed quick_check: {}", messages.join("; ")),
        }),
        Err(error) => Err(error.into()),
    }
}

/// Applies a single migration atomically. Any failure rolls the transaction
/// back, leaving `user_version` and the schema untouched at the previous level.
fn apply_one(connection: &Connection, migration: &Migration) -> Result<(), StorageError> {
    let transaction = connection.unchecked_transaction()?;
    let outcome = transaction
        .execute_batch(migration.sql)
        .and_then(|()| transaction.pragma_update(None, "user_version", migration.version));
    if let Err(source) = outcome {
        return Err(StorageError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        });
    }
    transaction
        .commit()
        .map_err(|source| StorageError::Migration {
            version: migration.version,
            name: migration.name,
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek as _, Write as _};

    fn temp_db_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tokenowl-mig-{tag}-{}-{}.db",
            std::process::id(),
            nanoid::nanoid!(8)
        ))
    }

    /// Creates a legacy database at `user_version` 0 carrying one marker row.
    fn legacy_database(path: &Path) {
        let connection = Connection::open(path).expect("create legacy db");
        connection
            .execute_batch(
                "CREATE TABLE legacy (id INTEGER PRIMARY KEY); INSERT INTO legacy VALUES (7);",
            )
            .expect("seed legacy data");
    }

    fn user_version(connection: &Connection) -> i32 {
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("user_version")
    }

    fn legacy_row_survived(path: &Path) -> bool {
        let connection = Connection::open(path).expect("reopen db");
        let id: i64 = connection
            .query_row("SELECT id FROM legacy LIMIT 1", [], |row| row.get(0))
            .expect("legacy row");
        id == 7
    }

    fn backup_path(path: &Path) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(BACKUP_SUFFIX);
        PathBuf::from(name)
    }

    fn staging_path(path: &Path) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(BACKUP_STAGING_SUFFIX);
        PathBuf::from(name)
    }

    fn cleanup(path: &Path) {
        for candidate in [path.to_path_buf(), backup_path(path), staging_path(path)] {
            if candidate.is_dir() {
                let _ = std::fs::remove_dir(candidate);
            } else {
                let _ = std::fs::remove_file(candidate);
            }
        }
    }

    #[test]
    fn fresh_database_migrates_without_backup() {
        let path = temp_db_path("fresh");
        let connection = Connection::open(&path).expect("open");
        apply_migrations_with(&connection, Some(&path), MIGRATIONS).expect("migrate fresh db");
        drop(connection);
        let connection = Connection::open(&path).expect("reopen");
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
        assert!(!backup_path(&path).exists(), "fresh db needs no backup");
        cleanup(&path);
    }

    #[test]
    fn upgrading_writes_backup_and_preserves_user_data() {
        let path = temp_db_path("upgrade");
        legacy_database(&path);
        let connection = Connection::open(&path).expect("open");
        apply_migrations_with(&connection, Some(&path), MIGRATIONS).expect("migrate legacy db");
        drop(connection);
        assert!(
            backup_path(&path).exists(),
            "backup must exist after upgrade"
        );
        assert!(legacy_row_survived(&path), "user data must survive");
        let connection = Connection::open(&path).expect("reopen");
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
        // The target schema is present next to the legacy table.
        let accounts: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'accounts'",
                [],
                |row| row.get(0),
            )
            .expect("accounts table");
        assert_eq!(accounts, 1);
        cleanup(&path);
    }

    #[test]
    fn corrupted_database_refuses_to_migrate() {
        let path = temp_db_path("corrupt");
        {
            let connection = Connection::open(&path).expect("seed db");
            connection
                .execute_batch(
                    "CREATE TABLE filler (payload TEXT);
                     INSERT INTO filler VALUES (printf('%.8000c', 'x'));
                     INSERT INTO filler VALUES (printf('%.8000c', 'y'));",
                )
                .expect("seed rows");
        }
        // Corrupt a page behind the header so the file still opens as SQLite.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open for corruption");
        file.seek(std::io::SeekFrom::Start(4096))
            .and_then(|_| file.write_all(&[0xFF; 512]))
            .and_then(|()| file.flush())
            .expect("corrupt page");
        drop(file);

        let connection = Connection::open(&path).expect("corrupt db still opens");
        let error = apply_migrations_with(&connection, Some(&path), MIGRATIONS)
            .expect_err("corrupted database must refuse to migrate");
        assert!(
            matches!(error, StorageError::Integrity { .. }),
            "expected integrity failure, got {error:?}"
        );
        assert!(
            !backup_path(&path).exists(),
            "no backup may be taken from a corrupted database"
        );
        cleanup(&path);
    }

    #[test]
    fn newer_schema_version_refuses_to_open() {
        let path = temp_db_path("newer");
        {
            let connection = Connection::open(&path).expect("open");
            connection
                .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .expect("set future version");
        }
        let connection = Connection::open(&path).expect("open");
        let error = apply_migrations_with(&connection, Some(&path), MIGRATIONS)
            .expect_err("newer schema must refuse");
        assert!(
            matches!(error, StorageError::UnsupportedSchema { .. }),
            "expected unsupported schema, got {error:?}"
        );
        assert!(!backup_path(&path).exists());
        cleanup(&path);
    }

    #[test]
    fn failed_migration_rolls_back_and_keeps_database_usable() {
        let path = temp_db_path("rollback");
        legacy_database(&path);
        let broken = [Migration {
            version: 1,
            name: "broken",
            sql: "CREATE TABLE definitely_broken (",
        }];
        let connection = Connection::open(&path).expect("open");
        let error = apply_migrations_with(&connection, Some(&path), &broken)
            .expect_err("broken migration must fail");
        assert!(
            matches!(
                error,
                StorageError::Migration {
                    version: 1,
                    name: "broken",
                    ..
                }
            ),
            "expected migration failure, got {error:?}"
        );
        assert_eq!(user_version(&connection), 0, "version must roll back");
        drop(connection);
        assert!(legacy_row_survived(&path), "data must survive a rollback");
        assert!(backup_path(&path).exists(), "backup stays for recovery");
        cleanup(&path);
    }

    #[test]
    fn empty_migration_list_is_a_noop() {
        let connection = Connection::open_in_memory().expect("memory db");
        apply_migrations_with(&connection, None, &[]).expect("no migrations");
        assert_eq!(user_version(&connection), 0);
    }

    /// A backup generation that fails mid-flight must leave the previous
    /// good backup and the original database exactly as they were — the old
    /// behavior deleted the backup before writing the new one, so one failed
    /// `VACUUM` destroyed the only restorable copy.
    #[test]
    fn failed_backup_generation_keeps_the_previous_backup() {
        let path = temp_db_path("failedbak");
        legacy_database(&path);
        std::fs::write(backup_path(&path), b"previous-good-backup").expect("seed old backup");
        // Occupy the staging name with a directory: generation aborts
        // loudly instead of deleting something unexpected.
        std::fs::create_dir_all(staging_path(&path)).expect("block staging path");

        let connection = Connection::open(&path).expect("open");
        apply_migrations_with(&connection, Some(&path), MIGRATIONS)
            .expect_err("backup failure must abort the migration");
        assert_eq!(
            std::fs::read(backup_path(&path)).expect("old backup readable"),
            b"previous-good-backup",
            "the previous backup must survive a failed generation byte-for-byte"
        );
        assert_eq!(user_version(&connection), 0, "migration must not proceed");
        drop(connection);
        assert!(legacy_row_survived(&path), "original data intact");
        cleanup(&path);
    }

    /// A successful rotation leaves no staging residue and the backup is a
    /// readable database, not merely a file that exists.
    #[test]
    fn successful_backup_rotation_verifies_and_leaves_no_staging_file() {
        let path = temp_db_path("rotate");
        legacy_database(&path);
        let connection = Connection::open(&path).expect("open");
        apply_migrations_with(&connection, Some(&path), MIGRATIONS).expect("migrate");
        drop(connection);
        assert!(
            !staging_path(&path).exists(),
            "staging file must be renamed away"
        );
        let backup = Connection::open_with_flags(
            backup_path(&path),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("backup opens as a database");
        let legacy: i64 = backup
            .query_row("SELECT id FROM legacy LIMIT 1", [], |row| row.get(0))
            .expect("backup holds the pre-migration data");
        assert_eq!(legacy, 7);
        cleanup(&path);
    }
}
