//! Provider descriptors, normalized quota data, and response parsing.
//!
//! Provider adapters keep raw responses at the boundary. The normalized types in
//! this module intentionally use `Option` for every value a provider may omit so
//! that an unknown value is never presented as zero.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

/// Capabilities a provider actually exposes through its supported integration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub identity: bool,
    pub balance: bool,
    pub usage: bool,
    pub quota: bool,
    pub reset_at: bool,
    pub model_catalog: bool,
    pub pricing: bool,
    pub oauth: bool,
    pub api_key: bool,
    pub browser_session: bool,
    pub local_source: bool,
    pub open_ai_compatible: bool,
}

impl ProviderCapabilities {
    /// Capabilities for a provider with no declared endpoints.
    pub const fn none() -> Self {
        Self {
            identity: false,
            balance: false,
            usage: false,
            quota: false,
            reset_at: false,
            model_catalog: false,
            pricing: false,
            oauth: false,
            api_key: false,
            browser_session: false,
            local_source: false,
            open_ai_compatible: false,
        }
    }

    /// Returns whether a capability is available.
    pub const fn supports(&self, capability: ProviderCapability) -> bool {
        match capability {
            ProviderCapability::Identity => self.identity,
            ProviderCapability::Balance => self.balance,
            ProviderCapability::Usage => self.usage,
            ProviderCapability::Quota => self.quota,
            ProviderCapability::ResetAt => self.reset_at,
            ProviderCapability::ModelCatalog => self.model_catalog,
            ProviderCapability::Pricing => self.pricing,
            ProviderCapability::Oauth => self.oauth,
            ProviderCapability::ApiKey => self.api_key,
            ProviderCapability::BrowserSession => self.browser_session,
            ProviderCapability::LocalSource => self.local_source,
            ProviderCapability::OpenAiCompatible => self.open_ai_compatible,
        }
    }
}

/// A named provider capability, useful when a caller needs to check one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCapability {
    Identity,
    Balance,
    Usage,
    Quota,
    ResetAt,
    ModelCatalog,
    Pricing,
    Oauth,
    ApiKey,
    BrowserSession,
    LocalSource,
    OpenAiCompatible,
}

/// Broad provider region metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRegion {
    #[default]
    Global,
    NorthAmerica,
    Europe,
    AsiaPacific,
    China,
    Custom(String),
}

/// Authentication mechanisms supported by a provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[default]
    ApiKey,
    Bearer,
    Basic,
    OAuth,
    BrowserSession,
    None,
    Custom(String),
}

fn default_refresh() -> Duration {
    Duration::from_secs(300)
}

/// A provider's stable, UI-independent description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub region: ProviderRegion,
    pub auth_methods: Vec<AuthMethod>,
    pub capabilities: ProviderCapabilities,
    #[serde(default = "default_refresh")]
    pub default_refresh: Duration,
    pub docs_url: Option<String>,
}

impl Default for ProviderDescriptor {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            region: ProviderRegion::Global,
            auth_methods: Vec::new(),
            capabilities: ProviderCapabilities::none(),
            default_refresh: default_refresh(),
            docs_url: None,
        }
    }
}

impl ProviderDescriptor {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            ..Self::default()
        }
    }
}

/// Alias used by generic OpenAI-compatible integrations.
pub type OpenAiCompatibleDescriptor = ProviderDescriptor;

/// Unified provider error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderErrorKind {
    AuthRequired,
    AuthExpired,
    NetworkUnavailable,
    Timeout,
    TlsError,
    RateLimited,
    ProviderUnavailable,
    InvalidResponse,
    ParserFailed,
    UnsupportedCapability,
    ConfigInvalid,
    CredentialError,
    StorageError,
    Unknown,
}

/// Backwards-compatible name for consumers that call the structured kind a code.
pub type ProviderErrorCode = ProviderErrorKind;

/// Structured error returned at the provider boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderError {
    pub code: ProviderErrorKind,
    pub message: String,
    pub retry_at: Option<String>,
    pub retryable: bool,
    pub http_status: Option<u16>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        let retryable = matches!(
            kind,
            ProviderErrorKind::NetworkUnavailable
                | ProviderErrorKind::Timeout
                | ProviderErrorKind::TlsError
                | ProviderErrorKind::RateLimited
                | ProviderErrorKind::ProviderUnavailable
        );
        Self {
            code: kind,
            message: message.into(),
            retry_at: None,
            retryable,
            http_status: None,
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_retry_at(mut self, retry_at: impl Into<String>) -> Self {
        self.retry_at = Some(retry_at.into());
        self
    }

    pub fn auth_required(status: u16) -> Self {
        Self::new(
            ProviderErrorKind::AuthRequired,
            "provider authorization required",
        )
        .with_status(status)
    }

    pub fn rate_limited(status: u16, retry_at: Option<String>) -> Self {
        let mut error = Self::new(
            ProviderErrorKind::RateLimited,
            "provider rate limit reached",
        )
        .with_status(status);
        error.retry_at = retry_at;
        error
    }

    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::InvalidResponse, message)
    }

    pub fn parser_failed(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ParserFailed, message)
    }

    pub fn kind(&self) -> ProviderErrorKind {
        self.code
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}

