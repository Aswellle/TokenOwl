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
///
/// The boolean fields mirror the provider protocol's capability matrix, so the
/// pedantic "too many bools" lint does not apply here.
#[allow(clippy::struct_excessive_bools)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaUnit {
    #[default]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataSource {
    OfficialApi,
    OAuthApi,
    BrowserSession,
    Cookie,
    CliLocal,
    LocalDb,
    #[default]
    CustomEndpoint,
    Manual,
    Cached,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Confidence {
    Exact,
    Derived,
    Estimated,
    Stale,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SnapshotStatus {
    Available,
    Partial,
    #[default]
    Unavailable,
    Stale,
    Error,
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

impl<'a> IntoIterator for &'a QuotaSnapshotSet {
    type Item = &'a QuotaSnapshot;
    type IntoIter = std::slice::Iter<'a, QuotaSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
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
    /// Manifest-declared field mappings; `None` means the built-in lenient
    /// field-name search applies. Mapping evaluation happens only through
    /// `parse_openai_compatible_with_mappings`.
    pub field_mappings: Option<FieldMappings>,
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
            field_mappings: None,
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
        for (field, endpoint) in [
            ("models", self.models_endpoint.as_deref()),
            ("balance", self.balance_endpoint.as_deref()),
            ("usage", self.usage_endpoint.as_deref()),
            ("quota", self.quota_endpoint.as_deref()),
            ("reset", self.reset_endpoint.as_deref()),
        ] {
            if let Some(endpoint) = endpoint {
                return validate_relative_endpoint_path(field, endpoint).map_err(|reason| {
                    ProviderError::new(ProviderErrorKind::ConfigInvalid, reason)
                });
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

/// The one set of relative-endpoint rules, shared by manifest registration
/// and saved account configs so the two boundaries can never disagree.
///
/// A valid endpoint is a non-empty relative path rooted at the base URL:
/// it must start with a single `/`, and must not escape upward (`..`),
/// hide an absolute target (`//host`), smuggle a second URL component
/// (backslash, query, or fragment).
pub(crate) fn validate_relative_endpoint_path(field: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if !value.starts_with('/') || value.starts_with("//") {
        return Err(format!(
            "{field} must be a relative path such as /usage (found `{value}`)"
        ));
    }
    if value.contains('\\') {
        return Err(format!("{field} must not contain backslashes"));
    }
    if value.contains('?') || value.contains('#') {
        return Err(format!("{field} must not contain a query or fragment"));
    }
    if value.split('/').any(|segment| segment == "..") {
        return Err(format!(
            "{field} must not traverse outside the base URL (`..`)"
        ));
    }
    Ok(())
}

/// Header-name rules shared by manifest registration: a bare token with no
/// whitespace, colon, or control characters. The header VALUE is the
/// secret and never passes through here.
pub(crate) fn validate_header_name(field: &str, value: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{field} must be 1-128 header-name characters (letters, digits, `-` `_` `.`)"
        ))
    }
}

/// A transport-neutral response passed to a provider parser.
///
/// Deliberately neither `Serialize` nor `Deserialize`: nothing crosses the
/// IPC boundary as a raw response — parsers consume it by reference inside
/// Rust — and keeping serde off the type prevents a future DTO from
/// accidentally embedding provider payloads.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    pub status: u16,
    pub body: String,
    pub headers: BTreeMap<String, String>,
}

impl std::fmt::Debug for ProviderResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The body is the raw provider payload; `Debug` reaches logs and
        // error context, so only size metadata may appear.
        formatter
            .debug_struct("ProviderResponse")
            .field("status", &self.status)
            .field("body", &format!("<{} bytes redacted>", self.body.len()))
            .finish_non_exhaustive()
    }
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
    parse_openai_compatible_lenient(response)?.ok_or_else(|| {
        ProviderError::invalid_response("response contains no recognized quota fields")
    })
}

