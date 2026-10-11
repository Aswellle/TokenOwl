// Built-in provider registration

pub mod openai;
use crate::domain::provider::{
    AuthMethod, OpenAiCompatibleConfig, ProviderCapabilities, ProviderDescriptor, ProviderRegion,
};
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::providers::registry::AdapterRegistry;
use std::sync::Arc;

/// All built-in provider descriptors.
pub const BUILTIN_PROVIDERS: &[(&str, &str)] = &[
    ("openai", "OpenAI"),
    ("anthropic", "Anthropic"),
    ("gemini", "Google Gemini"),
    ("openrouter", "OpenRouter"),
    ("deepseek", "DeepSeek"),
    ("qwen", "Qwen / Alibaba"),
    ("kimi", "Kimi / Moonshot"),
    ("minimax", "MiniMax"),
];

/// Built-in manifests embedded from the repo-level `providers/builtin` directory.
pub const BUILTIN_MANIFESTS: &[(&str, &str)] = &[
    (
        "openai",
        include_str!("../../../../../../providers/builtin/openai.yaml"),
    ),
    (
        "anthropic",
        include_str!("../../../../../../providers/builtin/anthropic.yaml"),
    ),
    (
        "gemini",
        include_str!("../../../../../../providers/builtin/gemini.yaml"),
    ),
    (
        "openrouter",
        include_str!("../../../../../../providers/builtin/openrouter.yaml"),
    ),
    (
        "deepseek",
        include_str!("../../../../../../providers/builtin/deepseek.yaml"),
    ),
    (
        "qwen",
        include_str!("../../../../../../providers/builtin/qwen.yaml"),
    ),
    (
        "kimi",
        include_str!("../../../../../../providers/builtin/kimi.yaml"),
    ),
    (
        "minimax",
        include_str!("../../../../../../providers/builtin/minimax.yaml"),
    ),
];

/// One row of the `docs/09` provider capability matrix catalog.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMatrixRow {
    pub id: String,
    pub name: String,
    /// Capability level of an implemented provider; `None` means planned.
    pub level: Option<String>,
    pub account_types: Vec<String>,
    pub quota_units: Vec<String>,
    pub target_windows: Vec<String>,
    pub priority: String,
    pub implemented: bool,
}

/// Planning-table rows for providers without a built-in manifest yet
/// (`docs/09`: a product plan, not a promise — levels stay unknown until a
/// manifest with fixtures lands).
type PlannedRow = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

const PLANNED_PROVIDERS: &[PlannedRow] = &[
    (
        "new-api",
        "New API",
        "P0",
        &["gateway"],
        &["custom"],
        &["custom"],
    ),
    (
        "one-api",
        "One API",
        "P0",
        &["gateway"],
        &["custom"],
        &["custom"],
    ),
    (
        "litellm",
        "LiteLLM",
        "P0",
        &["gateway"],
        &["custom"],
        &["custom"],
    ),
    (
        "custom-openai",
        "Custom OpenAI",
        "P0",
        &["custom"],
        &["custom"],
        &["custom"],
    ),
    (
        "zhipu",
        "Zhipu / GLM",
        "P1",
        &["subscription", "api"],
        &["quota", "token"],
        &["provider-defined"],
    ),
    (
        "doubao",
        "Doubao / Volcengine",
        "P1",
        &["api", "coding"],
        &["request", "token"],
        &["provider-defined"],
    ),
    (
        "mistral",
        "Mistral",
        "P1",
        &["api", "subscription"],
        &["credit", "currency"],
        &["monthly", "balance"],
    ),
    (
        "groq",
        "Groq",
        "P1",
        &["api"],
        &["request", "token", "rate"],
        &["hourly", "provider-defined"],
    ),
    (
        "xai",
        "xAI",
        "P1",
        &["api"],
        &["currency", "token"],
        &["billing"],
    ),
    ("cohere", "Cohere", "P1", &["api"], &["usage"], &["billing"]),
    (
        "siliconflow",
        "SiliconFlow",
        "P1",
        &["api"],
        &["currency", "token"],
        &["balance", "usage"],
    ),
    (
        "stepfun",
        "StepFun",
        "P1",
        &["api", "subscription"],
        &["credit", "token"],
        &["provider-defined"],
    ),
    (
        "baidu-qianfan",
        "Baidu Qianfan",
        "P1",
        &["api"],
        &["currency", "token"],
        &["billing"],
    ),
    (
        "iflytek",
        "iFlytek",
        "P2",
        &["api"],
        &["quota", "points"],
        &["provider-defined"],
    ),
    (
        "tencent-hunyuan",
        "Tencent Hunyuan",
        "P2",
        &["api"],
        &["token", "credit"],
        &["provider-defined"],
    ),
    (
        "baichuan",
        "Baichuan",
        "P2",
        &["api"],
        &["token", "balance"],
        &["billing"],
    ),
    (
        "01ai",
        "01.AI",
        "P2",
        &["api"],
        &["token", "balance"],
        &["billing"],
    ),
    (
        "xiaomi-mimo",
        "Xiaomi MiMo",
        "P2",
        &["subscription", "api"],
        &["usage", "credits"],
        &["provider-defined"],
    ),
    (
        "360",
        "360",
        "P2",
        &["api"],
        &["credit"],
        &["provider-defined"],
    ),
    (
        "coze",
        "Coze",
        "P2",
        &["api", "platform"],
        &["credits", "usage"],
        &["provider-defined"],
    ),
];

