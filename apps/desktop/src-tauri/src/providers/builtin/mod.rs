// Built-in provider registration

pub mod anthropic;
pub mod openai;
use crate::domain::provider::ProviderDescriptor;
use crate::providers::builtin::anthropic::AnthropicAdapter;
use crate::providers::builtin::openai::OpenAiCompatibleAdapter;
use crate::providers::registry::AdapterRegistry;

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
    (
        "zhipu",
        include_str!("../../../../../../providers/builtin/zhipu.yaml"),
    ),
    (
        "doubao",
        include_str!("../../../../../../providers/builtin/doubao.yaml"),
    ),
    (
        "mistral",
        include_str!("../../../../../../providers/builtin/mistral.yaml"),
    ),
    (
        "groq",
        include_str!("../../../../../../providers/builtin/groq.yaml"),
    ),
    (
        "xai",
        include_str!("../../../../../../providers/builtin/xai.yaml"),
    ),
    (
        "siliconflow",
        include_str!("../../../../../../providers/builtin/siliconflow.yaml"),
    ),
    (
        "stepfun",
        include_str!("../../../../../../providers/builtin/stepfun.yaml"),
    ),
    (
        "baichuan",
        include_str!("../../../../../../providers/builtin/baichuan.yaml"),
    ),
    (
        "01ai",
        include_str!("../../../../../../providers/builtin/01ai.yaml"),
    ),
    (
        "360",
        include_str!("../../../../../../providers/builtin/360.yaml"),
    ),
    (
        "new-api",
        include_str!("../../../../../../providers/builtin/new-api.yaml"),
    ),
    (
        "one-api",
        include_str!("../../../../../../providers/builtin/one-api.yaml"),
    ),
    (
        "litellm",
        include_str!("../../../../../../providers/builtin/litellm.yaml"),
    ),
];

/// One row of the `docs/09` provider capability matrix catalog.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMatrixRow {
    pub id: String,
    pub name: String,
    pub api_format: String,
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
        "custom-openai",
        "Custom OpenAI",
        "P0",
        &["custom"],
        &["custom"],
        &["custom"],
    ),
    ("cohere", "Cohere", "P1", &["api"], &["usage"], &["billing"]),
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
        "xiaomi-mimo",
        "Xiaomi MiMo",
        "P2",
        &["subscription", "api"],
        &["usage", "credits"],
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
                api_format: manifest
                    .api_format
                    .clone()
                    .unwrap_or_else(|| "openai-compatible".to_owned()),
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
            api_format: "openai-compatible".to_owned(),
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
    pub api_format: String,
    pub custom_base_url: bool,
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
                api_format: manifest
                    .api_format
                    .clone()
                    .unwrap_or_else(|| "openai-compatible".to_owned()),
                custom_base_url: manifest.custom_base_url.unwrap_or(false),
                level: capability.level_id().to_owned(),
                account_types: capability.account_types,
                quota_units: capability.quota_units,
                target_windows: capability.target_windows,
                endpoints: manifest.endpoints,
            }
        })
        .collect()
}

/// Creates a registry with one adapter per embedded manifest. Adapter
/// configs (base URL, endpoints, capabilities) come from the manifests, so
/// adding a provider YAML is all it takes to extend the registry.
///
/// # Panics
///
/// Panics when an embedded manifest fails to parse or validate — both are
/// build-time bugs also covered by tests.
pub fn builtin_registry() -> AdapterRegistry {
    use crate::providers::manifest::ProviderManifest;
    let mut registry = AdapterRegistry::new();
    for (_id, yaml) in BUILTIN_MANIFESTS {
        let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
        let config = manifest.to_config();
        // Native wire protocols route to their own adapters; everything else
        // rides the OpenAI-compatible transport.
        let adapter: std::sync::Arc<dyn crate::domain::provider::ProviderAdapter> =
            match manifest.api_format.as_deref() {
                Some("anthropic") => std::sync::Arc::new(
                    AnthropicAdapter::with_default_limits(config)
                        .expect("anthropic manifest yields a valid adapter config"),
                ),
                _ => std::sync::Arc::new(
                    OpenAiCompatibleAdapter::with_default_limits(config)
                        .expect("embedded manifest yields a valid adapter config"),
                ),
            };
        registry.register(adapter);
    }
    registry
}

/// Returns all built-in provider descriptors.
pub fn builtin_descriptors() -> Vec<ProviderDescriptor> {
    builtin_registry()
        .all()
        .iter()
        .map(|adapter| adapter.descriptor().clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_providers() {
        let registry = builtin_registry();
        assert_eq!(registry.len(), BUILTIN_MANIFESTS.len());
        assert_eq!(registry.len(), 21, "eight P0 + thirteen P1 manifests");
    }

    #[test]
    fn builtin_registry_routes_anthropic_to_native_adapter() {
        let registry = builtin_registry();
        let adapter = registry.get("anthropic").expect("anthropic adapter");
        let caps = adapter.descriptor().capabilities;
        // The native adapter exposes the L2 surface only: models catalog,
        // no invented quota/usage/balance endpoints.
        assert!(caps.model_catalog);
        assert!(!caps.quota);
        assert!(!caps.usage);
        assert!(!caps.balance);
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
            21,
            "eight P0 + thirteen P1 manifests are implemented"
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
        assert_eq!(level_of("anthropic"), "L2");
        assert_eq!(level_of("deepseek"), "L3");
        assert_eq!(level_of("minimax"), "L2");
        let planned = rows
            .iter()
            .find(|row| row.id == "cohere")
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
        assert_eq!(descriptors.len(), BUILTIN_MANIFESTS.len());
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.id == "openai")
        );
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.id == "deepseek")
        );
    }

    #[test]
    fn openai_adapter_has_quota_capability() {
        let registry = builtin_registry();
        let adapter = registry.get("openai").expect("openai adapter");
        let descriptor = adapter.descriptor();
        assert!(descriptor.capabilities.api_key);
        assert!(descriptor.capabilities.open_ai_compatible);
    }
}
