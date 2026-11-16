//! Offline manifest playground: validate a pasted YAML manifest and run its
//! field mappings against a pasted sample response.
//!
//! Nothing here touches the network, the credential store, the database, or
//! any local file. The sample body is parsed locally and only normalized
//! metrics leave this command — never the body itself. That keeps the
//! authoring aid inside the same privacy boundary as the rest of the app and
//! makes it usable with a redacted response.

use super::CommandError;
use crate::domain::provider::{
    FieldMappings, ProviderResponse, QuotaSnapshot, ResetObservation, parse_balance_response,
    parse_openai_compatible_lenient, parse_openai_compatible_with_mappings, parse_reset_response,
};
use crate::providers::manifest::ProviderManifest;
use serde::{Deserialize, Serialize};

/// Manifest bytes accepted from the playground. The community loader uses the
/// same limit; a pasted pack that would be skipped at load time is refused
/// here before parsing.
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Sample-response bytes accepted from the playground. A quota response is
/// small; this is generous without letting a paste exhaust memory.
const MAX_SAMPLE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPlaygroundInput {
    pub manifest_yaml: String,
    /// `models` | `quota` | `usage` | `balance` | `reset`
    pub endpoint: String,
    pub sample_body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPlaygroundView {
    /// Whether the manifest itself parses and passes the registration
    /// contract. A false value means the sample was not parsed.
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub schema_version: Option<u32>,
    pub id: Option<String>,
    pub capability_level: Option<String>,
    pub declared_endpoints: Vec<String>,
    pub metrics: Vec<PlaygroundMetricView>,
    pub reset: Option<PlaygroundResetView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundMetricView {
    pub metric_id: Option<String>,
    pub metric_name: Option<String>,
    pub unit: Option<String>,
    pub currency: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    /// `available` | `partial` | `unavailable` | `error`
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaygroundResetView {
    pub reset_at: String,
    pub window_id: Option<String>,
}

fn empty_view() -> ManifestPlaygroundView {
    ManifestPlaygroundView {
        valid: false,
        errors: Vec::new(),
        warnings: Vec::new(),
        schema_version: None,
        id: None,
        capability_level: None,
        declared_endpoints: Vec::new(),
        metrics: Vec::new(),
        reset: None,
    }
}

fn declared_endpoints(manifest: &ProviderManifest) -> Vec<String> {
    let mut kinds = Vec::new();
    for (kind, endpoint) in [
        ("models", &manifest.endpoints.models),
        ("quota", &manifest.endpoints.quota),
        ("usage", &manifest.endpoints.usage),
        ("balance", &manifest.endpoints.balance),
        ("reset", &manifest.endpoints.reset),
    ] {
        if endpoint.is_some() {
            kinds.push(kind.to_owned());
        }
    }
    kinds
}

fn metric_view(snapshot: &QuotaSnapshot) -> PlaygroundMetricView {
    PlaygroundMetricView {
        metric_id: snapshot.metric_id.clone(),
        metric_name: snapshot.metric_name.clone(),
        unit: snapshot
            .unit
            .as_ref()
            .map(|unit| format!("{unit:?}").to_lowercase()),
        currency: snapshot.currency.clone(),
        used: snapshot.used,
        limit: snapshot.limit,
        remaining: snapshot.remaining,
        status: format!("{:?}", snapshot.status).to_lowercase(),
    }
}

fn reset_view(observation: &ResetObservation) -> PlaygroundResetView {
    PlaygroundResetView {
        reset_at: observation.reset_at.clone(),
        window_id: observation.window_id.clone(),
    }
}

/// Parses a sample body with the same functions the adapter uses. Returns
/// normalized snapshots and/or a reset observation; parser failures become
/// `errors` rather than an IPC failure, because a bad sample is expected
/// authoring feedback.
fn parse_sample(
    manifest: &ProviderManifest,
    endpoint: &str,
    body: &str,
    view: &mut ManifestPlaygroundView,
) {
    let config = manifest.to_config();
    let response = ProviderResponse::new(200, body);
    if endpoint == "models" {
        if serde_json::from_str::<serde_json::Value>(body).is_err() {
            view.errors.push("sample body is not valid JSON".to_owned());
        } else {
            view.warnings.push(
                "models has no field mapping to test; only JSON validity was checked".to_owned(),
            );
        }
        return;
    }
    let mappings = config.field_mappings.as_ref();
    if endpoint == "reset" {
        if let Some(reset) = mappings.and_then(|mappings| mappings.reset.as_ref()) {
            match parse_reset_response(&response, reset) {
                Ok(Some(observation)) => {
                    view.reset = Some(reset_view(&observation));
                    return;
                }
                Ok(None) => {
                    view.warnings
                        .push("the declared reset value path resolved to nothing".to_owned());
                    return;
                }
                Err(error) => {
                    view.errors.push(error.to_string());
                    return;
                }
            }
        }
        match parse_openai_compatible_lenient(&response) {
            Ok(Some(set)) => {
                if let Some(reset) = set.snapshots.iter().find_map(|snapshot| {
                    snapshot.reset_at.clone().map(|reset_at| ResetObservation {
                        reset_at,
                        window_id: snapshot.metric_id.clone(),
                    })
                }) {
                    view.reset = Some(reset_view(&reset));
                    view.metrics.extend(set.snapshots.iter().map(metric_view));
                } else {
                    view.warnings.push(
                        "no reset timestamp was recognized; declare mapping.reset for a dedicated endpoint"
                            .to_owned(),
                    );
                }
            }
            Ok(None) => view
                .warnings
                .push("the sample body contains no recognizable reset field".to_owned()),
            Err(error) => view.errors.push(error.to_string()),
        }
        return;
    }
    let parsed = if endpoint == "balance" {
        match parse_balance_response(&response) {
            Ok(Some(set)) => Ok(Some(set)),
            Ok(None) => parse_with_mappings(&response, mappings),
            Err(error) => Err(error),
        }
    } else {
        parse_with_mappings(&response, mappings)
    };
    match parsed {
        Ok(Some(set)) => {
            view.metrics.extend(set.snapshots.iter().map(metric_view));
            if view.metrics.is_empty() {
                view.warnings
                    .push("the sample body mapped to no metrics".to_owned());
            }
        }
        Ok(None) => view.warnings.push(
            "the sample body is valid but no mapped or recognized quota field resolved".to_owned(),
        ),
        Err(error) => view.errors.push(error.to_string()),
    }
}

fn parse_with_mappings(
    response: &ProviderResponse,
    mappings: Option<&FieldMappings>,
) -> crate::domain::provider::ProviderResult<Option<crate::domain::provider::QuotaSnapshotSet>> {
    match mappings {
        Some(mappings) if mappings.has_quota_mappings() => {
            parse_openai_compatible_with_mappings(response, mappings)
        }
        _ => parse_openai_compatible_lenient(response),
    }
}

/// Validates a pasted manifest and evaluates one sample response with its
/// mappings. Local-only; no request is made and nothing is persisted.
#[tauri::command]
pub fn manifest_playground(
    input: ManifestPlaygroundInput,
) -> Result<ManifestPlaygroundView, CommandError> {
    if input.manifest_yaml.len() > MAX_MANIFEST_BYTES {
        return Err(CommandError::InvalidInput(
            "manifest YAML exceeds the 256 KiB playground limit".to_owned(),
        ));
    }
    if input.sample_body.len() > MAX_SAMPLE_BYTES {
        return Err(CommandError::InvalidInput(
            "sample body exceeds the 64 KiB playground limit".to_owned(),
        ));
    }
    if !matches!(
        input.endpoint.as_str(),
        "models" | "quota" | "usage" | "balance" | "reset"
    ) {
        return Err(CommandError::InvalidInput(
            "endpoint must be models, quota, usage, balance, or reset".to_owned(),
        ));
    }

    let mut view = empty_view();
    if let Err(reason) = crate::providers::community::reject_unknown_fields(&input.manifest_yaml) {
        view.errors.push(reason);
        return Ok(view);
    }
    let manifest = match ProviderManifest::from_yaml(&input.manifest_yaml) {
        Ok(manifest) => manifest,
        Err(error) => {
            view.errors.push(format!("invalid manifest: {error}"));
            return Ok(view);
        }
    };
    view.schema_version = Some(manifest.schema_version);
    view.id = Some(manifest.id.clone());
    view.capability_level = manifest.effective_capability_level();
    view.declared_endpoints = declared_endpoints(&manifest);
    if let Err(reason) = manifest.validate_for_registration() {
        view.errors.push(reason);
        return Ok(view);
    }
    if !view
        .declared_endpoints
        .iter()
        .any(|kind| kind == &input.endpoint)
    {
        view.warnings.push(format!(
            "the manifest does not declare a `{}` endpoint; the sample is still parsed",
            input.endpoint
        ));
    }
    view.valid = true;
    parse_sample(&manifest, &input.endpoint, &input.sample_body, &mut view);
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r"
schemaVersion: 1
id: acme-relay
version: 1
name: Acme Relay
auth: [api_key]
baseUrl: https://relay.acme.test/v1
endpoints:
  models: /models
  usage: /usage
  reset: /reset
mapping:
  usage:
    value: data.used
    unit: token
    role: used
  reset:
    value: data.reset_at
    timestampFormat: unix_seconds
    windowId: data.metric_id
capability:
  level: L3
  accountTypes: [api]
  quotaUnits: [token]
  targetWindows: [provider-defined]
";

    fn run(manifest: &str, endpoint: &str, body: &str) -> ManifestPlaygroundView {
        manifest_playground(ManifestPlaygroundInput {
            manifest_yaml: manifest.to_owned(),
            endpoint: endpoint.to_owned(),
            sample_body: body.to_owned(),
        })
        .expect("playground result")
    }

    #[test]
    fn maps_a_sample_body_through_the_declared_mapping() {
        let view = run(MANIFEST, "usage", r#"{"data":{"used":42.5}}"#);
        assert!(view.valid);
        assert_eq!(view.schema_version, Some(1));
        assert_eq!(view.declared_endpoints, vec!["models", "usage", "reset"]);
        assert_eq!(view.metrics.len(), 1);
        assert_eq!(view.metrics[0].metric_id.as_deref(), Some("usage"));
        assert_eq!(view.metrics[0].used, Some(42.5));
        assert_eq!(view.metrics[0].unit.as_deref(), Some("token"));
    }

    #[test]
    fn parses_a_declared_reset_observation() {
        let view = run(
            MANIFEST,
            "reset",
            r#"{"data":{"reset_at":1764000000,"metric_id":"weekly"}}"#,
        );
        assert!(view.valid);
        let reset = view.reset.expect("reset observation");
        assert_eq!(reset.reset_at, "2025-11-24T16:00:00Z");
        assert_eq!(reset.window_id.as_deref(), Some("weekly"));
    }

    #[test]
    fn unknown_fields_and_bad_versions_are_reported_not_parsed() {
        let sneaky = MANIFEST.replace("auth: [api_key]", "auth: [api_key]\nscript: rm -rf /");
        let view = run(&sneaky, "usage", "{}");
        assert!(!view.valid);
        assert!(view.errors[0].contains("manifest.script"));
        assert!(view.metrics.is_empty());

        let future = MANIFEST.replace("schemaVersion: 1", "schemaVersion: 9");
        let view = run(&future, "usage", "{}");
        assert!(!view.valid);
        assert!(view.errors[0].contains("unsupported schemaVersion 9"));
    }

    #[test]
    fn malformed_samples_are_authoring_errors_not_panics() {
        let view = run(MANIFEST, "usage", "not json");
        assert!(view.valid, "the manifest itself is fine");
        assert!(view.metrics.is_empty());
        assert_eq!(view.errors.len(), 1);
        assert!(view.errors[0].contains("JSON"));

        let view = run(MANIFEST, "usage", r#"{"other":1}"#);
        assert!(view.metrics.is_empty());
        assert_ne!(view.warnings.len(), 0);
    }

    #[test]
    fn oversized_inputs_are_refused_by_the_playground_limits() {
        let error = manifest_playground(ManifestPlaygroundInput {
            manifest_yaml: "x".repeat(MAX_MANIFEST_BYTES + 1),
            endpoint: "usage".to_owned(),
            sample_body: "{}".to_owned(),
        })
        .expect_err("oversized manifest");
        assert!(error.to_string().contains("256 KiB"));

        let error = manifest_playground(ManifestPlaygroundInput {
            manifest_yaml: MANIFEST.to_owned(),
            endpoint: "usage".to_owned(),
            sample_body: "x".repeat(MAX_SAMPLE_BYTES + 1),
        })
        .expect_err("oversized sample");
        assert!(error.to_string().contains("64 KiB"));
    }
}