pub type ProviderResult<T> = Result<T, ProviderError>;

/// Units commonly used by quota and balance endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaUnit {
    Token,
    Credit,
    Request,
    Currency,
    Percent,
}

/// A quota window supplied by the provider.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub window_type: Option<WindowType>,
    pub duration_seconds: Option<i64>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub reset_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowType {
    Rolling,
    Fixed,
    Calendar,
    Subscription,
    Custom,
}

/// Where a normalized value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataSource {
    OfficialApi,
    OAuthApi,
    BrowserSession,
    Cookie,
    CliLocal,
    LocalDb,
    CustomEndpoint,
    Manual,
    Cached,
}

impl Default for DataSource {
    fn default() -> Self {
        Self::CustomEndpoint
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    Exact,
    Derived,
    Estimated,
    Stale,
    Unknown,
}

impl Default for Confidence {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SnapshotStatus {
    Available,
    Partial,
    Unavailable,
    Stale,
    Error,
}

impl Default for SnapshotStatus {
    fn default() -> Self {
        Self::Unavailable
    }
}

/// Normalized quota data. Missing provider values remain `None` and serialize as
/// JSON `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuotaSnapshot {
    pub id: Option<String>,
    pub account_id: Option<String>,
    pub metric_id: Option<String>,
    pub metric_name: Option<String>,
    pub unit: Option<QuotaUnit>,
    pub currency: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub window: Option<QuotaWindow>,
    pub reset_at: Option<String>,
    pub observed_at: Option<String>,
    pub source: DataSource,
    pub confidence: Confidence,
    pub status: SnapshotStatus,
}

impl QuotaSnapshot {
    pub fn unknown() -> Self {
        Self::default()
    }
}

/// A collection allows providers to expose multiple quota buckets in one response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaSnapshotSet {
    pub snapshots: Vec<QuotaSnapshot>,
}

impl QuotaSnapshotSet {
    pub fn new(snapshots: Vec<QuotaSnapshot>) -> Self {
        Self { snapshots }
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn as_slice(&self) -> &[QuotaSnapshot] {
        &self.snapshots
    }

    pub fn iter(&self) -> std::slice::Iter<'_, QuotaSnapshot> {
        self.snapshots.iter()
    }
}

impl From<Vec<QuotaSnapshot>> for QuotaSnapshotSet {
    fn from(snapshots: Vec<QuotaSnapshot>) -> Self {
        Self::new(snapshots)
    }
}

impl IntoIterator for QuotaSnapshotSet {
    type Item = QuotaSnapshot;
    type IntoIter = std::vec::IntoIter<QuotaSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.snapshots.into_iter()
    }
}

pub type NormalizedQuotaSnapshot = QuotaSnapshot;
pub type NormalizedQuotaSnapshotSet = QuotaSnapshotSet;

/// Configuration for a declarative OpenAI-compatible endpoint.
///
/// The config stores only a credential reference. Secret values are resolved by
/// the credential service at request time and never belong in this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleConfig {
    pub id: String,
    pub display_name: String,
    pub region: ProviderRegion,
    pub base_url: String,
    pub credential_ref: Option<String>,
    pub auth_method: AuthMethod,
    pub api_key_header: String,
    pub models_endpoint: Option<String>,
    pub balance_endpoint: Option<String>,
    pub usage_endpoint: Option<String>,
    pub quota_endpoint: Option<String>,
    pub reset_endpoint: Option<String>,
    pub docs_url: Option<String>,
    pub default_refresh: Duration,
    pub capabilities: ProviderCapabilities,
}

impl Default for OpenAiCompatibleConfig {
    fn default() -> Self {
        Self::new(
            "custom.openai-compatible",
            "Custom OpenAI-Compatible",
            "https://example.invalid/v1",
        )
    }
}

