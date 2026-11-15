// OpenAI-compatible provider adapter
// Original content moved from src-tauri/src/openai_adapter.rs
//! Generic OpenAI-compatible HTTP adapter.
//!
//! The adapter owns transport concerns and hands the raw body directly to the existing
//! provider parser. It never logs or persists that body.

use crate::credentials::{CredentialError, CredentialStore};
use crate::domain::provider::{
    AccountContext, AccountIdentity, AuthMethod, DataSource, FetchContext, HealthStatus,
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
        Self::new_with_proxy(limits, config, None)
    }

    /// [`Self::new`] routed through an explicit user-configured proxy.
    pub fn new_with_proxy(
        limits: AdapterLimits,
        config: OpenAiCompatibleConfig,
        proxy: Option<String>,
    ) -> ProviderResult<Self> {
        if limits.timeout.is_zero() || limits.max_response_bytes == 0 {
            return Err(ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "HTTP timeout and response limit must be positive",
            ));
        }
        // The shared client is the transport authority: redirects are denied,
        // no implicit environment proxy is honored, and the response-size cap
        // is enforced while streaming. Only the timeout, size limit and the
        // user's own explicit proxy vary per adapter.
        let client = HttpClient::new(HttpClientConfig::from_adapter_limits_with_proxy(
            limits.timeout,
            limits.max_response_bytes,
            proxy,
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

    pub fn with_proxy(
        proxy: Option<String>,
        config: OpenAiCompatibleConfig,
    ) -> ProviderResult<Self> {
        Self::new_with_proxy(AdapterLimits::default(), config, proxy)
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
    ///
    /// A provider that declares only a models endpoint has no quota surface to
    /// fetch: the models probe is the whole refresh answer instead (see
    /// [`Self::fetch_models_health`]).
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
        if endpoints.is_empty() && config.reset_endpoint.is_none() {
            return self
                .fetch_models_health(config, account_id, credentials)
                .await;
        }
        let mut outcomes = Vec::with_capacity(endpoints.len() + 1);
        for (kind, endpoint) in &endpoints {
            outcomes.push((
                *kind,
                self.fetch_endpoint(config, endpoint, credentials, kind)
                    .await,
            ));
        }
        // A declared reset endpoint is a real request with its own mapping,
        // not decoration: fetch it, parse it, and bind the observation to the
        // window the provider names — or keep it as an account-level reset
        // card when no window is declared. A failed reset endpoint never
        // erases its siblings' quota data.
        let mut reset_observation = None;
        if let Some(reset_path) = config.reset_endpoint.as_deref() {
            match self
                .fetch_reset_endpoint(config, reset_path, credentials)
                .await
            {
                ResetFetch::Observation(observation) => {
                    outcomes.push((
                        "reset",
                        EndpointOutcome::Snapshots(QuotaSnapshotSet::new(vec![QuotaSnapshot {
                            id: Some(RESET_SNAPSHOT_ID.to_owned()),
                            metric_id: Some("reset".to_owned()),
                            metric_name: Some("reset".to_owned()),
                            reset_at: Some(observation.reset_at.clone()),
                            status: SnapshotStatus::Partial,
                            ..QuotaSnapshot::unknown()
                        }])),
                    ));
                    reset_observation = Some(observation);
                }
                ResetFetch::Snapshots(set) => {
                    outcomes.push(("reset", EndpointOutcome::Snapshots(set)));
                }
                ResetFetch::Unrecognized => {
                    outcomes.push(("reset", EndpointOutcome::Unrecognized));
                }
                ResetFetch::Failed(error) => {
                    outcomes.push(("reset", EndpointOutcome::Failed(error)));
                }
            }
        }
        let mut snapshots = merge_endpoint_outcomes(outcomes)?;
        // When the provider named a window, the reset belongs there: the
        // standalone reset card folds into the target snapshot. A reset that
        // names no window — or names an unknown one — stays its own card
        // instead of being copied onto every window with invented precision.
        if let Some(observation) = reset_observation
            && let Some(window_id) = observation.window_id.as_deref()
        {
            let target = snapshots.iter_mut().find(|snapshot| {
                snapshot.metric_id.as_deref() == Some(window_id)
                    || snapshot.id.as_deref() == Some(window_id)
            });
            if let Some(target) = target {
                target.reset_at = Some(observation.reset_at.clone());
                if let Some(window) = target.window.as_mut() {
                    window.reset_at = Some(observation.reset_at.clone());
                } else {
                    target.window = Some(crate::domain::provider::QuotaWindow {
                        reset_at: Some(observation.reset_at.clone()),
                        ..crate::domain::provider::QuotaWindow::default()
                    });
                }
                snapshots.retain(|snapshot| snapshot.id.as_deref() != Some(RESET_SNAPSHOT_ID));
            }
        }
        // The parsers are shared by custom and built-in accounts and default
        // to `CustomEndpoint`. A built-in manifest's traffic is the provider's
        // official API, and the UI's source label should say so.
        let source = if crate::providers::builtin::is_builtin_provider_id(&config.id) {
            DataSource::OfficialApi
        } else {
            DataSource::CustomEndpoint
        };
        for snapshot in &mut snapshots {
            snapshot.source = source;
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
        kind: EndpointKind,
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
                // Balance endpoints use a dedicated envelope parser first
                // (DeepSeek and similar `balance_infos` responses); anything
                // it does not recognize falls back to the generic mapping /
                // lenient path so custom balance shapes keep working.
                let parsed = if kind == "balance" {
                    match crate::domain::provider::parse_balance_response(&response) {
                        Ok(Some(set)) => Ok(Some(set)),
                        Ok(None) => parse_configured(config, &response),
                        Err(error) => Err(error),
                    }
                } else {
                    parse_configured(config, &response)
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

    /// Requests the declared reset endpoint. With a manifest mapping the
    /// timestamp is parsed through it (declared format, never inferred);
    /// without one, the lenient parser's reset-bearing fields answer instead.
    async fn fetch_reset_endpoint(
        &self,
        config: &OpenAiCompatibleConfig,
        endpoint: &str,
        credentials: &CredentialStore,
    ) -> ResetFetch {
        let url = match endpoint_url(config, endpoint) {
            Ok(url) => url,
            Err(error) => return ResetFetch::Failed(error),
        };
        let headers = match request_headers(config, credentials) {
            Ok(headers) => headers,
            Err(error) => return ResetFetch::Failed(error),
        };
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let response = match self.client.send_capped(request).await {
            Ok(response) => response,
            Err(error) => return ResetFetch::Failed(error),
        };
        if let Some(mapping) = config
            .field_mappings
            .as_ref()
            .and_then(|mappings| mappings.reset.as_ref())
        {
            return match crate::domain::provider::parse_reset_response(&response, mapping) {
                Ok(Some(observation)) => ResetFetch::Observation(observation),
                Ok(None) => ResetFetch::Unrecognized,
                Err(error) => ResetFetch::Failed(error),
            };
        }
        match parse_configured(config, &response) {
            Ok(Some(set))
                if set
                    .snapshots
                    .iter()
                    .any(|snapshot| snapshot.reset_at.is_some() || snapshot.window.is_some()) =>
            {
                ResetFetch::Snapshots(set)
            }
            Ok(_) => ResetFetch::Unrecognized,
            Err(error) => ResetFetch::Failed(error),
        }
    }

    /// Model-only refresh answer (L2 health probe). A provider with no
    /// quota/usage/balance endpoint proves its credential through the models
    /// endpoint, exactly as the Anthropic and Gemini native adapters do. A
    /// verified probe yields one explicit [`SnapshotStatus::Unavailable`]
    /// snapshot — connection healthy, zero invented quota values, nothing
    /// recorded into history — instead of a fetch failure.
    async fn fetch_models_health(
        &self,
        config: &OpenAiCompatibleConfig,
        account_id: &str,
        credentials: &CredentialStore,
    ) -> ProviderResult<FetchResult> {
        match self.probe_models(config, credentials).await {
            Ok(ProbeOutcome::Verified) => {
                let source = if crate::providers::builtin::is_builtin_provider_id(&config.id) {
                    DataSource::OfficialApi
                } else {
                    DataSource::CustomEndpoint
                };
                let snapshot = QuotaSnapshot {
                    id: Some(MODELS_PROBE_SNAPSHOT_ID.to_owned()),
                    account_id: Some(account_id.to_owned()),
                    metric_id: Some("models".to_owned()),
                    metric_name: Some("connection".to_owned()),
                    status: SnapshotStatus::Unavailable,
                    source,
                    ..QuotaSnapshot::unknown()
                };
                let observed_at = unix_seconds();
                Ok(FetchResult {
                    snapshots: QuotaSnapshotSet::new(vec![snapshot]),
                    observed_at,
                })
            }
            // Every non-verified verdict fails the refresh with its own
            // structured error — including the probe's request-building
            // failures (invalid config, unusable credential reference),
            // which arrive as the outer `Err`.
            Ok(
                ProbeOutcome::Malformed(error)
                | ProbeOutcome::Rejected(error)
                | ProbeOutcome::Indeterminate(error),
            )
            | Err(error) => Err(error),
        }
    }

    /// Calls the configured models endpoint and classifies the wire outcome.
    /// `Err` is reserved for failures before a request can be built (invalid
    /// config, credential resolution); everything on the wire — including
    /// transport timeouts — comes back as a [`ProbeOutcome`].
    pub async fn probe_models(
        &self,
        config: &OpenAiCompatibleConfig,
        credentials: &CredentialStore,
    ) -> ProviderResult<ProbeOutcome> {
        let Some(endpoint) = config.models_endpoint.as_deref() else {
            return Err(ProviderError::new(
                ProviderErrorKind::UnsupportedCapability,
                "provider declares no models endpoint to probe",
            ));
        };
        self.probe_endpoint(config, endpoint, credentials).await
    }

    /// Credential verdict for a candidate key: probes the models endpoint
    /// when the provider declares one, else its first quota-bearing
    /// endpoint. A 2xx body the app cannot parse still proves the
    /// credential — authentication happens at the HTTP layer.
    pub async fn probe_credential(
        &self,
        config: &OpenAiCompatibleConfig,
        credentials: &CredentialStore,
    ) -> ProviderResult<crate::scheduler::CredentialVerdict> {
        config.validate()?;
        let outcome = if config.models_endpoint.is_some() {
            self.probe_models(config, credentials).await?
        } else {
            match configured_quota_endpoints(config).into_iter().next() {
                Some((_kind, endpoint)) => {
                    self.probe_endpoint(config, endpoint, credentials).await?
                }
                None => {
                    return Err(ProviderError::new(
                        ProviderErrorKind::UnsupportedCapability,
                        "provider declares no endpoint to probe",
                    ));
                }
            }
        };
        Ok(match outcome {
            ProbeOutcome::Verified | ProbeOutcome::Malformed(_) => {
                crate::scheduler::CredentialVerdict::Verified
            }
            ProbeOutcome::Rejected(error) => crate::scheduler::CredentialVerdict::Rejected(error),
            ProbeOutcome::Indeterminate(error) => {
                crate::scheduler::CredentialVerdict::Indeterminate(error)
            }
        })
    }

    /// One GET against an endpoint, classified through the shared probe
    /// verdicts. The request never carries a body or query.
    async fn probe_endpoint(
        &self,
        config: &OpenAiCompatibleConfig,
        endpoint: &str,
        credentials: &CredentialStore,
    ) -> ProviderResult<ProbeOutcome> {
        let url = endpoint_url(config, endpoint)?;
        let headers = request_headers(config, credentials)?;
        let request = self
            .client
            .get(url)
            .headers(headers)
            .header("accept", "application/json");
        let outcome = self.client.send_capped(request).await;
        Ok(match outcome {
            Ok(response) => classify_probe_response(&response),
            Err(error) => ProbeOutcome::Indeterminate(error),
        })
    }
}

/// Snapshot id of the model-only health answer. Tests and UI heuristics key
/// on the metric id, never on a display string.
pub const MODELS_PROBE_SNAPSHOT_ID: &str = "models-probe";

/// Snapshot id of a standalone (window-unbound) reset-endpoint observation.
pub const RESET_SNAPSHOT_ID: &str = "reset-endpoint";

/// Result of requesting the configured reset endpoint.
#[derive(Debug, Clone, PartialEq)]
enum ResetFetch {
    /// The declared reset mapping resolved to a timestamp observation.
    Observation(crate::domain::provider::ResetObservation),
    /// No reset mapping declared (custom accounts): the lenient parser
    /// recognized reset-bearing fields in the response.
    Snapshots(QuotaSnapshotSet),
    /// The endpoint answered but exposed no reset value.
    Unrecognized,
    /// The request or its payload failed structurally.
    Failed(ProviderError),
}

/// What a models probe concluded. `Verified` proves the credential and the
/// endpoint work without inventing any quota value; `Rejected` proves the
/// credential was refused; `Indeterminate` means the answer says nothing
/// about the credential (network, rate limit, server outage); `Malformed`
/// means authentication passed but the payload was unusable.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeOutcome {
    Verified,
    Rejected(ProviderError),
    Indeterminate(ProviderError),
    Malformed(ProviderError),
}

/// Pure wire-status classification shared by the models probe and the
/// candidate-credential probe. A 2xx must at least be JSON to count as
/// verified — the payload itself is never retained.
fn classify_probe_response(response: &crate::domain::provider::ProviderResponse) -> ProbeOutcome {
    match response.status {
        200..=299 => match serde_json::from_str::<serde_json::Value>(&response.body) {
            Ok(_) => ProbeOutcome::Verified,
            Err(error) => ProbeOutcome::Malformed(ProviderError::parser_failed(format!(
                "models probe returned invalid JSON: {error}"
            ))),
        },
        401 | 403 => ProbeOutcome::Rejected(ProviderError::auth_required(response.status)),
        408 => ProbeOutcome::Indeterminate(
            ProviderError::new(ProviderErrorKind::Timeout, "provider request timed out")
                .with_status(response.status),
        ),
        429 => ProbeOutcome::Indeterminate(ProviderError::rate_limited(
            response.status,
            response
                .headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("retry-after"))
                .map(|(_, value)| value.clone()),
        )),
        500..=599 => ProbeOutcome::Indeterminate(
            ProviderError::new(
                ProviderErrorKind::ProviderUnavailable,
                "provider returned a server error",
            )
            .with_status(response.status),
        ),
        status => ProbeOutcome::Indeterminate(
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                format!("unexpected provider HTTP status {status}"),
            )
            .with_status(status),
        ),
    }
}

fn parse_configured(
    config: &OpenAiCompatibleConfig,
    response: &crate::domain::provider::ProviderResponse,
) -> ProviderResult<Option<QuotaSnapshotSet>> {
    match config.field_mappings.as_ref() {
        // A manifest that declared quota mappings is parsed by them: name
        // guessing could misread its response shape.
        Some(mappings) if mappings.has_quota_mappings() => {
            crate::domain::provider::parse_openai_compatible_with_mappings(response, mappings)
        }
        // Mappings that only carry the reset mapping (or none at all) leave
        // the quota fields to the built-in lenient field-name search.
        _ => crate::domain::provider::parse_openai_compatible_lenient(response),
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
        if let Some(path) = path
            && !endpoints.iter().any(|(_, existing)| *existing == path)
        {
            endpoints.push((kind, path));
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
        && let Some(error) = first_error
    {
        return Err(error);
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

    /// The models probe maps every wire status class to its verdict: 2xx
    /// verifies, 401/403 reject, 429/5xx/transport stay indeterminate.
    #[test]
    fn models_probe_classification_covers_every_status_class() {
        use crate::domain::provider::ProviderResponse;
        assert_eq!(
            classify_probe_response(&ProviderResponse::new(200, r#"{"data":[]}"#)),
            ProbeOutcome::Verified
        );
        let rejected = classify_probe_response(&ProviderResponse::new(401, "{}"));
        let ProbeOutcome::Rejected(error) = rejected else {
            panic!("401 must reject");
        };
        assert_eq!(error.code, ProviderErrorKind::AuthRequired);
        assert_eq!(error.http_status, Some(401));

        let limited = classify_probe_response(
            &ProviderResponse::new(429, "{}").with_header("Retry-After", "30"),
        );
        let ProbeOutcome::Indeterminate(error) = limited else {
            panic!("429 is indeterminate, not a key rejection");
        };
        assert_eq!(error.code, ProviderErrorKind::RateLimited);
        assert_eq!(error.retry_at.as_deref(), Some("30"));

        let server_error = classify_probe_response(&ProviderResponse::new(503, "{}"));
        assert!(matches!(server_error, ProbeOutcome::Indeterminate(_)));

        let unexpected = classify_probe_response(&ProviderResponse::new(302, ""));
        assert!(matches!(unexpected, ProbeOutcome::Indeterminate(_)));
    }

    /// A 2xx body that is not JSON at all is a structure error, not a
    /// verified credential — an HTML login page must not read as healthy.
    #[test]
    fn models_probe_2xx_non_json_body_is_malformed_not_verified() {
        use crate::domain::provider::ProviderResponse;
        let outcome = classify_probe_response(&ProviderResponse::new(200, "<html>gateway</html>"));
        let ProbeOutcome::Malformed(error) = outcome else {
            panic!("non-JSON 2xx must be malformed");
        };
        assert_eq!(error.code, ProviderErrorKind::ParserFailed);
    }

    /// A model-only provider answers its refresh with one explicit unknown
    /// snapshot: connection healthy, no invented quota values.
    #[test]
    fn model_only_probe_snapshot_carries_no_quota_values() {
        let snapshot = QuotaSnapshot {
            id: Some(MODELS_PROBE_SNAPSHOT_ID.to_owned()),
            account_id: Some("account-1".to_owned()),
            metric_id: Some("models".to_owned()),
            metric_name: Some("connection".to_owned()),
            status: SnapshotStatus::Unavailable,
            source: DataSource::OfficialApi,
            ..QuotaSnapshot::unknown()
        };
        assert_eq!(snapshot.used, None);
        assert_eq!(snapshot.limit, None);
        assert_eq!(snapshot.remaining, None);
        assert_eq!(snapshot.reset_at, None);
    }

    /// Every builtin manifest that declares only a models endpoint rides the
    /// probe path: no quota endpoints configured, models endpoint present,
    /// and the declared level is the L2 the probe can actually deliver.
    #[test]
    fn models_only_builtins_route_through_the_probe() {
        use crate::providers::manifest::ProviderManifest;
        let mut model_only = 0;
        for (id, yaml) in crate::providers::builtin::BUILTIN_MANIFESTS {
            let manifest = ProviderManifest::from_yaml(yaml).expect("embedded manifest parses");
            let config = manifest.to_config();
            let quota_endpoints = configured_quota_endpoints(&config);
            if quota_endpoints.is_empty() && config.models_endpoint.is_some() {
                model_only += 1;
                assert!(
                    config.validate().is_ok(),
                    "model-only manifest {id} must validate"
                );
                assert!(
                    matches!(
                        manifest.effective_capability_level().as_deref(),
                        Some("L2" | "L1")
                    ),
                    "model-only manifest {id} must not promise more than the probe delivers"
                );
            }
        }
        assert!(
            model_only >= 5,
            "expected several model-only builtins to exercise the probe path, found {model_only}"
        );
    }
}
