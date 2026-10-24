// Community provider manifests
//! Loads user- or community-supplied provider manifests at runtime.
//!
//! Community manifests are a trust boundary, so the loader is deliberately
//! strict: every file must parse as a complete manifest, declare an
//! `openai-compatible` wire protocol (the only transport a data file can
//! safely request), declare an `https` base URL, and carry a unique id that
//! does not collide with a built-in. Invalid files are skipped and reported,
//! never partially registered. There is no signature verification yet — that
//! belongs to a distribution channel that does not exist, and pretending
//! otherwise would be worse than stating the limitation (docs/08: license and
//! provenance review happens before anything ships as built-in).

use crate::providers::manifest::ProviderManifest;
use std::path::{Path, PathBuf};
use url::Url;

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

/// Upper bound on manifest files read in one scan. A directory stuffed with
/// packs must not slow startup or grow the skip list without limit.
const MAX_MANIFEST_FILES: usize = 32;
/// Upper bound on one manifest file, checked before reading. Legitimate
/// packs are a few KiB; anything near this size is data, not configuration.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
/// Upper bound on reported skip reasons, so a directory full of broken packs
/// stays a summary instead of a wall of text in the UI.
const MAX_SKIPPED_REASONS: usize = 8;

/// Scans `directory` for `*.yaml` / `*.yml` manifests and validates each.
/// Missing directories are not an error — community packs are optional — but
/// an unreadable directory is reported instead of silently swallowed.
pub fn scan_community_manifests(directory: &Path) -> CommunityScan {
    let mut scan = CommunityScan::default();
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return scan,
        Err(error) => {
            scan.skipped.push(format!(
                "{}: directory unreadable ({error})",
                directory.display()
            ));
            return scan;
        }
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("yaml" | "yml")
                ) {
                    files.push(path);
                }
            }
            Err(error) => {
                scan.skipped
                    .push(format!("directory entry unreadable ({error})"));
            }
        }
    }
    files.sort();
    if files.len() > MAX_MANIFEST_FILES {
        let overflow = files.len() - MAX_MANIFEST_FILES;
        files.truncate(MAX_MANIFEST_FILES);
        scan.skipped.push(format!(
            "{overflow} more files skipped beyond the {MAX_MANIFEST_FILES}-file pack limit"
        ));
    }

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
    if scan.skipped.len() > MAX_SKIPPED_REASONS {
        // Truncate to one below the cap so the summary line itself stays
        // within the bound.
        let overflow = scan.skipped.len() - (MAX_SKIPPED_REASONS - 1);
        scan.skipped.truncate(MAX_SKIPPED_REASONS - 1);
        scan.skipped
            .push(format!("{overflow} more files skipped (details omitted)"));
    }
    scan
}

/// Loads and validates one community manifest file.
fn load_community_manifest(path: &Path) -> Result<CommunityManifest, String> {
    // `symlink_metadata` does not follow links: a symlink wearing a `.yaml`
    // name is not a regular file and must be refused rather than followed to
    // an arbitrary target.
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| format!("unreadable ({error})"))?;
    if !metadata.is_file() {
        return Err("not a regular file, skipped".to_owned());
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "file exceeds the {} KiB manifest limit",
            MAX_MANIFEST_BYTES / 1024
        ));
    }
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
    // The shared transport denies redirects unconditionally; a pack asking to
    // follow them is rejected with a clear reason rather than registering
    // with different network behavior than it declares.
    if let Some(reason) = manifest.unsupported_network_request() {
        return Err(reason);
    }
    validate_base_url(&manifest.base_url)?;
    Ok(CommunityManifest {
        manifest,
        source_path: path.to_path_buf(),
    })
}

/// Community packs must declare an absolute `https` base URL.
///
/// Unlike a hand-typed account, a pack arrives as a file from outside the
/// application, so a plaintext or scheme-less URL would silently downgrade
/// the credential-bearing request path. Gateways that legitimately need a
/// user-supplied host (`customBaseUrl`) are built-ins: their URL comes from
/// the account form, not from the data file.
fn validate_base_url(base_url: &str) -> Result<(), String> {
    let parsed = Url::parse(base_url.trim())
        .map_err(|error| format!("base URL is not a valid absolute URL ({error})"))?;
    if parsed.scheme() != "https" {
        return Err(format!(
            "base URL must use https (found `{}`)",
            parsed.scheme()
        ));
    }
    if parsed.host_str().is_none() {
        return Err("base URL has no host".to_owned());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        // The base URL is persisted to SQLite, which must never hold secrets.
        return Err("base URL must not embed credentials".to_owned());
    }
    Ok(())
}

