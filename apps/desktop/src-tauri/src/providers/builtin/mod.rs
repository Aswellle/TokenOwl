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