/// Parses a response into snapshots, returning `Ok(None)` when the payload is
/// valid but carries no recognizable quota fields. Multi-endpoint callers use
/// this so one endpoint without quota fields does not fail the whole refresh.
/// A field mapping resolved from a provider manifest: a dot-path into the
/// response payload plus optional unit/currency overrides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMapping {
    pub value: String,
    pub unit: Option<String>,
    pub currency: Option<String>,
    /// Which snapshot slot the mapped value feeds: `used` (default),
    /// `remaining`, or `limit`. Providers report different sides of the same
    /// figure — `OpenRouter` hands out remaining credits, others spent.
    #[serde(default)]
    pub role: Option<String>,
    /// Display name for a `metrics` entry (`充值余额`, `赠送余额`, …).
    #[serde(default)]
    pub name: Option<String>,
}

/// Quota-bearing field mappings a manifest may declare.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMappings {
    pub balance: Option<FieldMapping>,
    pub usage: Option<FieldMapping>,
    /// Additional named value mappings read from the SAME response — for
    /// providers that report several account figures at once (`DeepSeek`'s
    /// granted/topped-up split, `SiliconFlow`'s balance/charge/total). Each
    /// entry becomes its own snapshot labeled by `name`.
    #[serde(default)]
    pub metrics: Vec<FieldMapping>,
}

impl FieldMapping {
    /// Evaluates the dot-path against a JSON value; array indices are
    /// supported as path segments (`data.0.total_used`).
    fn lookup<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        let mut current = value;
        for segment in self.value.split('.') {
            current = match current {
                Value::Object(map) => map.get(segment)?,
                Value::Array(items) => {
                    let index: usize = segment.parse().ok()?;
                    items.get(index)?
                }
                _ => return None,
            };
        }
        Some(current)
    }

    fn as_f64(&self, value: &Value) -> Option<f64> {
        let raw = self.lookup(value)?;
        // An explicit mapping is a semantic declaration by the manifest
        // author, so numeric strings ("42.5") are accepted here even though
        // the name-based search would reject them.
        match raw {
            Value::Number(number) => number.as_f64().filter(|number| number.is_finite()),
            Value::String(text) => text.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
            _ => None,
        }
    }
}

/// Parses a response using explicit manifest field mappings. Each mapping
/// yields at most one snapshot; mappings that resolve to nothing are skipped
/// rather than zero-filled. Mapped snapshots carry `PARTIAL` status unless
/// both used and limit are mapped.
pub fn parse_openai_compatible_with_mappings(
    response: &ProviderResponse,
    mappings: &FieldMappings,
) -> ProviderResult<Option<QuotaSnapshotSet>> {
    classify_http_status(response)?;
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|error| ProviderError::parser_failed(format!("invalid JSON response: {error}")))?;

    let build = |mapping: &FieldMapping, metric: &str| -> Option<QuotaSnapshot> {
        let value = mapping.as_f64(&value);
        value.and_then(|value| {
            let unit = mapping.unit.as_deref().and_then(|unit| match unit {
                "token" | "tokens" => Some(QuotaUnit::Token),
                "credit" | "credits" => Some(QuotaUnit::Credit),
                "request" | "requests" => Some(QuotaUnit::Request),
                "currency" | "usd" => Some(QuotaUnit::Currency),
                "percent" => Some(QuotaUnit::Percent),
                // An unrecognized unit is not silently re-labeled: the
                // mapping is skipped so the UI shows no invented metric.
                _ => None,
            })?;
            let mut snapshot = QuotaSnapshot::unknown();
            snapshot.id = Some(format!("{metric}-mapped"));
            snapshot.metric_id = Some(metric.to_owned());
            snapshot.metric_name = Some(mapping.name.clone().unwrap_or_else(|| metric.to_owned()));
            snapshot.unit = Some(unit);
            snapshot.currency.clone_from(&mapping.currency);
            // The mapping author picks the slot: a "remaining" mapping must
            // not masquerade as consumption, or a 88.88 CNY balance would
            // read as "used 88.88 of nothing".
            match mapping.role.as_deref() {
                Some("remaining") => snapshot.remaining = Some(value),
                Some("limit") => snapshot.limit = Some(value),
                _ => snapshot.used = Some(value),
            }
            snapshot.status = SnapshotStatus::Partial;
            snapshot.source = DataSource::CustomEndpoint;
            Some(snapshot)
        })
    };

    let mut snapshots = Vec::new();
    if let Some(balance) = mappings.balance.as_ref() {
        snapshots.extend(build(balance, "balance"));
    }
    if let Some(usage) = mappings.usage.as_ref() {
        snapshots.extend(build(usage, "usage"));
    }
    for metric in &mappings.metrics {
        snapshots.extend(build(metric, metric.name.as_deref().unwrap_or("metric")));
    }
    if snapshots.is_empty() {
        return Ok(None);
    }
    Ok(Some(QuotaSnapshotSet::new(snapshots)))
}