impl OpenAiCompatibleConfig {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            region: ProviderRegion::Global,
            base_url: base_url.into(),
            credential_ref: None,
            auth_method: AuthMethod::ApiKey,
            api_key_header: "Authorization".to_owned(),
            models_endpoint: Some("/models".to_owned()),
            balance_endpoint: None,
            usage_endpoint: None,
            quota_endpoint: None,
            reset_endpoint: None,
            docs_url: None,
            default_refresh: default_refresh(),
            capabilities: ProviderCapabilities {
                api_key: true,
                open_ai_compatible: true,
                ..ProviderCapabilities::none()
            },
        }
    }

    /// Checks the endpoint shape before a network layer uses it.
    pub fn validate(&self) -> ProviderResult<()> {
        let base_url = self.base_url.trim();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "base_url must use http or https",
            ));
        }
        if self.id.trim().is_empty() || self.display_name.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                "provider id and display name are required",
            ));
        }
        for endpoint in [
            self.models_endpoint.as_deref(),
            self.balance_endpoint.as_deref(),
            self.usage_endpoint.as_deref(),
            self.quota_endpoint.as_deref(),
            self.reset_endpoint.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !endpoint.starts_with('/') || endpoint.starts_with("//") {
                return Err(ProviderError::new(
                    ProviderErrorKind::ConfigInvalid,
                    "endpoint paths must be relative to base_url",
                ));
            }
        }
        Ok(())
    }

    /// Builds a descriptor without claiming capabilities that have no endpoint.
    pub fn descriptor(&self) -> ProviderDescriptor {
        let mut capabilities = self.capabilities.clone();
        capabilities.model_catalog |= self.models_endpoint.is_some();
        capabilities.balance |= self.balance_endpoint.is_some();
        capabilities.usage |= self.usage_endpoint.is_some();
        capabilities.quota |= self.quota_endpoint.is_some();
        capabilities.reset_at |= self.reset_endpoint.is_some();
        ProviderDescriptor {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            region: self.region.clone(),
            auth_methods: vec![self.auth_method.clone()],
            capabilities,
            default_refresh: self.default_refresh,
            docs_url: self.docs_url.clone(),
        }
    }
}

impl OpenAiCompatibleConfig {
    pub fn parse_response(&self, response: &ProviderResponse) -> ProviderResult<QuotaSnapshotSet> {
        parse_openai_compatible_fixture(response)
    }
}

pub type OpenAICompatibleConfig = OpenAiCompatibleConfig;
pub type OpenAICompatibleDescriptor = OpenAiCompatibleDescriptor;
pub type OpenAiCompatibleProviderConfig = OpenAiCompatibleConfig;
pub type OpenAICompatibleProviderConfig = OpenAiCompatibleConfig;
pub type OpenAiCompatibleProviderDescriptor = OpenAiCompatibleDescriptor;
pub type OpenAICompatibleProviderDescriptor = OpenAiCompatibleDescriptor;

/// A transport-neutral response passed to a provider parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderResponse {
    pub status: u16,
    pub body: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl ProviderResponse {
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
            headers: BTreeMap::new(),
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Parses a generic OpenAI-compatible JSON quota response.
pub fn parse_openai_compatible_response(
    status: u16,
    body: &str,
) -> ProviderResult<QuotaSnapshotSet> {
    parse_openai_compatible_fixture(&ProviderResponse::new(status, body))
}

/// Parses a generic OpenAI-compatible response, including retry headers.
pub fn parse_openai_compatible_fixture(
    response: &ProviderResponse,
) -> ProviderResult<QuotaSnapshotSet> {
    classify_http_status(response)?;
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|error| ProviderError::parser_failed(format!("invalid JSON response: {error}")))?;
    let objects = quota_objects(&value);
    let mut snapshots = Vec::with_capacity(objects.len());
    for object in objects {
        if let Some(snapshot) = snapshot_from_object(object)? {
            snapshots.push(snapshot);
        }
    }
    if snapshots.is_empty() {
        return Err(ProviderError::invalid_response(
            "response contains no recognized quota fields",
        ));
    }
    Ok(QuotaSnapshotSet::new(snapshots))
}

fn classify_http_status(response: &ProviderResponse) -> ProviderResult<()> {
    match response.status {
        200..=299 => Ok(()),
        401 | 403 => Err(ProviderError::auth_required(response.status)),
        408 => Err(
            ProviderError::new(ProviderErrorKind::Timeout, "provider request timed out")
                .with_status(response.status),
        ),
        429 => {
            let retry_at = response.header("retry-after").map(ToOwned::to_owned);
            Err(ProviderError::rate_limited(response.status, retry_at))
        }
        500..=599 => Err(ProviderError::new(
            ProviderErrorKind::ProviderUnavailable,
            "provider returned a server error",
        )
        .with_status(response.status)),
        status => Err(ProviderError::new(
            ProviderErrorKind::InvalidResponse,
            format!("unexpected provider HTTP status {status}"),
        )
        .with_status(status)),
    }
}

