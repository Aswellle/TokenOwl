// Anthropic native-protocol adapter
//! Anthropic Messages-API adapter (L2: Health/Model).
//!
//! Anthropic does not publish an account-quota endpoint for API keys, so this
//! adapter reports exactly what the official surface provides and nothing
//! more: `GET /v1/models` doubles as a health and account-identity probe
//! (401 → auth problem, 200 → healthy), and the documented
//! `anthropic-ratelimit-*` response headers become quota-shaped snapshots
//! when present. It never invents a usage or balance endpoint to fill a UI
//! progress bar (docs/09: unknown is not zero).

use crate::credentials::{CredentialError, CredentialStore};
use crate::domain::provider::{
    AccountContext, AccountIdentity, Confidence, FetchContext, HealthStatus,
    OpenAiCompatibleConfig, ProviderAdapter, ProviderDescriptor, ProviderError, ProviderErrorKind,
    ProviderResponse, ProviderResult, QuotaSnapshot, QuotaSnapshotSet, SnapshotStatus, UsagePoint,
    UsageRange, WindowType,
};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 512 * 1024;
/// The API version header Anthropic documents for every REST call.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// `anthropic-ratelimit-unified-5h-*` / `...-7d-*` header prefixes, per the
/// official rate-limits documentation.
const RATE_LIMIT_PREFIXES: [(&str, &str, i64); 2] = [
    ("anthropic-ratelimit-unified-5h", "5h", 5 * 3600),
    ("anthropic-ratelimit-unified-7d", "7d", 7 * 86_400),
];

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
pub struct AnthropicAdapter {
    client: Client,
    limits: AdapterLimits,
    config: OpenAiCompatibleConfig,
}

impl AnthropicAdapter {
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

    pub fn config(&self) -> &OpenAiCompatibleConfig {
        &self.config
    }

    /// Calls `GET {base}/v1/models` — one request that answers health and
    /// identity at once and returns the ratelimit headers from the response.
    async fn probe(
        &self,
        credentials: &CredentialStore,
    ) -> ProviderResult<(
        HealthStatus,
        Option<AccountIdentity>,
        std::collections::BTreeMap<String, String>,
    )> {
        let url = endpoint_url(&self.config)?;
        let headers = request_headers(&self.config, credentials)?;
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let response = self.read_response(request).await?;
        let status = response.status;
        let identity = match status {
            200..=299 => HealthStatus::Healthy,
            401 | 403 => return Err(auth_error(status)),
            429 => HealthStatus::Degraded,
            500..=599 => HealthStatus::Unhealthy,
            _ => HealthStatus::Unknown,
        };
        let account = if (200..=299).contains(&status) {
            models_identity(&response.body).ok()
        } else {
            None
        };
        Ok((identity, account, response.headers))
    }

    /// Streams one response under the configured size limit. Header order is
    /// preserved through the `reqwest` `HeaderMap` iteration.
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
        let headers = response_headers_to_map(&response_headers);
        Ok(ProviderResponse {
            status: status.as_u16(),
            body,
            headers,
        })
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for AnthropicAdapter {
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
            "validate_account runs through fetch_quota on the Anthropic adapter",
        ))
    }

    /// The models probe is the quota source: ratelimit headers, when the
    /// deployment reports them, become window snapshots; a response that lacks
    /// them yields nothing rather than a zero.
    async fn fetch_quota(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<QuotaSnapshotSet> {
        let credentials = ctx.credentials.as_ref().ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::CredentialError,
                "Anthropic adapter requires a credential store",
            )
        })?;
        let (health, identity, headers) = self.probe(credentials).await?;
        if health == HealthStatus::Unhealthy {
            return Err(ProviderError::new(
                ProviderErrorKind::ProviderUnavailable,
                "Anthropic API answered with a server error",
            ));
        }
        let observed_at = unix_seconds();
        let mut snapshots = ratelimit_snapshots(&headers, observed_at);
        for snapshot in &mut snapshots {
            snapshot.account_id = Some(account.account_id.clone());
        }
        if snapshots.is_empty() && identity.is_none() {
            // Probe succeeded but exposed nothing observable: that is a valid
            // L2 answer, surfaced as an explicitly unknown snapshot set.
            let mut unknown = QuotaSnapshot::unknown();
            unknown.account_id = Some(account.account_id.clone());
            unknown.metric_name = Some("Anthropic API".to_owned());
            unknown.status = SnapshotStatus::Unavailable;
            return Ok(QuotaSnapshotSet::new(vec![unknown]));
        }
        Ok(QuotaSnapshotSet::new(snapshots))
    }

    async fn fetch_usage(
        &self,
        _account: &AccountContext,
        _range: UsageRange,
        _ctx: &FetchContext,
    ) -> ProviderResult<Vec<UsagePoint>> {
        // No documented per-key usage endpoint; the history pipeline records
        // client-side instead.
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
                "Anthropic adapter requires a credential store",
            )
        })?;
        let (health, _, _) = self.probe(credentials).await?;
        Ok(health)
    }
}