fn split_list(value: &[String]) -> Vec<String> {
    value.to_vec()
}

/// The full `docs/09` capability catalog: implemented providers carry the
/// capability block from their embedded manifest, the rest are planning rows.
///
/// # Panics
///
/// Panics when an embedded manifest stops parsing, which is a build-time bug
/// caught by the consistency tests as well.
pub fn provider_matrix() -> Vec<ProviderMatrixRow> {
    use crate::providers::manifest::ProviderManifest;
    let mut rows: Vec<ProviderMatrixRow> = BUILTIN_MANIFESTS
        .iter()
        .filter_map(|(_id, yaml)| {
            let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
            let capability = manifest.capability.as_ref()?;
            Some(ProviderMatrixRow {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                level: Some(capability.level_id().to_owned()),
                account_types: split_list(&capability.account_types),
                quota_units: split_list(&capability.quota_units),
                target_windows: split_list(&capability.target_windows),
                priority: capability
                    .priority
                    .clone()
                    .unwrap_or_else(|| "P0".to_owned()),
                implemented: true,
            })
        })
        .collect();
    let priority_rank = |priority: &str| match priority {
        "P0" => 0,
        "P1" => 1,
        _ => 2,
    };
    rows.sort_by(|a, b| {
        priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then_with(|| a.name.cmp(&b.name))
    });
    for (id, name, priority, types, units, windows) in PLANNED_PROVIDERS {
        rows.push(ProviderMatrixRow {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            level: None,
            account_types: types.iter().map(|value| (*value).to_owned()).collect(),
            quota_units: units.iter().map(|value| (*value).to_owned()).collect(),
            target_windows: windows.iter().map(|value| (*value).to_owned()).collect(),
            priority: (*priority).to_owned(),
            implemented: false,
        });
    }
    rows
}

/// Setup profile for one built-in provider: everything the account form
/// needs to prefill from the manifest instead of free-typing endpoints.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub level: String,
    pub account_types: Vec<String>,
    pub quota_units: Vec<String>,
    pub target_windows: Vec<String>,
    pub endpoints: crate::providers::manifest::ManifestEndpoints,
}

/// Setup profiles for all embedded manifests, in manifest order.
///
/// # Panics
///
/// Panics when an embedded manifest stops parsing (build-time bug).
pub fn manifest_profiles() -> Vec<ManifestProfile> {
    use crate::providers::manifest::ProviderManifest;
    BUILTIN_MANIFESTS
        .iter()
        .map(|(_id, yaml)| {
            let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
            let capability = manifest.capability.clone().unwrap_or_default();
            ManifestProfile {
                id: manifest.id,
                name: manifest.name,
                base_url: manifest.base_url,
                level: capability.level_id().to_owned(),
                account_types: capability.account_types,
                quota_units: capability.quota_units,
                target_windows: capability.target_windows,
                endpoints: manifest.endpoints,
            }
        })
        .collect()
}

/// Creates a registry with all built-in provider adapters.
pub fn builtin_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    for (id, name) in BUILTIN_PROVIDERS {
        let adapter = builtin_adapter(id, name);
        registry.register(adapter);
    }

    registry
}

