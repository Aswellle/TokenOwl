// Declarative YAML manifest adapter
use crate::domain::provider::{OpenAiCompatibleConfig, ProviderCapabilities, ProviderRegion};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// YAML manifest for OpenAI-compatible providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderManifest {
    pub id: String,
    pub version: u32,
    pub name: String,
    pub category: Option<String>,
    pub region: Option<String>,
    pub auth: Vec<String>,
    pub base_url: String,
    pub api_key_header: Option<String>,
    pub endpoints: ManifestEndpoints,
    pub mapping: Option<ManifestMapping>,
    pub network: Option<ManifestNetwork>,
    /// `docs/09` capability-matrix metadata; optional so older manifests parse.
    #[serde(default)]
    pub capability: Option<ManifestCapability>,
    /// Wire protocol of the provider API: `openai-compatible` (default),
    /// `anthropic`, `google-ai`, … Account creation is gated to protocols the
    /// adapter actually speaks.
    #[serde(default)]
    pub api_format: Option<String>,
    /// Gateways have no fixed base URL; the user supplies one at setup.
    #[serde(default)]
    pub custom_base_url: Option<bool>,
}

/// `docs/09` capability-matrix metadata for one provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCapability {
    /// L4 full quota > L3 usage/balance > L2 health/model > L1 custom.
    pub level: Option<String>,
    pub account_types: Vec<String>,
    pub quota_units: Vec<String>,
    pub target_windows: Vec<String>,
    pub priority: Option<String>,
}

impl ManifestCapability {
    /// Normalized level id (`L1`..`L4`); unknown or missing means L1 custom.
    pub fn level_id(&self) -> &str {
        match self.level.as_deref() {
            Some(level @ ("L4" | "L3" | "L2")) => level,
            _ => "L1",
        }
    }
}

/// Endpoint paths declared in the manifest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEndpoints {
    pub models: Option<String>,
    pub balance: Option<String>,
    pub usage: Option<String>,
    pub quota: Option<String>,
    pub reset: Option<String>,
}

/// Field mapping configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMapping {
    pub balance: Option<ManifestFieldMapping>,
    pub usage: Option<ManifestFieldMapping>,
}

/// Single field mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFieldMapping {
    pub value: String,
    pub unit: Option<String>,
    pub currency: Option<String>,
}

/// Network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNetwork {
    /// Provider-suggested request timeout in milliseconds. Metadata only:
    /// request timeouts come from each adapter's `AdapterLimits`, and this
    /// value is deliberately not a refresh interval.
    pub timeout_ms: Option<u64>,
    /// Parsed for schema compatibility, but the shared transport always
    /// denies redirects. `Some(true)` is rejected at load time with a clear
    /// reason instead of being silently ignored.
    pub follow_redirects: Option<bool>,
}

impl ProviderManifest {
    /// Parse a YAML manifest string.
    ///
    /// # Security
    /// Manifests are trusted input from the application's own configuration.
    /// Untrusted YAML with deeply nested structures could cause a stack overflow;
    /// callers should validate the source before parsing.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Network policy the transport cannot honor, as a user-presentable
    /// reason. Loading boundaries (community packs, embedded manifests)
    /// reject the manifest instead of silently ignoring the flag.
    pub fn unsupported_network_request(&self) -> Option<String> {
        if self
            .network
            .as_ref()
            .and_then(|network| network.follow_redirects)
            == Some(true)
        {
            return Some(
                "followRedirects: true is not supported; redirects are always denied".to_owned(),
            );
        }
        None
    }