/// Maps documented ratelimit headers to windowed quota snapshots.
fn ratelimit_snapshots(
    headers: &std::collections::BTreeMap<String, String>,
    observed_at: u64,
) -> Vec<QuotaSnapshot> {
    let mut snapshots = Vec::new();
    for (prefix, tag, span_seconds) in RATE_LIMIT_PREFIXES {
        let limit = header_value(headers, &format!("{prefix}-limit"));
        let remaining = header_value(headers, &format!("{prefix}-remaining"));
        let reset = header_value(headers, &format!("{prefix}-reset"));
        let (Some(limit), Some(remaining)) = (limit, remaining) else {
            continue;
        };
        let (Ok(limit), Ok(remaining)) = (limit.parse::<f64>(), remaining.parse::<f64>()) else {
            continue;
        };
        let mut snapshot = QuotaSnapshot::unknown();
        snapshot.id = Some(format!("anthropic-{tag}"));
        snapshot.metric_id = Some(format!("ratelimit-{tag}"));
        snapshot.metric_name = Some(format!("Anthropic {tag} 速率窗口"));
        snapshot.unit = Some(crate::domain::provider::QuotaUnit::Request);
        snapshot.limit = Some(limit);
        snapshot.remaining = Some(remaining);
        if remaining <= limit {
            snapshot.used = Some((limit - remaining).max(0.0));
        }
        // The reset header is an RFC 3339 instant per the rate-limits docs.
        snapshot.window = Some(crate::domain::provider::QuotaWindow {
            window_type: Some(WindowType::Rolling),
            duration_seconds: Some(span_seconds),
            starts_at: None,
            ends_at: None,
            reset_at: reset.clone(),
        });
        snapshot.reset_at = reset;
        snapshot.observed_at = Some(rfc3339_from_unix(observed_at));
        snapshot.source = crate::domain::provider::DataSource::OfficialApi;
        snapshot.confidence = Confidence::Exact;
        snapshot.status = SnapshotStatus::Available;
        snapshots.push(snapshot);
    }
    snapshots
}

fn header_value(
    headers: &std::collections::BTreeMap<String, String>,
    name: &str,
) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

/// Extracts a display identity from the models payload without retaining it.
fn models_identity(body: &str) -> Result<AccountIdentity, ()> {
    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| ())?;
    let count = parsed
        .get("data")
        .and_then(|data| data.as_array())
        .map_or(0, std::vec::Vec::len);
    if count == 0 {
        return Err(());
    }
    Ok(AccountIdentity {
        account_id: String::new(),
        provider_id: "anthropic".to_owned(),
        display_name: Some(format!("{count} models available")),
    })
}

fn endpoint_url(config: &OpenAiCompatibleConfig) -> ProviderResult<Url> {
    let endpoint = config.models_endpoint.as_deref().unwrap_or("/v1/models");
    let base = Url::parse(&config.base_url).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "base URL is not a valid absolute URL",
        )
    })?;
    base.join(endpoint.trim_start_matches('/')).map_err(|_| {
        ProviderError::new(
            ProviderErrorKind::ConfigInvalid,
            "models endpoint is not a valid relative path",
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
            "Anthropic adapter requires a credential reference",
        ));
    };
    let secret = credentials
        .resolve(reference)
        .map_err(map_credential_error)?;
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(secret.as_str()).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "credential value is not a valid header value",
            )
        })?,
    );
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_VERSION),
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

