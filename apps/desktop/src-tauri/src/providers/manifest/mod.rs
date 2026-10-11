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
    pub timeout_ms: Option<u64>,
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

        let default_refresh = self
            .network
            .as_ref()
            .and_then(|n| n.timeout_ms)
            .map_or_else(|| Duration::from_secs(300), Duration::from_millis);

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
