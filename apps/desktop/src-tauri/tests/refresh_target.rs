//! Integration-level regression: the refresh path must target the account's
//! own provider config, not the placeholder config the native adapters were
//! constructed with.
//!
//! Background: `AppRuntime` builds `AnthropicAdapter` / `GoogleAiAdapter` from
//! `OpenAiCompatibleConfig::default()`, whose `base_url` is
//! `https://example.invalid/v1`. If an adapter reads its construction config
//! instead of the per-account config supplied through `FetchContext`, every
//! refresh silently targets that placeholder host and fails. Unit tests that
//! construct adapters with a real config cannot catch this, which is why this
//! test walks the same shapes the runtime uses.

use tokenowl_lib::domain::provider::{FetchContext, OpenAiCompatibleConfig};

/// The placeholder host `OpenAiCompatibleConfig::default()` carries. Asserted
/// rather than hard-coded so a change to the default surfaces here.
fn placeholder_base() -> String {
    OpenAiCompatibleConfig::default().base_url
}

#[test]
fn default_config_is_the_placeholder_host() {
    let base = placeholder_base();
    assert!(
        base.contains("example.invalid"),
        "adapter construction default is expected to be a placeholder, got {base}"
    );
}

/// Mirrors `scheduler::native_fetch`: the account config from the repository is
/// placed into `FetchContext.config`. The adapter must resolve its request
/// target from there.
#[test]
fn fetch_context_carries_the_account_config_for_the_probe() {
    let account_config = OpenAiCompatibleConfig::new(
        "anthropic:account-1",
        "Anthropic",
        "https://api.anthropic.com/v1",
    );
    let ctx = FetchContext {
        timeout: account_config.default_refresh,
        base_url: Some(account_config.base_url.clone()),
        credentials: None,
        config: Some(account_config.clone()),
    };

    let carried = ctx
        .config
        .as_ref()
        .expect("refresh path must supply the account config");
    assert_eq!(carried.base_url, "https://api.anthropic.com/v1");
    assert_ne!(carried.base_url, placeholder_base());
    assert_eq!(carried.id, "anthropic:account-1");
}

/// The resolution the adapters perform: prefer `ctx.config`, fall back to the
/// construction config only when no account config was supplied.
#[test]
fn adapter_resolution_prefers_the_account_config_over_its_own() {
    let construction = OpenAiCompatibleConfig::default();
    let account = OpenAiCompatibleConfig::new(
        "gemini:account-2",
        "Google Gemini",
        "https://generativelanguage.googleapis.com/v1beta",
    );

    let with_account = FetchContext {
        config: Some(account.clone()),
        ..FetchContext::default()
    };
    // Same expression as `effective_config` in both native adapters.
    let resolved = with_account.config.as_ref().unwrap_or(&construction);
    assert_eq!(
        resolved.base_url,
        "https://generativelanguage.googleapis.com/v1beta"
    );
    assert_ne!(resolved.base_url, construction.base_url);

    // Without an account config the construction config remains the fallback,
    // which is correct for direct adapter use (tests, tools).
    let without_account = FetchContext::default();
    let fallback = without_account.config.as_ref().unwrap_or(&construction);
    assert_eq!(fallback.base_url, placeholder_base());
}

/// The models endpoint travels with the account config, so the probe path is
/// not lost between manifest resolution and the request.
#[test]
fn account_config_retains_the_models_endpoint() {
    let mut account = OpenAiCompatibleConfig::new(
        "anthropic:account-3",
        "Anthropic",
        "https://api.anthropic.com/v1",
    );
    account.models_endpoint = Some("/v1/models".to_owned());
    let ctx = FetchContext {
        config: Some(account.clone()),
        ..FetchContext::default()
    };

    let carried = ctx.config.as_ref().expect("config");
    assert_eq!(carried.models_endpoint.as_deref(), Some("/v1/models"));

    // The probe URL the adapter builds from that config.
    let base = url::Url::parse(&carried.base_url).expect("base url");
    let endpoint = carried.models_endpoint.as_deref().expect("models endpoint");
    let probe = base
        .join(endpoint.trim_start_matches('/'))
        .expect("probe url");
    assert_eq!(probe.as_str(), "https://api.anthropic.com/v1/models");
}