pub fn parse_openai_compatible_lenient(
    response: &ProviderResponse,
) -> ProviderResult<Option<QuotaSnapshotSet>> {
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
        return Ok(None);
    }
    Ok(Some(QuotaSnapshotSet::new(snapshots)))
}

/// Parses the balance envelope several providers use for their billing
/// endpoint, most notably `DeepSeek`:
///
/// ```json
/// { "is_available": false,
///   "balance_infos": [{ "currency": "CNY", "total_balance": "-0.23" }] }
/// ```
///
/// The generic name-based parser only looks at top-level `remaining`/`balance`
/// fields, so a built-in provider that declares a balance endpoint would
/// otherwise fail every refresh despite a valid 200 response. Each currency
/// becomes one currency snapshot; an entry without a numeric `total_balance`
/// is skipped rather than zero-filled.
pub fn parse_balance_response(
    response: &ProviderResponse,
) -> ProviderResult<Option<QuotaSnapshotSet>> {
    classify_http_status(response)?;
    let value: Value = serde_json::from_str(&response.body)
        .map_err(|error| ProviderError::parser_failed(format!("invalid JSON response: {error}")))?;
    let Some(infos) = value.get("balance_infos").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut snapshots = Vec::with_capacity(infos.len());
    for info in infos {
        let Some(total) = info.get("total_balance").and_then(value_as_f64) else {
            continue;
        };
        let currency = info
            .get("currency")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let suffix = currency.clone().unwrap_or_else(|| "unknown".to_owned());
        let mut snapshot = QuotaSnapshot::unknown();
        snapshot.id = Some(format!("balance-{suffix}"));
        snapshot.metric_id = Some("balance".to_owned());
        snapshot.metric_name = Some("balance".to_owned());
        snapshot.unit = Some(QuotaUnit::Currency);
        snapshot.currency.clone_from(&currency);
        snapshot.remaining = Some(total);
        snapshot.status = SnapshotStatus::Available;
        snapshot.source = DataSource::CustomEndpoint;
        snapshots.push(snapshot);
        // The official envelope also splits the total into granted (gift)
        // and topped-up parts; surface both when the provider reports them
        // so a user can see how much of the balance actually expires.
        for (field, name) in [
            ("granted_balance", "赠送余额"),
            ("topped_up_balance", "充值余额"),
        ] {
            let Some(part) = info.get(field).and_then(value_as_f64) else {
                continue;
            };
            let mut part_snapshot = QuotaSnapshot::unknown();
            part_snapshot.id = Some(format!("balance-{field}-{suffix}"));
            part_snapshot.metric_id = Some(field.to_owned());
            part_snapshot.metric_name = Some(name.to_owned());
            part_snapshot.unit = Some(QuotaUnit::Currency);
            part_snapshot.currency.clone_from(&currency);
            part_snapshot.remaining = Some(part);
            part_snapshot.status = SnapshotStatus::Partial;
            part_snapshot.source = DataSource::CustomEndpoint;
            snapshots.push(part_snapshot);
        }
    }
    if snapshots.is_empty() {
        return Ok(None);
    }
    Ok(Some(QuotaSnapshotSet::new(snapshots)))
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|number| number.is_finite()),
        Value::String(text) => text.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
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
    // Window identity is quota data too: a window with no numbers still earns
    // an honest unknown snapshot instead of being dropped (the same rule
    // `reset_at` follows).
    let starts_at = string(object, &["starts_at", "startsAt"]);
    let ends_at = string(object, &["ends_at", "endsAt"]);
    let recognized = used.is_some()
        || limit.is_some()
        || remaining.is_some()
        || reset_at.is_some()
        || starts_at.is_some()
        || ends_at.is_some();
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
    let window = if reset_at.is_some() || starts_at.is_some() || ends_at.is_some() {
        Some(QuotaWindow {
            starts_at,
            ends_at,
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

/// 账户上下文，传递给 `ProviderAdapter` 方法。
#[derive(Debug, Clone)]
pub struct AccountContext {
    pub account_id: String,
    pub provider_id: String,
    pub credential_ref: Option<String>,
}

impl AccountContext {
    pub fn new(account_id: impl Into<String>, provider_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            provider_id: provider_id.into(),
            credential_ref: None,
        }
    }

    pub fn with_credential_ref(mut self, credential_ref: impl Into<String>) -> Self {
        self.credential_ref = Some(credential_ref.into());
        self
    }
}

/// Fetch 上下文，包含请求级别配置。
#[derive(Debug, Clone)]
pub struct FetchContext {
    pub timeout: Duration,
    pub base_url: Option<String>,
    pub credentials: Option<crate::credentials::CredentialStore>,
    /// The account's own provider config. The refresh path always supplies it,
    /// and adapters must prefer it over the config they were constructed with:
    /// `AppRuntime` builds the native adapters from
    /// `OpenAiCompatibleConfig::default()`, whose base URL is a placeholder,
    /// so reading the construction config would target the wrong host.
    pub config: Option<OpenAiCompatibleConfig>,
}

impl Default for FetchContext {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            base_url: None,
            credentials: None,
            config: None,
        }
    }
}