/// Deduplicates community manifests by id and against the built-in set.
/// The first file per id wins (files are scanned in sorted order, so the
/// outcome is stable); collisions with built-ins have already been rejected
/// by the scan.
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
        assert_eq!(scan.loaded.len(), 0);
        assert_eq!(scan.skipped.len(), 0);
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
        assert_eq!(scan.skipped.len(), 0);
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
        assert_eq!(scan.loaded.len(), 0);
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
        assert_eq!(scan.loaded.len(), 0);
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

    /// A single oversized manifest must be skipped by size, never read into
    /// memory wholesale.
    #[test]
    fn oversized_manifest_is_skipped_by_size() {
        let dir = std::env::temp_dir().join("tokenowl-community-oversize");
        let _ = std::fs::remove_dir_all(&dir);
        let mut pack = String::from(
            "# padding\nid: big\nversion: 1\nname: Big\nauth: [api_key]\nbaseUrl: https://big.test\n",
        );
        while pack.len() < 300 * 1024 {
            pack.push_str(
                "# aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            );
        }
        write_pack(&dir, "big.yaml", &pack);
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 0);
        assert!(
            scan.skipped[0].contains("exceeds"),
            "size limit reason expected, got: {}",
            scan.skipped[0]
        );
    }

    /// Something that is not a regular file (a directory wearing a `.yaml`
    /// name, or a symlink) must be refused, not read.
    #[test]
    fn non_regular_files_are_refused() {
        let dir = std::env::temp_dir().join("tokenowl-community-nonfile");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(&dir, "good.yaml", VALID_PACK);
        std::fs::create_dir_all(dir.join("trap.yaml")).expect("create trap dir");
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 1, "the good pack still loads");
        assert!(
            scan.skipped.iter().any(|reason| reason.contains("trap.yaml") && reason.contains("not a regular file")),
            "trap must be refused as non-regular, got {:?}",
            scan.skipped
        );
    }

    /// The scan cost is bounded: beyond the file cap the rest is not even
    /// read, and the skip list itself is truncated with a summary line.
    #[test]
    fn file_count_is_capped_and_skip_list_is_truncated() {
        let dir = std::env::temp_dir().join("tokenowl-community-many");
        let _ = std::fs::remove_dir_all(&dir);
        for index in 0..(MAX_MANIFEST_FILES + 4) {
            write_pack(&dir, &format!("m{index:03}.yaml"), "id: [unclosed");
        }
        let scan = scan_community_manifests(&dir);
        assert_eq!(
            scan.loaded.len(),
            0,
            "every file here is invalid; none may load"
        );
        assert!(
            scan.skipped.len() <= MAX_SKIPPED_REASONS,
            "skip list must be truncated, got {} entries",
            scan.skipped.len()
        );
        assert!(
            scan.skipped
                .iter()
                .any(|reason| reason.contains("more files skipped")),
            "truncation must be summarized, got {:?}",
            scan.skipped
        );
    }

    #[test]
    fn plaintext_base_url_is_rejected() {
        let dir = std::env::temp_dir().join("tokenowl-community-http");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(
            &dir,
            "plain.yaml",
            &VALID_PACK.replace("https://", "http://"),
        );
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 0);
        assert_eq!(scan.skipped.len(), 1);
        assert!(scan.skipped[0].contains("must use https"));
    }

    #[test]
    fn non_https_schemes_and_relative_urls_are_rejected() {
        let dir = std::env::temp_dir().join("tokenowl-community-schemes");
        let _ = std::fs::remove_dir_all(&dir);
        let cases = [
            ("file-scheme.yaml", "file:///etc/tokenowl/pack"),
            ("ftp-scheme.yaml", "ftp://relay.acme.test/v1"),
            ("scheme-less.yaml", "relay.acme.test/v1"),
            ("hostless.yaml", "https://"),
        ];
        for (name, url) in cases {
            write_pack(
                &dir,
                name,
                &VALID_PACK.replace("https://relay.acme.test/v1", url),
            );
        }
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 0);
        assert_eq!(scan.skipped.len(), cases.len());
        for reason in &scan.skipped {
            assert!(
                reason.contains("base URL"),
                "unexpected skip reason: {reason}"
            );
        }
    }

    #[test]
    fn credentialed_base_url_is_rejected() {
        let dir = std::env::temp_dir().join("tokenowl-community-userinfo");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(
            &dir,
            "creds.yaml",
            &VALID_PACK.replace(
                "https://relay.acme.test/v1",
                "https://user:secret@relay.acme.test/v1",
            ),
        );
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 0);
        assert!(scan.skipped[0].contains("must not embed credentials"));
    }

    /// Positive control for the tightening: an https URL with a port and a
    /// nested path is still a valid pack, so the rule rejects only what it
    /// claims to.
    #[test]
    fn https_url_with_port_and_path_still_loads() {
        let dir = std::env::temp_dir().join("tokenowl-community-https-ok");
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(
            &dir,
            "port.yaml",
            &VALID_PACK.replace(
                "https://relay.acme.test/v1",
                "https://relay.acme.test:8443/api/openai",
            ),
        );
        let scan = scan_community_manifests(&dir);
        assert_eq!(scan.loaded.len(), 1);
        assert_eq!(scan.skipped.len(), 0);
        assert_eq!(
            scan.loaded[0].manifest.base_url,
            "https://relay.acme.test:8443/api/openai"
        );
    }
}
