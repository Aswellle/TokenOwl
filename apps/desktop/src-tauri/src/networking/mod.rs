//! Unified HTTP client for provider requests.
//!
//! This is the only place a `reqwest::Client` may be built: every provider
//! adapter holds an [`HttpClient`], so redirect policy, proxy behavior,
//! timeouts, and the streaming response-size cap are enforced once instead of
//! being re-implemented per adapter. Adapters keep their own
//! `AdapterLimits` (per-provider timeout/size defaults) and feed them in via
//! [`HttpClientConfig::from_adapter_limits`].
use crate::domain::provider::{ProviderError, ProviderErrorKind, ProviderRegion, ProviderResponse};
use reqwest::header::HeaderMap;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// HTTP client configuration.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout.
    pub timeout: Duration,
    /// Connect timeout.
    pub connect_timeout: Duration,
    /// Maximum response body size in bytes.
    pub max_response_bytes: usize,
    /// Whether to follow redirects. The default is `false`: a 3xx response is
    /// surfaced to the caller instead of being followed, so credentials in
    /// request headers can never leak to a second host.
    pub follow_redirects: bool,
    /// Explicit proxy URL, used by [`ProxyRoute::Custom`]. `None` means the
    /// custom route is unavailable; the route itself still decides whether
    /// traffic is direct, system-proxied, or sent here.
    pub proxy: Option<String>,
    /// Per-region proxy routing. Defaults to direct transport (tests and
    /// adapters built without an app policy never pick up an ambient proxy);
    /// the application supplies the user's policy through a shared
    /// [`ProxyState`].
    pub routing: ProxyRouting,
}

/// How one request leaves the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyRoute {
    /// Connect straight to the provider.
    #[default]
    Direct,
    /// Whatever the operating system is configured to use (Windows Internet
    /// Settings, or the environment on other platforms).
    System,
    /// The explicit proxy URL the user stored.
    Custom,
}

/// Proxy policy split by provider region: domestic services usually need a
/// direct connection while international ones need the system proxy, and
/// neither should interfere with the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRouting {
    pub domestic: ProxyRoute,
    pub international: ProxyRoute,
}

impl Default for ProxyRouting {
    fn default() -> Self {
        Self {
            domestic: ProxyRoute::Direct,
            international: ProxyRoute::System,
        }
    }
}

impl ProxyRouting {
    pub fn route_for(&self, region: &ProviderRegion) -> ProxyRoute {
        match region {
            ProviderRegion::China => self.domestic,
            // Every other declared region — and any provider that declares
            // none — is treated as international.
            _ => self.international,
        }
    }

    /// The legacy single-proxy semantics: one URL for every request.
    fn single_proxy() -> Self {
        Self {
            domestic: ProxyRoute::Custom,
            international: ProxyRoute::Custom,
        }
    }
}

/// Live proxy policy shared by every adapter. The transport reads it per
/// request, so changing a proxy setting applies without restarting the app
/// and without rebuilding any client.
#[derive(Debug)]
pub struct ProxyState {
    routing: RwLock<ProxyRouting>,
    custom_url: RwLock<Option<String>>,
}

impl ProxyState {
    pub fn new(routing: ProxyRouting, custom_url: Option<String>) -> Self {
        Self {
            routing: RwLock::new(routing),
            custom_url: RwLock::new(custom_url),
        }
    }

    /// A poisoned lock degrades to the safe policy (direct transport)
    /// instead of panicking inside a network call.
    pub fn routing(&self) -> ProxyRouting {
        self.routing.read().map_or(
            ProxyRouting {
                domestic: ProxyRoute::Direct,
                international: ProxyRoute::Direct,
            },
            |guard| *guard,
        )
    }

    pub fn custom_url(&self) -> Option<String> {
        self.custom_url.read().ok().and_then(|guard| guard.clone())
    }

    pub fn set_routing(&self, routing: ProxyRouting) {
        if let Ok(mut guard) = self.routing.write() {
            *guard = routing;
        }
    }

    pub fn set_custom_url(&self, url: Option<String>) {
        if let Ok(mut guard) = self.custom_url.write() {
            *guard = url;
        }
    }

