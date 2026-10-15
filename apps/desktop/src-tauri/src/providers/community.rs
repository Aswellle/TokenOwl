// Community provider manifests
//! Loads user- or community-supplied provider manifests at runtime.
//!
//! Community manifests are a trust boundary, so the loader is deliberately
//! strict: every file must parse as a complete manifest, declare an
//! `openai-compatible` wire protocol (the only transport a data file can
//! safely request), and carry a unique id that does not collide with a
//! built-in. Invalid files are skipped and reported, never partially
//! registered. There is no signature verification yet — that belongs to a
//! distribution channel that does not exist, and pretending otherwise would
//! be worse than stating the limitation (docs/08: license and provenance
//! review happens before anything ships as built-in).

use crate::providers::manifest::ProviderManifest;
use std::path::{Path, PathBuf};

/// One community manifest, validated and ready to register.
#[derive(Debug, Clone)]
pub struct CommunityManifest {
    pub manifest: ProviderManifest,
    /// Where the YAML came from, for diagnostics and the UI source label.
    pub source_path: PathBuf,
}

/// Result of scanning a community directory.
#[derive(Debug, Default)]
pub struct CommunityScan {
    pub loaded: Vec<CommunityManifest>,
    pub skipped: Vec<String>,
}

/// Scans `directory` for `*.yaml` / `*.yml` manifests and validates each.
/// Missing directories are not an error — community packs are optional.
pub fn scan_community_manifests(directory: &Path) -> CommunityScan {
    let mut scan = CommunityScan::default();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return scan;
    };

    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("yaml" | "yml")
            )
        })
        .collect();
    files.sort();

    for path in files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        match load_community_manifest(&path) {
            Ok(manifest) => scan.loaded.push(manifest),
            Err(reason) => scan.skipped.push(format!("{name}: {reason}")),
        }
    }
    scan
}

/// Loads and validates one community manifest file.
fn load_community_manifest(path: &Path) -> Result<CommunityManifest, String> {
    let yaml = std::fs::read_to_string(path).map_err(|error| format!("unreadable ({error})"))?;
    let manifest = ProviderManifest::from_yaml(&yaml)
        .map_err(|error| format!("invalid manifest ({error})"))?;
    let format = manifest
        .api_format
        .as_deref()
        .unwrap_or("openai-compatible");
    if format != "openai-compatible" {
        return Err(format!(
            "unsupported wire protocol `{format}` (community packs are limited to openai-compatible)"
        ));
    }
    if crate::providers::builtin::BUILTIN_MANIFESTS
        .iter()
        .any(|(id, _)| *id == manifest.id)
    {
        return Err(format!(
            "id `{}` collides with a built-in provider",
            manifest.id
        ));
    }
    Ok(CommunityManifest {
        manifest,
        source_path: path.to_path_buf(),
    })
}

/// Deduplicates community manifests by id and against the built-in set.
/// Later files win within the community set; collisions with built-ins have
/// already been rejected by the scan.
pub fn dedupe_community(loaded: Vec<CommunityManifest>) -> Vec<CommunityManifest> {
    let mut seen = std::collections::BTreeSet::new();
    let mut unique = Vec::new();
    for manifest in loaded {
        if seen.insert(manifest.manifest.id.clone()) {
            unique.push(manifest);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pack(dir: &Path, name: &str, yaml: &str) -> PathBuf {
        std::fs::create_dir_all(dir).expect("create pack dir");
        let path = dir.join(name);
        std::fs::write(&path, yaml).expect("write manifest");
        path
    }

    const VALID_PACK: &str = "\
id: acme-relay
version: 1
name: Acme Relay
auth: [api_key]
baseUrl: https://relay.acme.test/v1
apiKeyHeader: Authorization
endpoints:
  models: /models
apiFormat: openai-compatible
";

    #[test]
    fn missing_directory_is_an_empty_scan() {
        let scan = scan_community_manifests(Path::new("Z:/definitely-missing"));
        assert!(scan.loaded.is_empty());
        assert!(scan.skipped.is_empty());
    }

    #[test]
    fn valid_pack_loads_with_source_path() {
        let dir = std::env::temp_dir().join("tokenowl-community-valid");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(&dir, "acme.yaml", VALID_PACK);
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 1);
        assert_eq!(scan.loaded[0].manifest.id, "acme-relay");
        assert!(scan.loaded[0].source_path.ends_with("acme.yaml"));
        assert!(scan.skipped.is_empty());
    }

    #[test]
    fn invalid_yaml_and_wrong_protocol_are_skipped_not_fatal() {
        let dir = std::env::temp_dir().join("tokenowl-community-invalid");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(&dir, "broken.yaml", "id: [unclosed");
        write_pack(
            &dir,
            "native.yaml",
            "id: native-pack\nversion: 1\nname: Native\nauth: [api_key]\nbaseUrl: https://x.test\nendpoints:\n  models: /v1/models\napiFormat: anthropic\n",
        );
        let scan = scan_community_manifests(&dir);
        assert!(scan.loaded.is_empty());
        assert_eq!(scan.skipped.len(), 2);
        assert!(scan.skipped[0].contains("broken.yaml"));
        assert!(
            scan.skipped
                .iter()
                .any(|reason| reason.contains("unsupported wire protocol"))
        );
    }

    #[test]
    fn builtin_id_collision_is_rejected() {
        let dir = std::env::temp_dir().join("tokenowl-community-collision");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(
            &dir,
            "impersonator.yaml",
            &VALID_PACK.replace("acme-relay", "openai"),
        );
        let scan = scan_community_manifests(&dir);
        assert!(scan.loaded.is_empty());
        assert!(scan.skipped[0].contains("collides with a built-in"));
    }

    #[test]
    fn dedupe_keeps_first_file_per_id() {
        let dir = std::env::temp_dir().join("tokenowl-community-dedupe");
        let _ = std::fs::remove_dir_all(&dir);
        let first = write_pack(&dir, "a-pack.yaml", VALID_PACK);
        let second = write_pack(&dir, "b-pack.yaml", &VALID_PACK.replace("v1", "v2"));
        let scan = scan_community_manifests(&dir);
        let deduped = dedupe_community(scan.loaded);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].source_path, first);
        assert_ne!(deduped[0].source_path, second);
    }
}
