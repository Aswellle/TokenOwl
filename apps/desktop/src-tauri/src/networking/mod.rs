//! Unified HTTP client for provider requests.
//!
//! This is the only place a `reqwest::Client` may be built: every provider
//! adapter holds an [`HttpClient`], so redirect policy, proxy behavior,
//! timeouts, and the streaming response-size cap are enforced once instead of
//! being re-implemented per adapter. Adapters keep their own
//! `AdapterLimits` (per-provider timeout/size defaults) and feed them in via
//! [`HttpClientConfig::from_adapter_limits`].
use crate::domain::provider::{ProviderError, ProviderErrorKind, ProviderResponse};
use reqwest::header::HeaderMap;
use reqwest::{Client, RequestBuilder};
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
    /// Proxy policy, explicit by design. `None` disables proxies entirely —
    /// environment variables such as `HTTP_PROXY` are ignored — while `Some`
    /// routes through exactly that proxy. Reqwest's implicit environment
    /// proxy detection is deliberately opted out of so transport behavior is
    /// deterministic and testable.
    pub proxy: Option<String>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(10),
            max_response_bytes: 1024 * 1024,
            follow_redirects: false,
            proxy: None,
        }
    }
}

impl HttpClientConfig {
    /// Transport policy for an adapter's per-provider limits. The adapter
    /// supplies the request timeout and response cap; redirect and proxy
    /// policy always take the global defaults (deny redirects, no implicit
    /// proxy).
    pub fn from_adapter_limits(timeout: Duration, max_response_bytes: usize) -> Self {
        Self {
            timeout,
            max_response_bytes,
            ..Self::default()
        }
    }
}

/// Unified HTTP client with security and retry policies. `Clone` is cheap
/// (the underlying connection pool is shared); `Debug` leaks nothing — the
/// config holds no secrets and `reqwest::Client`'s own Debug is opaque.
#[derive(Clone, Debug)]
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration.
    pub fn new(config: HttpClientConfig) -> Result<Self, ProviderError> {
        let mut builder = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout);

        if !config.follow_redirects {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }

        match &config.proxy {
            Some(proxy_url) => {
                let proxy = reqwest::Proxy::all(proxy_url).map_err(|e| {
                    ProviderError::new(
                        ProviderErrorKind::ConfigInvalid,
                        format!("invalid proxy URL: {e}"),
                    )
                })?;
                builder = builder.proxy(proxy);
            }
            None => builder = builder.no_proxy(),
        }

        let client = builder.build().map_err(|e| {
            ProviderError::new(
                ProviderErrorKind::ConfigInvalid,
                format!("failed to build HTTP client: {e}"),
            )
        })?;

        Ok(Self { client, config })
    }

    /// Create a new HTTP client with default configuration.
    pub fn with_defaults() -> Result<Self, ProviderError> {
        Self::new(HttpClientConfig::default())
    }

    /// Get the client configuration.
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Build a GET request.
    pub fn get(&self, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.client.get(url)
    }

    /// Build a POST request.
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
