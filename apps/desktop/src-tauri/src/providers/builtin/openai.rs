// OpenAI-compatible provider adapter
// Original content moved from src-tauri/src/openai_adapter.rs
//! Generic OpenAI-compatible HTTP adapter.
//!
//! The adapter owns transport concerns and hands the raw body directly to the existing
//! provider parser. It never logs or persists that body.

use crate::credentials::{CredentialError, CredentialStore};
use crate::domain::provider::{
    AccountContext, AccountIdentity, AuthMethod, FetchContext, HealthStatus,
    OpenAiCompatibleConfig, ProviderAdapter, ProviderDescriptor, ProviderError, ProviderErrorKind,
    ProviderResponse, ProviderResult, QuotaSnapshotSet, UsagePoint, UsageRange,
};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

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

#[derive(Clone, Debug, PartialEq)]
pub struct FetchResult {
    pub snapshots: QuotaSnapshotSet,
    pub observed_at: u64,
}

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleAdapter {
    client: Client,
    limits: AdapterLimits,
    config: OpenAiCompatibleConfig,
}

impl OpenAiCompatibleAdapter {
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

    pub fn limits(&self) -> AdapterLimits {
        self.limits
    }

    /// Fetches the configured quota/usage/balance endpoint and parses only normalized data.
    pub async fn fetch(
        &self,
        config: &OpenAiCompatibleConfig,
        account_id: &str,
        credentials: &CredentialStore,
    ) -> ProviderResult<FetchResult> {
        config.validate()?;
        if account_id.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "account id is required",
            ));
        }
        let endpoint = config
            .quota_endpoint
            .as_deref()
            .or(config.usage_endpoint.as_deref())
            .or(config.balance_endpoint.as_deref())
            .ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::UnsupportedCapability,
                    "provider has no quota, usage, or balance endpoint",
                )
            })?;
        let url = endpoint_url(config, endpoint)?;
        let headers = request_headers(config, credentials)?;
        let mut request = self.client.get(url).headers(headers);
        // Do not send a body or query containing secrets. Some compatible providers
        // require the account/model in metadata; those are configured as URL paths.
        request = request.header("accept", "application/json");
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
        let declared_length = response
            .content_length()
            .unwrap_or(0)
            .min(u64::try_from(size_limit).unwrap_or(u64::MAX));
        let mut body_bytes =
            Vec::with_capacity(usize::try_from(declared_length).unwrap_or(size_limit));
        while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
            if body_bytes.len().saturating_add(chunk.len()) > self.limits.max_response_bytes {
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
        let parser_response = ProviderResponse {
            status: status.as_u16(),
            body,
            headers: response_headers_to_map(&response_headers),
        };
        let mut snapshots = config.parse_response(&parser_response)?;
        for snapshot in &mut snapshots.snapshots {
            if snapshot.account_id.is_none() {
                snapshot.account_id = Some(account_id.to_owned());
            }
        }
        let observed_at = unix_seconds();
        Ok(FetchResult {
            snapshots,
            observed_at,
        })
    }
}

fn endpoint_url(config: &OpenAiCompatibleConfig, endpoint: &str) -> ProviderResult<Url> {
    let base = Url::parse(config.base_url.trim()).map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            format!("base_url is not a valid URL: {error}"),
        )
    })?;
    if !matches!(base.scheme(), "http" | "https")
        || base.host_str().is_none()
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "base_url must be an http(s) URL without credentials, query, or fragment",
        ));
    }
    if !endpoint.starts_with('/')
        || endpoint.starts_with("//")
        || endpoint.contains('\\')
        || endpoint.contains("..")
    {
        return Err(ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "endpoint paths must be relative paths beginning with '/'",
        ));
    }
    let joined = format!(
        "{}/{}",
        config.base_url.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    );
    Url::parse(&joined).map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            format!("endpoint URL is invalid: {error}"),
        )
    })
}

fn request_headers(
    config: &OpenAiCompatibleConfig,
    credentials: &CredentialStore,
) -> ProviderResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    let Some(reference) = config.credential_ref.as_deref() else {
        if !matches!(config.auth_method, AuthMethod::None) {
            return Err(ProviderError::new(
                ProviderErrorKind::CredentialError,
                "provider requires a credential reference",
            ));
        }
        return Ok(headers);
    };
    let secret = credentials
        .resolve(reference)
        .map_err(map_credential_error)?;
    if matches!(config.auth_method, AuthMethod::None) {
        return Ok(headers);
    }
    let header_name = HeaderName::from_bytes(config.api_key_header.as_bytes()).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "api_key_header is not a valid HTTP header name",
        )
    })?;
    let value = match config.auth_method {
        AuthMethod::ApiKey => {
            if config.api_key_header.eq_ignore_ascii_case("authorization") {
                format!("Bearer {}", secret.as_str())
            } else {
                secret.as_str().to_owned()
            }
        }
        AuthMethod::Bearer | AuthMethod::OAuth => format!("Bearer {}", secret.as_str()),
        AuthMethod::Basic => format!("Basic {}", secret.as_str()),
        AuthMethod::Custom(_) | AuthMethod::BrowserSession => secret.as_str().to_owned(),
        AuthMethod::None => unreachable!("handled above"),
    };
    let header_value = HeaderValue::from_str(&value).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::CredentialError,
            "credential cannot be represented as an HTTP header",
        )
    })?;
    headers.insert(header_name, header_value);
    Ok(headers)
}

