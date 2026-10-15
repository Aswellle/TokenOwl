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
    ProviderAdapter, ProviderDescriptor, ProviderError, ProviderErrorKind, ProviderResponse,
    ProviderResult, QuotaSnapshot, QuotaSnapshotSet, SnapshotStatus, UsagePoint, UsageRange,
};
use reqwest::Client;
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
    client: Client,
    limits: AdapterLimits,
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
        let client = Client::builder()
            .timeout(limits.timeout)
            .build()
            .map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::ConfigInvalid,
                    format!("unable to initialize HTTP client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            limits,
            config,
        })
    }

    pub fn with_default_limits(config: OpenAiCompatibleConfig) -> ProviderResult<Self> {
        Self::new(AdapterLimits::default(), config)
    }

    /// `GET {base}/v1beta/models` — health and identity in one request.
    async fn probe(&self, credentials: &CredentialStore) -> ProviderResult<ProbeOutcome> {
        let url = endpoint_url(&self.config)?;
        let headers = request_headers(&self.config, credentials)?;
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let response = self.read_response(request).await?;
        let status = response.status;
        match status {
            200..=299 => Ok(ProbeOutcome::Healthy),
            401 | 403 => Ok(ProbeOutcome::Rejected(status)),
            429 => Ok(ProbeOutcome::RateLimited),
            500..=599 => Ok(ProbeOutcome::ServerError(status)),
            _ => Ok(ProbeOutcome::UnknownStatus(status)),
        }
    }

    /// Streams one response under the configured size limit.
    async fn read_response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> ProviderResult<ProviderResponse> {
        let mut response = request.send().await.map_err(map_request_error)?;
        let status = response.status();
        let response_headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|length| length > self.limits.max_response_bytes as u64)
        {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider response exceeds configured size limit",
            )
            .with_status(status.as_u16()));
        }
        let size_limit = self.limits.max_response_bytes;
        let mut body_bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
            if body_bytes.len().saturating_add(chunk.len()) > size_limit {
                return Err(ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    "provider response exceeds configured size limit",
                )
                .with_status(status.as_u16()));
            }
            body_bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(body_bytes).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider response is not valid UTF-8",
            )
            .with_status(status.as_u16())
        })?;
        Ok(ProviderResponse {
            status: status.as_u16(),
            body,
            headers: response_headers_to_map(&response_headers),
        })
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
        match self.probe(credentials).await? {
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
        match self.probe(credentials).await? {
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

fn response_headers_to_map(headers: &HeaderMap) -> std::collections::BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
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

fn map_request_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::new(ProviderErrorKind::Timeout, "request timed out")
    } else if error.is_connect() {
        ProviderError::new(
            ProviderErrorKind::NetworkUnavailable,
            "connection to the provider failed",
        )
    } else {
        ProviderError::new(
            ProviderErrorKind::NetworkUnavailable,
            format!("request failed: {error}"),
        )
    }
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