/// 账户身份信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountIdentity {
    pub account_id: String,
    pub provider_id: String,
    pub display_name: Option<String>,
}

/// 使用范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

/// 单个使用数据点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePoint {
    pub id: Option<String>,
    pub account_id: String,
    pub model_id: Option<String>,
    pub metric_type: String,
    pub quantity: f64,
    pub unit: QuotaUnit,
    pub cost: Option<f64>,
    pub currency: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub source: DataSource,
    pub observed_at: String,
}

/// 健康检查结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    #[default]
    Unknown,
}

/// Provider 适配器 trait，所有 Provider 需实现此接口。
#[async_trait::async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// 返回 Provider 描述符。
    fn descriptor(&self) -> ProviderDescriptor;

    /// 验证账户凭证有效性。
    async fn validate_account(&self, account: &AccountContext) -> ProviderResult<AccountIdentity>;

    /// 获取配额快照集合。
    async fn fetch_quota(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<QuotaSnapshotSet>;

    /// 获取使用数据点。
    async fn fetch_usage(
        &self,
        account: &AccountContext,
        range: UsageRange,
        ctx: &FetchContext,
    ) -> ProviderResult<Vec<UsagePoint>>;

    /// 健康检查。
    async fn health_check(
        &self,
        account: &AccountContext,
        ctx: &FetchContext,
    ) -> ProviderResult<HealthStatus>;
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

    #[test]
    fn account_context_new_sets_ids() {
        let ctx = AccountContext::new("acct-1", "openai");
        assert_eq!(ctx.account_id, "acct-1");
        assert_eq!(ctx.provider_id, "openai");
        assert!(ctx.credential_ref.is_none());
    }

    #[test]
    fn account_context_with_credential_ref_chains() {
        let ctx = AccountContext::new("acct-1", "openai")
            .with_credential_ref("keyring://tokenowl/acct-1");
        assert_eq!(
            ctx.credential_ref.as_deref(),
            Some("keyring://tokenowl/acct-1")
        );
    }

    #[test]
    fn fetch_context_default_has_20s_timeout() {
        let ctx = FetchContext::default();
        assert_eq!(ctx.timeout, std::time::Duration::from_secs(20));
        assert!(ctx.base_url.is_none());
    }

    #[test]
    fn health_status_default_is_unknown() {
        assert_eq!(HealthStatus::default(), HealthStatus::Unknown);
    }
    #[test]
    fn mapped_parse_reads_dot_paths_and_applies_unit_currency() {
        let response = ProviderResponse::new(
            200,
            r#"{"result":{"balance":"42.5","currency":"CNY"},"usage":{"totalTokens":1234}}"#,
        );
        let mappings = FieldMappings {
            balance: Some(FieldMapping {
                value: "result.balance".to_owned(),
                unit: Some("currency".to_owned()),
                currency: Some("CNY".to_owned()),
                role: None,
                name: None,
            }),
            usage: Some(FieldMapping {
                value: "usage.totalTokens".to_owned(),
                unit: Some("token".to_owned()),
                currency: None,
                role: None,
                name: None,
            }),
            metrics: Vec::new(),
        };
        let set = parse_openai_compatible_with_mappings(&response, &mappings)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.len(), 2);
        let balance = set
            .snapshots
            .iter()
            .find(|snapshot| snapshot.metric_id.as_deref() == Some("balance"))
            .expect("balance snapshot");
        assert_eq!(balance.used, Some(42.5));
        assert_eq!(balance.unit, Some(QuotaUnit::Currency));
        assert_eq!(balance.currency.as_deref(), Some("CNY"));
        let usage = set
            .snapshots
            .iter()
            .find(|snapshot| snapshot.metric_id.as_deref() == Some("usage"))
            .expect("usage snapshot");
        assert_eq!(usage.used, Some(1_234.0));
        assert_eq!(usage.unit, Some(QuotaUnit::Token));
        assert_eq!(usage.status, SnapshotStatus::Partial);
    }

    #[test]
    fn mapped_parse_supports_array_indices_and_skips_unresolved() {
        let response =
            ProviderResponse::new(200, r#"{"data":[{"remaining":7.5},{"remaining":99.0}]}"#);
        let mappings = FieldMappings {
            balance: Some(FieldMapping {
                value: "data.1.remaining".to_owned(),
                unit: Some("credit".to_owned()),
                currency: None,
                role: None,
                name: None,
            }),
            usage: Some(FieldMapping {
                value: "missing.path".to_owned(),
                unit: Some("token".to_owned()),
                currency: None,
                role: None,
                name: None,
            }),
            metrics: Vec::new(),
        };
        let set = parse_openai_compatible_with_mappings(&response, &mappings)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.len(), 1);
        assert_eq!(set.snapshots[0].used, Some(99.0));
        assert_eq!(set.snapshots[0].unit, Some(QuotaUnit::Credit));
    }

    #[test]
    fn mapped_parse_unknown_unit_skips_and_non_numeric_is_ignored() {
        let response = ProviderResponse::new(200, r#"{"b":"lots","u":"12"}"#);
        let mappings = FieldMappings {
            balance: Some(FieldMapping {
                value: "b".to_owned(),
                unit: Some("widgets".to_owned()),
                currency: None,
                role: None,
                name: None,
            }),
            usage: Some(FieldMapping {
                value: "u".to_owned(),
                unit: Some("token".to_owned()),
                currency: None,
                role: None,
                name: None,
            }),
            metrics: Vec::new(),
        };
        // "lots" is not numeric -> balance skipped; "12" is a numeric string
        // and an explicit mapping declares its semantics, so usage parses.
        let set = parse_openai_compatible_with_mappings(&response, &mappings)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.len(), 1);
        assert_eq!(set.snapshots[0].metric_id.as_deref(), Some("usage"));
        assert_eq!(set.snapshots[0].used, Some(12.0));
    }

    #[test]
    fn parses_the_deepseek_balance_envelope() {
        // Captured from the real DeepSeek `/user/balance` response: the value
        // is a negative remaining balance in a currency, with no limit.
        let response = ProviderResponse::new(
            200,
            r#"{"is_available":false,"balance_infos":[{"currency":"CNY","total_balance":"-0.23","granted_balance":"0.00","topped_up_balance":"-0.23"}]}"#,
        );
        let set = parse_balance_response(&response)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.len(), 3, "total + granted + topped-up");
        let snapshot = &set.snapshots[0];
        assert_eq!(snapshot.remaining, Some(-0.23));
        assert_eq!(snapshot.unit, Some(QuotaUnit::Currency));
        assert_eq!(snapshot.currency.as_deref(), Some("CNY"));
        assert_eq!(snapshot.metric_name.as_deref(), Some("balance"));
        assert_eq!(snapshot.status, SnapshotStatus::Available);
        // The official envelope's granted/topped-up split surfaces as its own
        // labeled metrics (0 granted is real data and stays).
        let granted = &set.snapshots[1];
        assert_eq!(granted.metric_name.as_deref(), Some("赠送余额"));
        assert_eq!(granted.remaining, Some(0.0));
        assert_eq!(granted.status, SnapshotStatus::Partial);
        let topped = &set.snapshots[2];
        assert_eq!(topped.metric_name.as_deref(), Some("充值余额"));
        assert_eq!(topped.remaining, Some(-0.23));
    }

    #[test]
    fn mappings_honor_roles_and_emit_named_metrics() {
        // OpenRouter-shaped: the key endpoint reports usage spent plus the
        // remaining credits; the remaining figure must land in the remaining
        // slot, and the extra named metrics must keep their own labels.
        let response = ProviderResponse::new(
            200,
            r#"{"data":{"usage":0.11,"limit":50,"limit_remaining":49.89,"usage_daily":0.02}}"#,
        );
        let mappings = FieldMappings {
            usage: Some(FieldMapping {
                value: "data.usage".to_owned(),
                unit: Some("currency".to_owned()),
                currency: Some("USD".to_owned()),
                role: Some("used".to_owned()),
                name: None,
            }),
            metrics: vec![
                FieldMapping {
                    value: "data.limit_remaining".to_owned(),
                    unit: Some("currency".to_owned()),
                    currency: Some("USD".to_owned()),
                    role: Some("remaining".to_owned()),
                    name: Some("剩余额度".to_owned()),
                },
                FieldMapping {
                    value: "data.usage_daily".to_owned(),
                    unit: Some("currency".to_owned()),
                    currency: Some("USD".to_owned()),
                    role: Some("used".to_owned()),
                    name: Some("今日已用".to_owned()),
                },
            ],
            balance: None,
        };
        let set = parse_openai_compatible_with_mappings(&response, &mappings)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.snapshots.len(), 3);
        let used = &set.snapshots[0];
        assert_eq!(used.metric_name.as_deref(), Some("usage"));
        assert_eq!(used.used, Some(0.11));
        assert_eq!(used.remaining, None);
        let remaining = &set.snapshots[1];
        assert_eq!(remaining.metric_name.as_deref(), Some("剩余额度"));
        assert_eq!(remaining.remaining, Some(49.89));
        assert_eq!(
            remaining.used, None,
            "a remaining figure is not consumption"
        );
        let daily = &set.snapshots[2];
        assert_eq!(daily.metric_name.as_deref(), Some("今日已用"));
        assert_eq!(daily.used, Some(0.02));
    }

    #[test]
    fn balance_parser_handles_multiple_currencies_and_skips_unusable_entries() {
        let response = ProviderResponse::new(
            200,
            r#"{"balance_infos":[{"currency":"CNY","total_balance":12.5},{"currency":"USD","total_balance":"3.25"},{"currency":"EUR"}]}"#,
        );
        let set = parse_balance_response(&response)
            .expect("parse")
            .expect("snapshots");
        assert_eq!(set.len(), 2);
        assert_eq!(set.snapshots[0].id.as_deref(), Some("balance-CNY"));
        assert_eq!(set.snapshots[0].remaining, Some(12.5));
        assert_eq!(set.snapshots[1].id.as_deref(), Some("balance-USD"));
        assert_eq!(set.snapshots[1].remaining, Some(3.25));
    }

    #[test]
    fn balance_parser_ignores_unrelated_json() {
        let response = ProviderResponse::new(200, r#"{"remaining":42}"#);
        assert!(
            parse_balance_response(&response).expect("parse").is_none(),
            "a non-balance envelope must fall back to the generic parser"
        );
    }
}