fn response_headers_to_map(headers: &HeaderMap) -> std::collections::BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name_str = name.as_str().to_lowercase();
            if crate::networking::is_sensitive_header(&name_str) {
                return None;
            }
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}

fn map_request_error(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::new(ProviderErrorKind::Timeout, "provider request timed out")
    } else if error.is_connect() || error.is_request() {
        ProviderError::new(
            ProviderErrorKind::NetworkUnavailable,
            "provider network request failed",
        )
    } else if error.is_status() {
        ProviderError::new(
            ProviderErrorKind::ProviderUnavailable,
            "provider request failed",
        )
    } else {
        ProviderError::new(ProviderErrorKind::Unknown, "provider transport failed")
    }
}

fn map_credential_error(error: CredentialError) -> ProviderError {
    ProviderError::new(ProviderErrorKind::CredentialError, error.to_string())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[async_trait::async_trait]
impl ProviderAdapter for OpenAiCompatibleAdapter {
    fn descriptor(&self) -> ProviderDescriptor {
        self.config.descriptor()
    }

    async fn validate_account(&self, _account: &AccountContext) -> ProviderResult<AccountIdentity> {
        // OpenAI-compatible providers validate via fetch_quota.
        Err(ProviderError::new(
            ProviderErrorKind::UnsupportedCapability,
            "validate_account not supported by OpenAI-compatible adapter",
        ))
    }

    async fn fetch_quota(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<QuotaSnapshotSet> {
        let config = OpenAiCompatibleConfig::new(
            &account.provider_id,
            &account.provider_id,
            ctx.base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1"),
        );
        let credentials = ctx
            .credentials
            .clone()
            .unwrap_or_else(crate::credentials::CredentialStore::default);
        let result = self
            .fetch(&config, &account.account_id, &credentials)
            .await?;
        Ok(result.snapshots)
    }

    async fn fetch_usage(
        &self,
        _account: &AccountContext,
        _range: UsageRange,
        _ctx: &FetchContext,
    ) -> ProviderResult<Vec<UsagePoint>> {
        Err(ProviderError::new(
            ProviderErrorKind::UnsupportedCapability,
            "fetch_usage not supported by OpenAI-compatible adapter",
        ))
    }

    async fn health_check(
        &self,
        _account: &AccountContext,
        _ctx: &FetchContext,
    ) -> ProviderResult<HealthStatus> {
        Ok(HealthStatus::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_validation_preserves_base_path() {
        let config = OpenAiCompatibleConfig {
            base_url: "https://provider.test/v1".to_owned(),
            quota_endpoint: Some("/quota".to_owned()),
            ..OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1")
        };
        assert_eq!(
            endpoint_url(&config, "/quota").expect("URL").as_str(),
            "https://provider.test/v1/quota"
        );
    }

    #[test]
    fn endpoint_validation_rejects_non_http_and_absolute_paths() {
        let invalid_base =
            OpenAiCompatibleConfig::new("generic", "Generic", "file:///tmp/provider");
        assert!(endpoint_url(&invalid_base, "/quota").is_err());
        let config = OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1");
        assert!(endpoint_url(&config, "https://evil.test/quota").is_err());
        assert!(endpoint_url(&config, "//evil.test/quota").is_err());
    }

    #[test]
    fn request_requires_credential_for_authenticated_config() {
        let config = OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1");
        let error = request_headers(&config, &CredentialStore::default()).expect_err("credential");
        assert_eq!(error.code, ProviderErrorKind::CredentialError);
    }

    #[test]
    fn parser_status_mapping_is_reused_without_network() {
        let config = OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1");
        let response = ProviderResponse::new(429, "{}").with_header("Retry-After", "30");
        let error = config.parse_response(&response).expect_err("rate limit");
        assert_eq!(error.code, ProviderErrorKind::RateLimited);
        assert_eq!(error.http_status, Some(429));
        assert_eq!(error.retry_at.as_deref(), Some("30"));
    }
}