/// Creates a built-in adapter by provider ID.
///
/// # Panics
/// Panics if the adapter limits are invalid (zero timeout or zero max response bytes).
pub fn builtin_adapter(id: &str, name: &str) -> Arc<OpenAiCompatibleAdapter> {
    let (base_url, capabilities) = match id {
        "openai" => (
            "https://api.openai.com/v1",
            ProviderCapabilities {
                api_key: true,
                open_ai_compatible: true,
                quota: true,
                usage: true,
                model_catalog: true,
                ..ProviderCapabilities::none()
            },
        ),
        "anthropic" => (
            "https://api.anthropic.com/v1",
            ProviderCapabilities {
                api_key: true,
                model_catalog: true,
                ..ProviderCapabilities::none()
            },
        ),
        "gemini" => (
            "https://generativelanguage.googleapis.com/v1beta",
            ProviderCapabilities {
                api_key: true,
                model_catalog: true,
                ..ProviderCapabilities::none()
            },
        ),
        _ => (
            "https://api.openai.com/v1",
            ProviderCapabilities {
                api_key: true,
                open_ai_compatible: true,
                ..ProviderCapabilities::none()
            },
        ),
    };

    let config = OpenAiCompatibleConfig {
        id: id.to_owned(),
        display_name: name.to_owned(),
        region: ProviderRegion::Global,
        base_url: base_url.to_owned(),
        credential_ref: None,
        auth_method: AuthMethod::ApiKey,
        api_key_header: "Authorization".to_owned(),
        models_endpoint: Some("/models".to_owned()),
        balance_endpoint: if id == "deepseek" {
            Some("/user/balance".to_owned())
        } else {
            None
        },
        usage_endpoint: if id == "openai" || id == "qwen" || id == "kimi" {
            Some("/usage".to_owned())
        } else {
            None
        },
        quota_endpoint: if id == "openai" {
            Some("/quota".to_owned())
        } else {
            None
        },
        reset_endpoint: None,
        docs_url: None,
        default_refresh: std::time::Duration::from_secs(300),
        capabilities,
    };

    Arc::new(OpenAiCompatibleAdapter::with_default_limits(config).expect("valid adapter"))
}

/// Returns all built-in provider descriptors.
pub fn builtin_descriptors() -> Vec<ProviderDescriptor> {
    use crate::domain::provider::ProviderAdapter;
    BUILTIN_PROVIDERS
        .iter()
        .map(|(id, name)| (*builtin_adapter(id, name)).descriptor().clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::ProviderAdapter;

    #[test]
    fn builtin_registry_contains_all_providers() {
        let registry = builtin_registry();
        assert_eq!(registry.len(), BUILTIN_PROVIDERS.len());
    }

    #[test]
    fn builtin_registry_lookup_openai() {
        let registry = builtin_registry();
        assert!(registry.get("openai").is_some());
    }

    #[test]
    fn matrix_lists_full_catalog_with_implemented_levels() {
        let rows = provider_matrix();
        assert_eq!(rows.len(), 28);
        let implemented: Vec<_> = rows.iter().filter(|row| row.implemented).collect();
        assert_eq!(
            implemented.len(),
            8,
            "the eight P0 manifests are implemented"
        );
        let level_of = |id: &str| {
            rows.iter()
                .find(|row| row.id == id)
                .expect("matrix row")
                .level
                .clone()
                .expect("implemented rows carry a level")
        };
        assert_eq!(level_of("openai"), "L4");
        assert_eq!(level_of("anthropic"), "L4");
        assert_eq!(level_of("deepseek"), "L3");
        assert_eq!(level_of("minimax"), "L2");
        let planned = rows
            .iter()
            .find(|row| row.id == "litellm")
            .expect("planned row");
        assert!(!planned.implemented);
        assert!(
            planned.level.is_none(),
            "planned rows carry no invented level"
        );
        assert_eq!(rows.first().map(|row| row.priority.as_str()), Some("P0"));
    }

    #[test]
    fn embedded_manifest_capability_matches_matrix() {
        for (id, yaml) in BUILTIN_MANIFESTS {
            let manifest = crate::providers::manifest::ProviderManifest::from_yaml(yaml)
                .expect("embedded manifest parses");
            let capability = manifest.capability.as_ref().expect("capability block");
            let row = provider_matrix()
                .into_iter()
                .find(|row| row.id == *id)
                .expect("matrix row for builtin id");
            assert_eq!(row.level.as_deref(), Some(capability.level_id()));
            assert_eq!(row.priority, capability.priority.as_deref().unwrap_or("P0"));
        }
    }

    #[test]
    fn builtin_descriptors_match_registry() {
        let descriptors = builtin_descriptors();
        assert_eq!(descriptors.len(), BUILTIN_PROVIDERS.len());
        assert_eq!(descriptors[0].id, "openai");
        assert_eq!(descriptors[1].id, "anthropic");
    }

    #[test]
    fn openai_adapter_has_quota_capability() {
        let adapter = builtin_adapter("openai", "OpenAI");
        let descriptor = adapter.descriptor();
        assert!(descriptor.capabilities.api_key);
        assert!(descriptor.capabilities.open_ai_compatible);
    }
}