fn quota_objects(value: &Value) -> Vec<&Map<String, Value>> {
    match value {
        Value::Object(object) => match object.get("data") {
            Some(Value::Object(data)) => vec![data],
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
            _ => vec![object],
        },
        Value::Array(items) => items.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn snapshot_from_object(object: &Map<String, Value>) -> ProviderResult<Option<QuotaSnapshot>> {
    let used = number(object, &["used", "usage", "consumed"])?;
    let limit = number(object, &["limit", "max", "quota"])?;
    let remaining = number(object, &["remaining", "left", "balance"])?;
    let reset_at = string(object, &["reset_at", "resetAt", "resets_at", "resetsAt"]);
    let recognized = used.is_some() || limit.is_some() || remaining.is_some() || reset_at.is_some();
    if !recognized {
        return Ok(None);
    }

    let status = if used.is_some() && limit.is_some() {
        SnapshotStatus::Available
    } else {
        SnapshotStatus::Partial
    };
    let unit = string(object, &["unit", "metric_unit"]).and_then(|value| match value.as_str() {
        "token" | "tokens" => Some(QuotaUnit::Token),
        "credit" | "credits" => Some(QuotaUnit::Credit),
        "request" | "requests" => Some(QuotaUnit::Request),
        "currency" | "usd" | "dollar" | "dollars" => Some(QuotaUnit::Currency),
        "percent" | "%" => Some(QuotaUnit::Percent),
        _ => None,
    });
    let window = if reset_at.is_some() {
        Some(QuotaWindow {
            reset_at: reset_at.clone(),
            ..QuotaWindow::default()
        })
    } else {
        None
    };
    Ok(Some(QuotaSnapshot {
        id: string(object, &["id", "metric_id", "metricId"]),
        account_id: string(object, &["account_id", "accountId"]),
        metric_id: string(object, &["metric_id", "metricId"]),
        metric_name: string(object, &["metric_name", "metricName", "name"]),
        unit,
        currency: string(object, &["currency"]),
        used,
        limit,
        remaining,
        window,
        reset_at,
        observed_at: string(object, &["observed_at", "observedAt"]),
        source: DataSource::CustomEndpoint,
        confidence: Confidence::Exact,
        status,
    }))
}

fn number(object: &Map<String, Value>, keys: &[&str]) -> ProviderResult<Option<f64>> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            return match value {
                Value::Null => Ok(None),
                Value::Number(number) => number
                    .as_f64()
                    .ok_or_else(|| {
                        ProviderError::parser_failed(format!("field {key} is not a finite number"))
                    })
                    .map(Some),
                Value::String(value) => value.parse::<f64>().map(Some).map_err(|_| {
                    ProviderError::parser_failed(format!("field {key} is not a number"))
                }),
                _ => Err(ProviderError::parser_failed(format!(
                    "field {key} must be a number"
                ))),
            };
        }
    }
    Ok(None)
}

fn string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match object.get(*key) {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

/// A descriptor handle stored by the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHandle {
    pub descriptor: ProviderDescriptor,
    pub open_ai_compatible: Option<OpenAiCompatibleConfig>,
}

impl ProviderHandle {
    pub fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }
}

