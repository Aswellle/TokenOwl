// Google AI native-protocol adapter
//! Google AI (Gemini) adapter — L2: Health/Model only.
//!
//! The public Gemini API publishes no account-quota endpoint and documents
//! no rate-limit response headers (limits are per project and reviewed in
//! AI Studio), so this adapter reports exactly one observable surface:
//! `GET /v1beta/models` doubles as health and identity. The API key rides
//! the `x-goog-api-key` header instead of the documented `?key=` query
//! parameter so the secret never lands in URLs or logs. Unknown stays
//! unknown — no quota values are invented (docs/09).

use crate::credentials::{CredentialError, CredentialStore};
use crate::domain::provider::{
    AccountContext, AccountIdentity, FetchContext, HealthStatus, OpenAiCompatibleConfig,
    ProviderAdapter, ProviderDescriptor, ProviderError, ProviderErrorKind, ProviderResult,
    QuotaSnapshot, QuotaSnapshotSet, SnapshotStatus, UsagePoint, UsageRange,
};
use crate::networking::{HttpClient, HttpClientConfig};
use reqwest::header::{HeaderMap, HeaderValue};
use std::time::Duration;
use url::Url;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 512 * 1024;
/// Documented model-list path on the generativelanguage host.
const MODELS_PATH: &str = "v1beta/models";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdapterLimits {
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for AdapterLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GoogleAiAdapter {
    client: HttpClient,
    config: OpenAiCompatibleConfig,
}

impl GoogleAiAdapter {
    pub fn new(limits: AdapterLimits, config: OpenAiCompatibleConfig) -> ProviderResult<Self> {
        if limits.timeout.is_zero() || limits.max_response_bytes == 0 {
            return Err(ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "HTTP timeout and response limit must be positive",
            ));
        }
        // Shared transport authority: redirects denied, no implicit proxy,
        // streaming size cap. Only timeout and cap size vary per adapter.
        let client = HttpClient::new(HttpClientConfig::from_adapter_limits(
            limits.timeout,
            limits.max_response_bytes,
        ))?;
        Ok(Self { client, config })
    }

    pub fn with_default_limits(config: OpenAiCompatibleConfig) -> ProviderResult<Self> {
        Self::new(AdapterLimits::default(), config)
    }

    /// Resolves the config a request must use: the account's own config when
    /// the caller supplied one, falling back to the construction config only
    /// for direct adapter use (tests, tools). `AppRuntime` constructs these
    /// adapters from `OpenAiCompatibleConfig::default()`, whose base URL is a
    /// placeholder host, so the fallback must never win on the refresh path.
    fn effective_config<'a>(&'a self, ctx: &'a FetchContext) -> &'a OpenAiCompatibleConfig {
        ctx.config.as_ref().unwrap_or(&self.config)
    }

    /// `GET {base}/v1beta/models` — health and identity in one request.
    async fn probe(
        &self,
        config: &OpenAiCompatibleConfig,
        credentials: &CredentialStore,
    ) -> ProviderResult<ProbeOutcome> {
        let url = endpoint_url(config)?;
        let headers = request_headers(config, credentials)?;
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let response = self.client.send_capped(request).await?;
        let status = response.status;
        match status {
            200..=299 => Ok(ProbeOutcome::Healthy),
            401 | 403 => Ok(ProbeOutcome::Rejected(status)),
            429 => Ok(ProbeOutcome::RateLimited),
            500..=599 => Ok(ProbeOutcome::ServerError(status)),
            _ => Ok(ProbeOutcome::UnknownStatus(status)),
        }
    }
}

enum ProbeOutcome {
    Healthy,
    Rejected(u16),
    RateLimited,
    ServerError(u16),
    UnknownStatus(u16),
}

#[async_trait::async_trait]
impl ProviderAdapter for GoogleAiAdapter {
    fn descriptor(&self) -> ProviderDescriptor {
        let mut descriptor = self.config.descriptor();
        descriptor.capabilities.model_catalog = true;
        descriptor.capabilities.quota = false;
        descriptor.capabilities.usage = false;
        descriptor.capabilities.balance = false;
        descriptor.capabilities.reset_at = false;
        descriptor
    }

    async fn validate_account(&self, _account: &AccountContext) -> ProviderResult<AccountIdentity> {
        Err(ProviderError::new(
            ProviderErrorKind::UnsupportedCapability,
            "validate_account runs through fetch_quota on the Google AI adapter",
        ))
    }