fn auth_error(status: u16) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::AuthExpired,
        "Anthropic rejected the API key",
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

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// Seconds since the epoch → RFC 3339 UTC, without pulling a date library.
fn rfc3339_from_unix(seconds: u64) -> String {
    let days = seconds / 86_400;
    let secs_of_day = seconds % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm), valid for the epoch range.
    let z = i64::try_from(days).unwrap_or(i64::MAX) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    fn fixture_headers() -> BTreeMap<String, String> {
        [
            ("content-type", "application/json"),
            ("anthropic-ratelimit-unified-5h-limit", "800"),
            ("anthropic-ratelimit-unified-5h-remaining", "672"),
            (
                "anthropic-ratelimit-unified-5h-reset",
                "2026-10-12T12:00:00Z",
            ),
            ("anthropic-ratelimit-unified-7d-limit", "20000"),
            ("anthropic-ratelimit-unified-7d-remaining", "18000"),
            (
                "anthropic-ratelimit-unified-7d-reset",
                "2026-10-16T00:00:00Z",
            ),
            ("request-id", "req_01"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn ratelimit_headers_map_to_windowed_snapshots() {
        let snapshots = ratelimit_snapshots(&fixture_headers(), 1_790_000_000);
        assert_eq!(snapshots.len(), 2);
        let five = &snapshots[0];
        assert_eq!(five.metric_id.as_deref(), Some("ratelimit-5h"));
        assert_eq!(five.limit, Some(800.0));
        assert_eq!(five.remaining, Some(672.0));
        assert_eq!(five.used, Some(128.0));
        assert_eq!(
            five.window.as_ref().and_then(|w| w.duration_seconds),
            Some(5 * 3600)
        );
        assert_eq!(
            five.window.as_ref().and_then(|w| w.window_type),
            Some(WindowType::Rolling)
        );
        assert_eq!(five.reset_at.as_deref(), Some("2026-10-12T12:00:00Z"));
        let weekly = &snapshots[1];
        assert_eq!(weekly.remaining, Some(18_000.0));
        assert_eq!(
            weekly.window.as_ref().and_then(|w| w.duration_seconds),
            Some(7 * 86_400)
        );
    }

    #[test]
    fn missing_headers_yield_no_invented_zeroes() {
        let headers = BTreeMap::from([("request-id".to_owned(), "req_01".to_owned())]);
        assert!(ratelimit_snapshots(&headers, 1).is_empty());
        let partial = BTreeMap::from([(
            "anthropic-ratelimit-unified-5h-limit".to_owned(),
            "800".to_owned(),
        )]);
        assert!(ratelimit_snapshots(&partial, 1).is_empty());
    }

    #[test]
    fn malformed_header_values_are_skipped_not_zeroed() {
        let headers = BTreeMap::from([
            (
                "anthropic-ratelimit-unified-5h-limit".to_owned(),
                "abc".to_owned(),
            ),
            (
                "anthropic-ratelimit-unified-5h-remaining".to_owned(),
                "10".to_owned(),
            ),
        ]);
        assert!(ratelimit_snapshots(&headers, 1).is_empty());
    }

    #[test]
    fn models_identity_counts_entries_and_rejects_empty() {
        let body = r#"{"data":[{"id":"claude-sonnet-4"},{"id":"claude-opus-4"}]}"#;
        let identity = models_identity(body).expect("identity");
        assert_eq!(identity.provider_id, "anthropic");
        assert_eq!(identity.display_name.as_deref(), Some("2 models available"));
        assert!(models_identity(r#"{"data":[]}"#).is_err());
        assert!(models_identity("not json").is_err());
    }

    #[test]
    fn rfc3339_conversion_matches_known_instants() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_760_140_800), "2025-10-11T00:00:00Z");
        assert_eq!(rfc3339_from_unix(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn descriptor_reports_honest_l2_capabilities() {
        let mut config =
            OpenAiCompatibleConfig::new("anthropic", "Anthropic", "https://api.anthropic.com/v1");
        config.models_endpoint = Some("/v1/models".to_owned());
        config.api_key_header = "x-api-key".to_owned();
        let adapter = AnthropicAdapter::with_default_limits(config).expect("adapter");
        let capabilities = adapter.descriptor().capabilities;
        assert!(capabilities.model_catalog);
        assert!(!capabilities.quota);
        assert!(!capabilities.usage);
        assert!(!capabilities.balance);
        assert!(!capabilities.reset_at);
    }
}