/// Registry lookup for built-in and user-defined provider descriptors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, ProviderHandle>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, descriptor: ProviderDescriptor) -> Option<ProviderHandle> {
        self.providers.insert(
            descriptor.id.clone(),
            ProviderHandle {
                descriptor,
                open_ai_compatible: None,
            },
        )
    }

    pub fn register_open_ai_compatible(
        &mut self,
        config: OpenAiCompatibleConfig,
    ) -> ProviderResult<Option<ProviderHandle>> {
        config.validate()?;
        let descriptor = config.descriptor();
        Ok(self.providers.insert(
            descriptor.id.clone(),
            ProviderHandle {
                descriptor,
                open_ai_compatible: Some(config),
            },
        ))
    }

    pub fn lookup(&self, id: &str) -> Option<&ProviderHandle> {
        self.providers.get(id)
    }

    pub fn get(&self, id: &str) -> Option<&ProviderHandle> {
        self.lookup(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<ProviderHandle> {
        self.providers.remove(id)
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ProviderHandle)> {
        self.providers
            .iter()
            .map(|(id, handle)| (id.as_str(), handle))
    }
}

impl std::ops::Deref for ProviderHandle {
    type Target = ProviderDescriptor;

    fn deref(&self) -> &Self::Target {
        &self.descriptor
    }
}

impl<'a> IntoIterator for &'a ProviderRegistry {
    type Item = (&'a str, &'a ProviderHandle);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUCCESS: &str = r#"{
        "data": {
            "id": "tokens-5h",
            "metric_name": "5 hour tokens",
            "unit": "token",
            "used": 2500,
            "limit": 10000,
            "remaining": 7500,
            "reset_at": "2026-10-05T12:00:00Z"
        }
    }"#;

    const PARTIAL: &str = r#"{
        "data": {
            "metric_name": "account balance",
            "remaining": 42.5,
            "unit": "credit"
        }
    }"#;

    #[test]
    fn parses_success_fixture_with_all_quota_values() {
        let set = parse_openai_compatible_response(200, SUCCESS).expect("success fixture");
        let snapshot = &set.snapshots[0];
        assert_eq!(snapshot.used, Some(2500.0));
        assert_eq!(snapshot.limit, Some(10000.0));
        assert_eq!(snapshot.remaining, Some(7500.0));
        assert_eq!(snapshot.reset_at.as_deref(), Some("2026-10-05T12:00:00Z"));
        assert_eq!(snapshot.status, SnapshotStatus::Available);
        assert_eq!(snapshot.unit, Some(QuotaUnit::Token));
    }

    #[test]
    fn parses_partial_fixture_without_inventing_missing_values() {
        let set = parse_openai_compatible_response(200, PARTIAL).expect("partial fixture");
        let snapshot = &set.snapshots[0];
        assert_eq!(snapshot.used, None);
        assert_eq!(snapshot.limit, None);
        assert_eq!(snapshot.remaining, Some(42.5));
        assert_eq!(snapshot.reset_at, None);
        assert_eq!(snapshot.status, SnapshotStatus::Partial);
    }

    #[test]
    fn malformed_fixture_is_a_parser_error() {
        let error =
            parse_openai_compatible_response(200, "{not-json").expect_err("malformed fixture");
        assert_eq!(error.code, ProviderErrorKind::ParserFailed);
        assert!(!error.retryable);
    }

    #[test]
    fn unauthorized_fixture_is_structured_auth_error() {
        let error = parse_openai_compatible_response(401, r#"{"error":"unauthorized"}"#)
            .expect_err("unauthorized fixture");
        assert_eq!(error.code, ProviderErrorKind::AuthRequired);
        assert_eq!(error.http_status, Some(401));
        assert!(!error.retryable);
    }

    #[test]
    fn rate_limited_fixture_carries_retry_after() {
        let response = ProviderResponse::new(429, r#"{"error":"rate limited"}"#)
            .with_header("Retry-After", "30");
        let error = parse_openai_compatible_fixture(&response).expect_err("rate limit fixture");
        assert_eq!(error.code, ProviderErrorKind::RateLimited);
        assert_eq!(error.http_status, Some(429));
        assert_eq!(error.retry_at.as_deref(), Some("30"));
        assert!(error.retryable);
    }

    #[test]
    fn empty_or_unrecognized_fixture_is_invalid_response() {
        let error =
            parse_openai_compatible_response(200, r#"{"data":[]}"#).expect_err("empty fixture");
        assert_eq!(error.code, ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn config_descriptor_claims_only_declared_endpoint_capabilities() {
        let mut config = OpenAiCompatibleConfig::new(
            "custom.gateway",
            "Custom Gateway",
            "https://gateway.example/v1",
        );
        config.usage_endpoint = Some("/usage".to_owned());
        let descriptor = config.descriptor();
        assert!(descriptor.capabilities.open_ai_compatible);
        assert!(descriptor.capabilities.model_catalog);
        assert!(descriptor.capabilities.usage);
        assert!(!descriptor.capabilities.quota);
        assert!(!descriptor.capabilities.balance);
    }

    #[test]
    fn registry_replaces_and_looks_up_by_stable_id() {
        let mut registry = ProviderRegistry::new();
        let descriptor = ProviderDescriptor::new("fixture.provider", "Fixture Provider");
        assert!(registry.register(descriptor).is_none());
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry
                .lookup("fixture.provider")
                .map(|handle| handle.descriptor.display_name.as_str()),
            Some("Fixture Provider")
        );
    }
}