    /// Health-only by design: the models probe decides the snapshot surface.
    /// A healthy probe with no observable quota values answers with one
    /// explicitly unknown snapshot — never a zero.
    async fn fetch_quota(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<QuotaSnapshotSet> {
        let credentials = ctx.credentials.as_ref().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::CredentialError,
                "Google AI adapter requires a credential store",
            )
        })?;
        match self.probe(self.effective_config(ctx), credentials).await? {
            ProbeOutcome::Healthy => {
                let mut unknown = QuotaSnapshot::unknown();
                unknown.account_id = Some(account.account_id.clone());
                unknown.metric_name = Some("Google AI API".to_owned());
                unknown.status = SnapshotStatus::Unavailable;
                Ok(QuotaSnapshotSet::new(vec![unknown]))
            }
            ProbeOutcome::Rejected(status) => Err(auth_error(status)),
            ProbeOutcome::RateLimited => Err(ProviderError::new(
                ProviderErrorKind::RateLimited,
                "Google AI rate-limited the models probe",
            )
            .with_status(429)),
            ProbeOutcome::ServerError(status) => Err(ProviderError::new(
                ProviderErrorKind::ProviderUnavailable,
                "Google AI answered with a server error",
            )
            .with_status(status)),
            ProbeOutcome::UnknownStatus(status) => Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "Google AI answered with an unexpected status",
            )
            .with_status(status)),
        }
    }

    async fn fetch_usage(
        &self,
        _account: &AccountContext,
        _range: UsageRange,
        _ctx: &FetchContext,
    ) -> ProviderResult<Vec<UsagePoint>> {
        // No documented per-key usage endpoint; history stays client-side.
        Ok(Vec::new())
    }

    async fn health_check(
        &self,
        _account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<HealthStatus> {
        let credentials = ctx.credentials.as_ref().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::CredentialError,
                "Google AI adapter requires a credential store",
            )
        })?;
        match self.probe(self.effective_config(ctx), credentials).await? {
            ProbeOutcome::Healthy => Ok(HealthStatus::Healthy),
            ProbeOutcome::Rejected(status) => Err(auth_error(status)),
            ProbeOutcome::RateLimited => Ok(HealthStatus::Degraded),
            ProbeOutcome::ServerError(_) => Ok(HealthStatus::Unhealthy),
            ProbeOutcome::UnknownStatus(_) => Ok(HealthStatus::Unknown),
        }
    }
}

fn endpoint_url(config: &OpenAiCompatibleConfig) -> ProviderResult<Url> {
    let base = Url::parse(&config.base_url).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "base URL is not a valid absolute URL",
        )
    })?;
    base.join(MODELS_PATH).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "models path could not be joined onto the base URL",
        )
    })
}

fn request_headers(
    config: &OpenAiCompatibleConfig,
    credentials: &CredentialStore,
) -> ProviderResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    let Some(reference) = config.credential_ref.as_deref() else {
        return Err(ProviderError::new(
            ProviderErrorKind::CredentialError,
            "Google AI adapter requires a credential reference",
        ));
    };
    let secret = credentials
        .resolve(reference)
        .map_err(map_credential_error)?;
    headers.insert(
        HeaderName::from_static("x-goog-api-key"),
        HeaderValue::from_str(secret.as_str()).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "credential value is not a valid header value",
            )
        })?,
    );
    Ok(headers)
}

use reqwest::header::HeaderName;

fn auth_error(status: u16) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::AuthExpired,
        "Google AI rejected the API key",
    )
    .with_status(status)
}

fn map_credential_error(error: CredentialError) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::CredentialError,
        format!("credential resolution failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_url_joins_the_documented_models_path() {
        let mut config = OpenAiCompatibleConfig::new(
            "gemini",
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
        );
        config.models_endpoint = Some("/v1beta/models".to_owned());
        let url = endpoint_url(&config).expect("url");
        assert_eq!(
            url.as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
    }

    #[test]
    fn effective_config_prefers_the_account_config_over_the_placeholder() {
        // Same contract as the Anthropic adapter: AppRuntime builds this from
        // the placeholder default, so the fetch context must win.
        let placeholder = OpenAiCompatibleConfig::default();
        assert!(placeholder.base_url.contains("example.invalid"));
        let adapter = GoogleAiAdapter::with_default_limits(placeholder.clone()).expect("adapter");

        let account = OpenAiCompatibleConfig::new(
            "gemini:account-1",
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
        );
        let ctx = FetchContext {
            config: Some(account),
            ..FetchContext::default()
        };
        assert_eq!(
            adapter.effective_config(&ctx).base_url,
            "https://generativelanguage.googleapis.com/v1beta"
        );

        let bare = FetchContext::default();
        assert_eq!(
            adapter.effective_config(&bare).base_url,
            placeholder.base_url
        );
    }

    #[test]
    fn probe_url_follows_the_effective_config() {
        let adapter = GoogleAiAdapter::with_default_limits(OpenAiCompatibleConfig::default())
            .expect("adapter");
        let account = OpenAiCompatibleConfig::new(
            "gemini:account-2",
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
        );
        let ctx = FetchContext {
            config: Some(account),
            ..FetchContext::default()
        };
        let url = endpoint_url(adapter.effective_config(&ctx)).expect("probe url");
        assert_eq!(
            url.as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        assert!(!url.as_str().contains("example.invalid"));
    }

    #[test]
    fn descriptor_reports_health_only_capabilities() {
        let mut config = OpenAiCompatibleConfig::new(
            "gemini",
            "Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
        );
        config.models_endpoint = Some("/v1beta/models".to_owned());
        let adapter = GoogleAiAdapter::with_default_limits(config).expect("adapter");
        let capabilities = adapter.descriptor().capabilities;
        assert!(capabilities.model_catalog);
        assert!(!capabilities.quota);
        assert!(!capabilities.usage);
        assert!(!capabilities.balance);
        assert!(!capabilities.reset_at);
    }
}