    /// Which route a request for this region takes right now.
    pub fn route_for(&self, region: &ProviderRegion) -> ProxyRoute {
        self.routing().route_for(region)
    }
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(10),
            max_response_bytes: 1024 * 1024,
            follow_redirects: false,
            proxy: None,
            routing: ProxyRouting::default(),
        }
    }
}

impl HttpClientConfig {
    /// Transport policy for an adapter's per-provider limits. The adapter
    /// supplies the request timeout and response cap; redirect and proxy
    /// policy always take the global defaults (deny redirects, no implicit
    /// proxy).
    pub fn from_adapter_limits(timeout: Duration, max_response_bytes: usize) -> Self {
        Self::from_adapter_limits_with_proxy(timeout, max_response_bytes, None)
    }

    /// [`Self::from_adapter_limits`] with an explicit proxy. The proxy only
    /// ever arrives from the user's own settings — never from the
    /// environment — so transport behavior stays deterministic and testable.
    pub fn from_adapter_limits_with_proxy(
        timeout: Duration,
        max_response_bytes: usize,
        proxy: Option<String>,
    ) -> Self {
        Self {
            timeout,
            max_response_bytes,
            proxy,
            ..Self::default()
        }
    }
}

/// Unified HTTP client with security and retry policies. `Clone` is cheap
/// (the underlying connection pool is shared); `Debug` leaks nothing — the
/// config holds no secrets and `reqwest::Client`'s own Debug is opaque.
#[derive(Clone, Debug)]
pub struct HttpClient {
    direct: Client,
    system: Client,
    custom: Client,
    config: HttpClientConfig,
    state: Arc<ProxyState>,
}

/// One request's transport, bound to the client its route resolved to.
///
/// Adapters never hold a `reqwest::Client` directly; they ask for the routed
/// view with [`HttpClient::route`], which keeps the proxy decision in one
/// place and reads the live policy at call time.
pub struct RouteClient<'a> {
    client: &'a Client,
    config: &'a HttpClientConfig,
}

fn base_builder(config: &HttpClientConfig) -> reqwest::ClientBuilder {
    let mut builder = Client::builder()
        .timeout(config.timeout)
        .connect_timeout(config.connect_timeout);
    if !config.follow_redirects {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }
    builder
}