    /// Field-level bounds every manifest must satisfy before it can register.
    ///
    /// These are structural limits — id format, name and mapping lengths,
    /// timeout range — not transport policy (see
    /// [`ProviderManifest::unsupported_network_request`]). Both load
    /// boundaries call this so a hand-edited pack fails with the offending
    /// field named instead of registering garbage that breaks later.
    pub fn validate_schema_bounds(&self) -> Result<(), String> {
        let id_valid = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !self.id.starts_with('-')
            && !self.id.ends_with('-');
        if !id_valid {
            return Err(format!(
                "id `{}` must be 1-64 characters of lowercase letters, digits, and hyphens, not starting or ending with a hyphen",
                self.id
            ));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 64 {
            return Err("name must be 1-64 characters".to_owned());
        }
        if let Some(timeout_ms) = self.network.as_ref().and_then(|n| n.timeout_ms) {
            if !(1_000..=120_000).contains(&timeout_ms) {
                return Err(format!(
                    "network.timeoutMs {timeout_ms} must be between 1000 and 120000"
                ));
            }
        }
        if let Some(mapping) = self.mapping.as_ref() {
            for (kind, field) in [("balance", &mapping.balance), ("usage", &mapping.usage)] {
                if let Some(field) = field {
                    if field.value.trim().is_empty()
                        || field.value.chars().count() > 128
                        || field.value.chars().any(char::is_whitespace)
                    {
                        return Err(format!(
                            "mapping.{kind}.value must be 1-128 characters without whitespace"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Convert the manifest to an `OpenAiCompatibleConfig`.
    pub fn to_config(&self) -> OpenAiCompatibleConfig {
        let region = self
            .region
            .as_deref()
            .and_then(|r| match r {
                "global" => Some(ProviderRegion::Global),
                "north_america" => Some(ProviderRegion::NorthAmerica),
                "europe" => Some(ProviderRegion::Europe),
                "asia_pacific" => Some(ProviderRegion::AsiaPacific),
                "china" => Some(ProviderRegion::China),
                _ => None,
            })
            .unwrap_or_default();

        let mut capabilities = ProviderCapabilities::none();
        capabilities.api_key = self.auth.iter().any(|a| a == "api_key");
        capabilities.open_ai_compatible = true;
        capabilities.model_catalog = self.endpoints.models.is_some();
        capabilities.balance = self.endpoints.balance.is_some();
        capabilities.usage = self.endpoints.usage.is_some();
        capabilities.quota = self.endpoints.quota.is_some();
        capabilities.reset_at = self.endpoints.reset.is_some();

        // `network.timeout_ms` historically leaked into `default_refresh`
        // here, turning a 10,000 ms request-timeout wish into a 10 s refresh
        // cadence. The two are different concerns: the refresh interval is
        // the scheduler default, and request timeouts belong to the shared
        // transport's per-adapter limits.
        let default_refresh = Duration::from_secs(300);

        let field_mappings =
            self.mapping
                .as_ref()
                .map(|mapping| crate::domain::provider::FieldMappings {
                    balance: mapping.balance.as_ref().map(|field| {
                        crate::domain::provider::FieldMapping {
                            value: field.value.clone(),
                            unit: field.unit.clone(),
                            currency: field.currency.clone(),
                        }
                    }),
                    usage: mapping.usage.as_ref().map(|field| {
                        crate::domain::provider::FieldMapping {
                            value: field.value.clone(),
                            unit: field.unit.clone(),
                            currency: field.currency.clone(),
                        }
                    }),
                });
        OpenAiCompatibleConfig {
            id: self.id.clone(),
            display_name: self.name.clone(),
            region,
            base_url: self.base_url.clone(),
            credential_ref: None,
            auth_method: crate::domain::provider::AuthMethod::ApiKey,
            api_key_header: self
                .api_key_header
                .clone()
                .unwrap_or_else(|| "Authorization".to_owned()),
            models_endpoint: self.endpoints.models.clone(),
            balance_endpoint: self.endpoints.balance.clone(),
            usage_endpoint: self.endpoints.usage.clone(),
            quota_endpoint: self.endpoints.quota.clone(),
            reset_endpoint: self.endpoints.reset.clone(),
            docs_url: None,
            default_refresh,
            capabilities,
            field_mappings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENAI_MANIFEST: &str = r"
id: openai
version: 1
name: OpenAI
category: official
region: global
auth:
  - api_key
baseUrl: https://api.openai.com/v1
apiKeyHeader: Authorization
endpoints:
  models: /v1/models
  usage: /v1/usage
  quota: /v1/quota
network:
  timeout_ms: 10000
  followRedirects: false
capability:
  level: L4
  accountTypes: [api, usage]
  quotaUnits: [currency, token]
  targetWindows: [monthly, custom]
  priority: P0
";

    #[test]
    fn parse_openai_manifest() {
        let manifest = ProviderManifest::from_yaml(OPENAI_MANIFEST).expect("valid YAML");
        assert_eq!(manifest.id, "openai");
        assert_eq!(manifest.name, "OpenAI");
        assert_eq!(manifest.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn manifest_to_config_preserves_capabilities() {
        let manifest = ProviderManifest::from_yaml(OPENAI_MANIFEST).expect("valid YAML");
        let config = manifest.to_config();
        assert!(config.capabilities.api_key);
        assert!(config.capabilities.open_ai_compatible);
        assert!(config.capabilities.model_catalog);
        assert!(config.capabilities.usage);
        assert!(config.capabilities.quota);
    }

    #[test]
    fn network_timeout_ms_is_not_a_refresh_interval() {
        let manifest = ProviderManifest::from_yaml(OPENAI_MANIFEST).expect("valid YAML");
        let config = manifest.to_config();
        // `network.timeout_ms: 10000` is a request-timeout wish. It must not
        // leak into the refresh cadence, which stays the 300 s scheduler
        // default; a 10 s "refresh interval" was the historical bug.
        assert_eq!(config.default_refresh, Duration::from_secs(300));
    }

    #[test]
    fn manifest_without_network_keeps_the_default_refresh() {
        let yaml = r"
id: minimal-refresh
version: 1
name: Minimal Refresh Provider
auth:
  - api_key
baseUrl: https://example.com/v1
endpoints: {}
";
        let config = ProviderManifest::from_yaml(yaml)
            .expect("valid YAML")
            .to_config();
        assert_eq!(config.default_refresh, Duration::from_secs(300));
    }

    #[test]
    fn follow_redirects_true_is_reported_as_unsupported() {
        let yaml = r"
id: redirecting
version: 1
name: Redirecting Provider
auth:
  - api_key
baseUrl: https://example.com/v1
endpoints: {}
network:
  followRedirects: true
";
        let manifest = ProviderManifest::from_yaml(yaml).expect("valid YAML");
        let reason = manifest
            .unsupported_network_request()
            .expect("followRedirects: true must not be silently accepted");
        assert!(reason.contains("followRedirects"));

        let allowed = ProviderManifest::from_yaml(OPENAI_MANIFEST).expect("valid YAML");
        assert_eq!(allowed.unsupported_network_request(), None);
    }

    fn bounds_yaml(id: &str, name: &str, timeout_ms: u64, mapping_value: &str) -> String {
        format!(
            "\
id: {id}
version: 1
name: {name}
auth:
  - api_key
baseUrl: https://relay.example.com/v1
endpoints:
  models: /models
network:
  timeoutMs: {timeout_ms}
mapping:
  balance:
    value: \"{mapping_value}\"
"
        )
    }

    #[test]
    fn schema_bounds_accept_a_well_formed_manifest() {
        let manifest = ProviderManifest::from_yaml(&bounds_yaml(
            "acme-relay",
            "Acme Relay",
            15_000,
            "data.balance",
        ))
        .expect("valid YAML");
        assert_eq!(manifest.validate_schema_bounds(), Ok(()));
    }

    /// ids become part of provider ids and capability keys; they must stay
    /// lowercase, short, and hyphen-separated.
    #[test]
    fn schema_bounds_reject_malformed_ids() {
        for id in [
            "",
            "UPPER",
            "has_underscore",
            "has space",
            "-leading",
            "trailing-",
            &"a".repeat(65),
        ] {
            let manifest =
                ProviderManifest::from_yaml(&bounds_yaml(id, "Acme", 15_000, "data.balance"))
                    .expect("valid YAML");
            assert!(
                manifest.validate_schema_bounds().is_err(),
                "`{id}` must fail the id bound"
            );
        }
    }

    #[test]
    fn schema_bounds_reject_oversized_names() {
        let long_name = "n".repeat(65);
        let manifest =
            ProviderManifest::from_yaml(&bounds_yaml("acme", &long_name, 15_000, "data.balance"))
                .expect("valid YAML");
        assert!(manifest.validate_schema_bounds().is_err());
    }

    /// `timeoutMs` is metadata today, but a nonsense value (zero, or hours)
    /// signals a miswritten pack and must be caught at load, not deferred to
    /// some future consumer.
    #[test]
    fn schema_bounds_reject_out_of_range_timeouts() {
        for timeout_ms in [0, 500, 120_001] {
            let manifest = ProviderManifest::from_yaml(&bounds_yaml(
                "acme",
                "Acme",
                timeout_ms,
                "data.balance",
            ))
            .expect("valid YAML");
            assert!(
                manifest.validate_schema_bounds().is_err(),
                "timeoutMs {timeout_ms} must be rejected"
            );
        }
    }

    #[test]
    fn schema_bounds_reject_unusable_mapping_paths() {
        for mapping_value in ["", &"x".repeat(129), "has space"] {
            let manifest =
                ProviderManifest::from_yaml(&bounds_yaml("acme", "Acme", 15_000, mapping_value))
                    .expect("valid YAML");
            assert!(
                manifest.validate_schema_bounds().is_err(),
                "mapping `{mapping_value}` must be rejected"
            );
        }
    }

    /// Embedded manifests are authored alongside the validator; if one stops
    /// passing, that is a build-time bug, not a skip.
    #[test]
    fn embedded_manifests_pass_the_schema_bounds() {
        for (id, yaml) in crate::providers::builtin::BUILTIN_MANIFESTS {
            let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
            assert!(
                manifest.validate_schema_bounds().is_ok(),
                "embedded manifest `{id}` fails schema bounds"
            );
        }
    }

    #[test]
    fn manifest_without_optional_endpoints() {
        let yaml = r"
id: minimal
version: 1
name: Minimal Provider
auth:
  - api_key
baseUrl: https://example.com/v1
endpoints: {}
";
        let manifest = ProviderManifest::from_yaml(yaml).expect("valid YAML");
        let config = manifest.to_config();
        assert!(!config.capabilities.model_catalog);
        assert!(!config.capabilities.balance);
        assert!(!config.capabilities.usage);
        assert!(!config.capabilities.quota);
    }

    #[test]
    fn invalid_yaml_is_rejected() {
        let result = ProviderManifest::from_yaml("invalid: [yaml");
        assert!(result.is_err());
    }
}
