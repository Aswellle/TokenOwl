//! Shared write path for user-requested export files.
//!
//! Every export lands under `<app data>/exports/` with a `tokenowl-` prefix,
//! a timestamp, and a short random suffix so two exports in the same second
//! cannot overwrite each other. The directory is bounded: after a successful
//! write, the oldest `tokenowl-*` files beyond the cap are removed. Only the
//! export directory is ever written, and a prune failure never fails the
//! export the user asked for.

use super::CommandError;
use std::path::{Path, PathBuf};

/// How many export files the directory keeps across every export kind.
pub(super) const MAX_EXPORT_FILES: usize = 20;

/// Writes `body` as `<data_dir>/exports/tokenowl-<prefix>-<stamp>-<nonce>.<ext>`
/// and prunes older exports. Returns the absolute path shown to the user.
pub(super) fn write_export(
    data_dir: &Path,
    prefix: &str,
    extension: &str,
    body: &str,
) -> Result<PathBuf, CommandError> {
    let export_dir = data_dir.join("exports");
    std::fs::create_dir_all(&export_dir).map_err(|error| {
        CommandError::InvalidInput(format!("cannot create the export directory: {error}"))
    })?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let nonce = nanoid::nanoid!(4);
    let path = export_dir.join(format!("tokenowl-{prefix}-{stamp}-{nonce}.{extension}"));
    std::fs::write(&path, body)
        .map_err(|error| CommandError::InvalidInput(format!("cannot write the export: {error}")))?;
    prune_exports(&export_dir);
    Ok(path)
}

/// Chronological sort key for one export file: `<date>-<time>-<nonce>` from
/// `tokenowl-<kind>-<date>-<time>-<nonce>.<ext>`. Names that do not match keep
/// their full text, so an unexpected file still has a deterministic order.
fn export_sort_key(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let mut parts = stem.rsplitn(4, '-');
    let nonce = parts.next().unwrap_or_default();
    let time = parts.next().unwrap_or_default();
    let date = parts.next().unwrap_or_default();
    if date.is_empty() || time.is_empty() {
        return name.to_owned();
    }
    format!("{date}-{time}-{nonce}")
}

/// Keeps the exports directory bounded: the oldest `tokenowl-*` files beyond
/// [`MAX_EXPORT_FILES`] are removed. Names carry the timestamp, so lexical
/// order is chronological.
pub(super) fn prune_exports(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut exports: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("tokenowl-"))
        })
        .collect();
    if exports.len() <= MAX_EXPORT_FILES {
        return;
    }
    // Names carry a timestamp, but the prefix differs by export kind, so
    // sorting the whole path would group by kind instead of time. Compare the
    // `<date>-<time>-<nonce>` tail that every writer appends.
    exports.sort_by_key(|path| {
        path.file_name()
            .map_or_else(String::new, |name| export_sort_key(&name.to_string_lossy()))
    });
    let excess = exports.len() - MAX_EXPORT_FILES;
    for path in exports.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_export_uses_the_export_directory_and_a_stable_prefix() {
        let data_dir = std::env::temp_dir().join(format!(
            "tokenowl-export-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        let path = write_export(&data_dir, "accounts", "json", "{}").expect("export");
        assert!(path.starts_with(data_dir.join("exports")));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("tokenowl-accounts-"));
        assert!(name.ends_with(".json"));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{}");
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn pruning_is_shared_across_export_kinds() {
        let dir = std::env::temp_dir().join(format!(
            "tokenowl-export-prune-{}-{}",
            std::process::id(),
            nanoid::nanoid!(6)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        for index in 0..(MAX_EXPORT_FILES + 3) {
            let prefix = if index % 2 == 0 {
                "history"
            } else {
                "accounts"
            };
            std::fs::write(
                dir.join(format!(
                    "tokenowl-{prefix}-20260101-0000{index:02}-aaaa.json"
                )),
                "{}",
            )
            .expect("write");
        }
        std::fs::write(dir.join("keep-me.txt"), "user file").expect("write");
        prune_exports(&dir);
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names.len(),
            MAX_EXPORT_FILES + 1,
            "only tokenowl exports are pruned"
        );
        assert!(names.contains(&"keep-me.txt".to_owned()));
        assert!(!names.iter().any(|name| name.contains("000000")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