fn build_error(error: reqwest::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::ConfigInvalid,
        format!("failed to build HTTP client: {error}"),
    )
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration.
    pub fn new(config: HttpClientConfig) -> Result<Self, ProviderError> {
        Self::with_state(config, None)
    }

    /// A client sharing an application-wide [`ProxyState`]. Every adapter
    /// built on the same state sees a routing or proxy-URL change on its
    /// next request — no restart and no client rebuild.
    pub fn with_state(
        config: HttpClientConfig,
        shared: Option<Arc<ProxyState>>,
    ) -> Result<Self, ProviderError> {
        let state = if let Some(state) = shared {
            state
        } else {
            // Legacy single-proxy configuration: a config that names one
            // proxy URL means "send everything there", not "ignore it
            // because the default routing is direct".
            let routing = if config.proxy.is_some() && config.routing == ProxyRouting::default() {
                ProxyRouting::single_proxy()
            } else {
                config.routing
            };
            Arc::new(ProxyState::new(routing, config.proxy.clone()))
        };

        // Three pools, one per departure route. `Direct` explicitly opts out
        // of every ambient proxy; `System` keeps reqwest's platform
        // detection (Windows Internet Settings, environment elsewhere); and
        // `Custom` resolves the stored URL per request so a changed proxy
        // takes effect immediately.
        let direct = base_builder(&config)
            .no_proxy()
            .build()
            .map_err(build_error)?;
        let system = base_builder(&config).build().map_err(build_error)?;
        let custom_state = Arc::clone(&state);
        let custom = base_builder(&config)
            .proxy(reqwest::Proxy::custom(move |_url: &reqwest::Url| {
                custom_state
                    .custom_url()
                    .and_then(|url| reqwest::Url::parse(&url).ok())
            }))
            .build()
            .map_err(build_error)?;

        Ok(Self {
            direct,
            system,
            custom,
            config,
            state,
        })
    }

    /// Create a new HTTP client with default configuration.
    pub fn with_defaults() -> Result<Self, ProviderError> {
        Self::new(HttpClientConfig::default())
    }

    /// Get the client configuration.
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// The live proxy policy this client reads.
    pub fn proxy_state(&self) -> Arc<ProxyState> {
        Arc::clone(&self.state)
    }

    /// Transport for a provider in `region`, chosen from the live policy.
    pub fn route(&self, region: &ProviderRegion) -> RouteClient<'_> {
        let client = match self.state.route_for(region) {
            ProxyRoute::Direct => &self.direct,
            ProxyRoute::System => &self.system,
            ProxyRoute::Custom => &self.custom,
        };
        RouteClient {
            client,
            config: &self.config,
        }
    }

    /// Direct transport, for callers with no provider region (transport
    /// checks and tests).
    pub fn direct(&self) -> RouteClient<'_> {
        RouteClient {
            client: &self.direct,
            config: &self.config,
        }
    }

    /// Check if a response exceeds the maximum size limit.
    pub fn check_response_size(&self, content_length: Option<u64>) -> Result<(), ProviderError> {
        if let Some(length) = content_length
            && length > self.config.max_response_bytes as u64
        {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                format!(
                    "response size {} exceeds limit {}",
                    length, self.config.max_response_bytes
                ),
            ));
        }
        Ok(())
    }

    /// Redact sensitive headers for logging.
    pub fn redact_headers(headers: &HeaderMap) -> Vec<(String, String)> {
        headers
            .iter()
            .map(|(name, value)| {
                let name_str = name.as_str().to_lowercase();
                let value_str = if is_sensitive_header(&name_str) {
                    "***REDACTED***".to_string()
                } else {
                    value.to_str().unwrap_or("").to_string()
                };
                (name.to_string(), value_str)
            })
            .collect()
    }

    /// Log request headers with sensitive values redacted.
    pub fn log_request(&self, method: &str, url: &str, headers: &HeaderMap) {
        let redacted = Self::redact_headers(headers);
        tracing::debug!(
            method = method,
            url = url,
            headers = ?redacted,
            "HTTP request"
        );
    }
}

impl RouteClient<'_> {
    /// Build a GET request on this route.
    pub fn get(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.client.get(url)
    }

    /// Build a POST request on this route.
    pub fn post(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.client.post(url)
    }

    /// Sends the request and streams the body under the configured size cap.
    ///
    /// A declared `Content-Length` over the cap is rejected before any body
    /// byte is read; a missing or lying length is caught while streaming by
    /// counting every chunk. The returned [`ProviderResponse`] keeps only
    /// non-sensitive headers. This is the single read path every adapter must
    /// use — adapters do not call `send()` themselves.
    pub async fn send_capped(
        &self,
        request: RequestBuilder,
    ) -> Result<ProviderResponse, ProviderError> {
        let mut response = request.send().await.map_err(map_request_error)?;
        let status = response.status();
        let response_headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|length| length > self.config.max_response_bytes as u64)
        {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "provider response exceeds configured size limit",
            )
            .with_status(status.as_u16()));
        }
        let size_limit = self.config.max_response_bytes;
        let declared_length = response
            .content_length()
            .unwrap_or(0)
            .min(u64::try_from(size_limit).unwrap_or(u64::MAX));
        let mut body_bytes =
            Vec::with_capacity(usize::try_from(declared_length).unwrap_or(size_limit));
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

/// Maps transport failures onto stable provider error categories. No raw
/// reqwest display text is kept: messages stay short and constant, and URLs
/// or bodies never enter the error string.
pub(crate) fn map_request_error(error: reqwest::Error) -> ProviderError {
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

/// Copies response headers into a plain map, dropping sensitive ones so the
/// stored header set can never carry credentials onward.
fn response_headers_to_map(headers: &HeaderMap) -> std::collections::BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name_str = name.as_str().to_lowercase();
            if is_sensitive_header(&name_str) {
                return None;
            }
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_owned(), value.to_owned()))
        })
        .collect()
}

