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
    ProviderResult, QuotaSnapshot, QuotaSnapshotSet, SnapshotStatus, UsagePoint, UsageRange,
};
use crate::networking::{HttpClient, HttpClientConfig};
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
    client: HttpClient,
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
        // The shared client is the transport authority: redirects are denied,
        // no implicit environment proxy is honored, and the response-size cap
        // is enforced while streaming. Only the timeout and size limit vary
        // per adapter.
        let client = HttpClient::new(HttpClientConfig::from_adapter_limits(
            limits.timeout,
            limits.max_response_bytes,
        ))?;
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

    /// Fetches every configured quota-bearing endpoint (quota, usage, balance)
    /// and merges their normalized snapshots. Endpoints are de-duplicated by
    /// path; a response without quota fields contributes nothing; a failed
    /// endpoint becomes an error snapshot while its siblings keep updating.
    /// When every endpoint fails, the first provider error is returned so the
    /// refresh pipeline keeps the last known good values.
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
        let endpoints = configured_quota_endpoints(config);
        if endpoints.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::UnsupportedCapability,
                "provider has no quota, usage, or balance endpoint",
            ));
        }
        let mut outcomes = Vec::with_capacity(endpoints.len());
        for (kind, endpoint) in &endpoints {
            outcomes.push((
                *kind,
                self.fetch_endpoint(config, endpoint, credentials).await,
            ));
        }
        let mut snapshots = merge_endpoint_outcomes(outcomes)?;
        for snapshot in &mut snapshots {
            if snapshot.account_id.is_none() {
                snapshot.account_id = Some(account_id.to_owned());
            }
        }
        let observed_at = unix_seconds();
        Ok(FetchResult {
            snapshots: QuotaSnapshotSet::new(snapshots),
            observed_at,
        })
    }

    async fn fetch_endpoint(
        &self,
        config: &OpenAiCompatibleConfig,
        endpoint: &str,
        credentials: &CredentialStore,
    ) -> EndpointOutcome {
        let url = match endpoint_url(config, endpoint) {
            Ok(url) => url,
            Err(error) => return EndpointOutcome::Failed(error),
        };
        let headers = match request_headers(config, credentials) {
            Ok(headers) => headers,
            Err(error) => return EndpointOutcome::Failed(error),
        };
        // Do not send a body or query containing secrets. Some compatible
        // providers require the account/model in metadata; those are
        // configured as URL paths.
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let outcome = self.client.send_capped(request).await;
        match outcome {
            Ok(response) => {
                let parsed = match config.field_mappings.as_ref() {
                    Some(mappings) => {
                        crate::domain::provider::parse_openai_compatible_with_mappings(
                            &response, mappings,
                        )
                    }
                    None => crate::domain::provider::parse_openai_compatible_lenient(&response),
                };
                match parsed {
                    Ok(Some(set)) => EndpointOutcome::Snapshots(set),
                    Ok(None) => EndpointOutcome::Unrecognized,
                    Err(error) => EndpointOutcome::Failed(error),
                }
            }
            Err(error) => EndpointOutcome::Failed(error),
        }
    }
}

/// A quota-bearing endpoint kind, in fetch order.
pub type EndpointKind = &'static str;

/// Result of requesting one configured endpoint.
#[derive(Debug, Clone, PartialEq)]
pub enum EndpointOutcome {
    /// The endpoint answered with recognizable quota fields.
    Snapshots(QuotaSnapshotSet),
    /// The endpoint answered but exposed no quota fields.
    Unrecognized,
    /// The request or its payload failed structurally.
    Failed(ProviderError),
}

/// Quota/usage/balance endpoints in fetch priority order, de-duplicated by path.
fn configured_quota_endpoints(config: &OpenAiCompatibleConfig) -> Vec<(EndpointKind, &str)> {
    let mut endpoints: Vec<(EndpointKind, &str)> = Vec::new();
    for (kind, path) in [
        ("quota", config.quota_endpoint.as_deref()),
        ("usage", config.usage_endpoint.as_deref()),
        ("balance", config.balance_endpoint.as_deref()),
    ] {
        if let Some(path) = path {
            if !endpoints.iter().any(|(_, existing)| *existing == path) {
                endpoints.push((kind, path));
            }
        }
    }
    endpoints
}