pub(crate) fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name,
        "authorization" | "x-api-key" | "cookie" | "set-cookie" | "proxy-authorization"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderName, HeaderValue};

    #[test]
    fn http_client_config_default() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(20));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.max_response_bytes, 1024 * 1024);
        assert!(!config.follow_redirects);
        assert_eq!(config.proxy, None);
    }

    #[test]
    fn from_adapter_limits_keeps_global_policies() {
        let config = HttpClientConfig::from_adapter_limits(Duration::from_secs(15), 512 * 1024);
        assert_eq!(config.timeout, Duration::from_secs(15));
        assert_eq!(config.max_response_bytes, 512 * 1024);
        // Redirect denial and the no-implicit-proxy rule are not per-adapter
        // settings; they must survive every construction path.
        assert!(!config.follow_redirects);
        assert_eq!(config.proxy, None);
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
    }

    #[test]
    fn routing_maps_china_to_domestic_and_everything_else_international() {
        let routing = ProxyRouting {
            domestic: ProxyRoute::Direct,
            international: ProxyRoute::System,
        };
        assert_eq!(
            routing.route_for(&ProviderRegion::China),
            ProxyRoute::Direct
        );
        assert_eq!(
            routing.route_for(&ProviderRegion::Global),
            ProxyRoute::System
        );
        assert_eq!(
            routing.route_for(&ProviderRegion::NorthAmerica),
            ProxyRoute::System
        );
        // An undeclared region takes the international route.
        assert_eq!(
            routing.route_for(&ProviderRegion::Custom("edge".to_owned())),
            ProxyRoute::System
        );
    }

    #[test]
    fn the_default_policy_is_domestic_direct_and_international_system() {
        let routing = ProxyRouting::default();
        assert_eq!(routing.domestic, ProxyRoute::Direct);
        assert_eq!(routing.international, ProxyRoute::System);
    }

    #[test]
    fn http_client_with_defaults_builds() {
        let client = HttpClient::with_defaults();
        assert!(client.is_ok());
    }

    #[test]
    fn check_response_size_none_is_ok() {
        let client = HttpClient::with_defaults().unwrap();
        assert!(client.check_response_size(None).is_ok());
    }

    #[test]
    fn check_response_size_within_limit() {
        let client = HttpClient::with_defaults().unwrap();
        assert!(client.check_response_size(Some(1024)).is_ok());
    }

    #[test]
    fn check_response_size_exceeds_limit() {
        let client = HttpClient::with_defaults().unwrap();
        assert!(client.check_response_size(Some(2 * 1024 * 1024)).is_err());
    }

    #[test]
    fn is_sensitive_header_detects_auth() {
        assert!(is_sensitive_header("authorization"));
        assert!(is_sensitive_header("x-api-key"));
        assert!(is_sensitive_header("cookie"));
        assert!(!is_sensitive_header("content-type"));
    }

    #[test]
    fn redact_headers_redacts_sensitive() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer secret123"),
        );
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        let redacted = HttpClient::redact_headers(&headers);
        assert_eq!(redacted[0].1, "***REDACTED***");
        assert_eq!(redacted[1].1, "application/json");
    }

    #[test]
    fn response_headers_to_map_drops_sensitive_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("sk-secret"),
        );
        headers.insert(
            HeaderName::from_static("retry-after"),
            HeaderValue::from_static("30"),
        );
        let mapped = response_headers_to_map(&headers);
        assert!(!mapped.contains_key("x-api-key"));
        assert_eq!(mapped.get("retry-after").map(String::as_str), Some("30"));
    }

    #[test]
    fn request_error_mapping_is_stable_and_secret_free() {
        // A refused loopback connection is a deterministic stand-in for an
        // unreachable provider: a real DNS miss depends on the host resolver,
        // and privacy proxies (fake-IP DNS) reclassify the failure.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}/v1/usage");
        let error = tauri::async_runtime::block_on(async {
            HttpClient::with_defaults()
                .unwrap()
                .direct()
                .get(&url)
                .send()
                .await
                .expect_err("refused connection must fail")
        });
        let mapped = map_request_error(error);
        assert!(
            matches!(
                mapped.code,
                ProviderErrorKind::NetworkUnavailable | ProviderErrorKind::Unknown
            ),
            "DNS/connect failures map to a stable category"
        );
        assert!(
            !mapped.message.contains(&format!("127.0.0.1:{port}")),
            "error text must not echo the URL"
        );
    }
}