/// Pure merge over per-endpoint outcomes. Failed endpoints become error
/// snapshots so the UI can mark that metric as unread; a refresh only fails
/// wholesale when every configured endpoint failed.
fn merge_endpoint_outcomes(
    outcomes: Vec<(EndpointKind, EndpointOutcome)>,
) -> ProviderResult<Vec<QuotaSnapshot>> {
    let mut snapshots = Vec::new();
    let mut first_error: Option<ProviderError> = None;
    for (kind, outcome) in outcomes {
        match outcome {
            EndpointOutcome::Snapshots(set) => {
                for mut snapshot in set.snapshots {
                    if snapshot.metric_id.is_none() {
                        snapshot.metric_id = Some(kind.to_owned());
                    }
                    snapshots.push(snapshot);
                }
            }
            EndpointOutcome::Unrecognized => {}
            EndpointOutcome::Failed(error) => {
                snapshots.push(QuotaSnapshot {
                    metric_id: Some(kind.to_owned()),
                    status: SnapshotStatus::Error,
                    ..QuotaSnapshot::default()
                });
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    if snapshots
        .iter()
        .all(|snapshot| snapshot.status == SnapshotStatus::Error)
    {
        if let Some(error) = first_error {
            return Err(error);
        }
    }
    if snapshots.is_empty() {
        return Err(ProviderError::invalid_response(
            "response contains no recognized quota fields",
        ));
    }
    Ok(snapshots)
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
    // OAuth-style credentials carry expiry metadata; an expired session is
    // rejected before a request is spent on it.
    let secret = if matches!(config.auth_method, AuthMethod::OAuth) {
        let (secret, session) = credentials
            .resolve_session(reference)
            .map_err(map_credential_error)?;
        if session
            .as_ref()
            .is_some_and(|session| session.expired_at(unix_seconds()))
        {
            return Err(ProviderError::new(
                ProviderErrorKind::AuthExpired,
                "OAuth session has expired; re-authentication is required",
            ));
        }
        secret
    } else {
        credentials
            .resolve(reference)
            .map_err(map_credential_error)?
    };
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
        let credentials = ctx.credentials.clone().unwrap_or_default();
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
    use crate::domain::provider::ProviderResponse;

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

    fn quota_value(used: f64, limit: f64) -> QuotaSnapshotSet {
        QuotaSnapshotSet::new(vec![QuotaSnapshot {
            used: Some(used),
            limit: Some(limit),
            status: SnapshotStatus::Available,
            ..QuotaSnapshot::default()
        }])
    }

    #[test]
    fn configured_endpoints_follow_priority_and_dedup_by_path() {
        let mut config =
            OpenAiCompatibleConfig::new("generic", "Generic", "https://provider.test/v1");
        config.quota_endpoint = Some("/usage".to_owned());
        config.usage_endpoint = Some("/usage".to_owned());
        config.balance_endpoint = Some("/balance".to_owned());
        let endpoints = configured_quota_endpoints(&config);
        assert_eq!(
            endpoints,
            vec![("quota", "/usage"), ("balance", "/balance")]
        );
    }

    #[test]
    fn merge_stamps_metric_ids_and_combines_endpoints() {
        let merged = merge_endpoint_outcomes(vec![
            (
                "quota",
                EndpointOutcome::Snapshots(quota_value(10.0, 100.0)),
            ),
            (
                "balance",
                EndpointOutcome::Snapshots(QuotaSnapshotSet::new(vec![QuotaSnapshot {
                    remaining: Some(42.5),
                    status: SnapshotStatus::Partial,
                    ..QuotaSnapshot::default()
                }])),
            ),
        ])
        .expect("merge");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].metric_id.as_deref(), Some("quota"));
        assert_eq!(merged[0].status, SnapshotStatus::Available);
        assert_eq!(merged[1].metric_id.as_deref(), Some("balance"));
    }

    #[test]
    fn merge_marks_failed_endpoint_without_losing_siblings() {
        let failure = ProviderError::auth_required(401);
        let merged = merge_endpoint_outcomes(vec![
            (
                "quota",
                EndpointOutcome::Snapshots(quota_value(10.0, 100.0)),
            ),
            ("balance", EndpointOutcome::Failed(failure)),
        ])
        .expect("partial failure still merges");
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].metric_id.as_deref(), Some("balance"));
        assert_eq!(merged[1].status, SnapshotStatus::Error);
        assert_eq!(merged[1].used, None, "failed metrics never invent values");
    }

    #[test]
    fn merge_fails_wholesale_when_every_endpoint_failed() {
        let error = merge_endpoint_outcomes(vec![
            (
                "quota",
                EndpointOutcome::Failed(ProviderError::auth_required(401)),
            ),
            (
                "balance",
                EndpointOutcome::Failed(ProviderError::rate_limited(429, None)),
            ),
        ])
        .expect_err("all endpoints failed");
        assert_eq!(error.code, ProviderErrorKind::AuthRequired);
    }

    #[test]
    fn merge_treats_unrecognized_endpoints_as_absent() {
        let error = merge_endpoint_outcomes(vec![
            ("quota", EndpointOutcome::Unrecognized),
            ("balance", EndpointOutcome::Unrecognized),
        ])
        .expect_err("no quota fields anywhere");
        assert_eq!(error.code, ProviderErrorKind::InvalidResponse);
        let merged = merge_endpoint_outcomes(vec![
            ("quota", EndpointOutcome::Unrecognized),
            (
                "balance",
                EndpointOutcome::Snapshots(quota_value(10.0, 100.0)),
            ),
        ])
        .expect("one working endpoint is enough");
        assert_eq!(merged.len(), 1);
    }
}
