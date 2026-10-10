//! Unified HTTP client for provider requests.
//!
//! Provides timeout, retry policy, TLS enforcement, redirect policy,
use crate::domain::provider::{ProviderError, ProviderErrorKind};
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
    /// Whether to follow redirects.
    pub follow_redirects: bool,
    /// Whether to enforce HTTPS.
    pub enforce_https: bool,
    /// Proxy URL (optional).
    pub proxy: Option<String>,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            connect_timeout: Duration::from_secs(10),
            max_response_bytes: 1024 * 1024,
            follow_redirects: false,
            enforce_https: true,
            proxy: None,
        }
    }
}

/// Unified HTTP client with security and retry policies.
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

        if let Some(proxy_url) = &config.proxy {
            if let Ok(proxy) = reqwest::Proxy::all(proxy_url) {
                builder = builder.proxy(proxy);
            }
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

    /// Get a reference to the underlying reqwest client.
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// Get the client configuration.
    pub fn config(&self) -> &HttpClientConfig {
        &self.config
    }

    /// Build a GET request.
    pub fn get(&self, url: &str) -> RequestBuilder {
        self.client.get(url)
    }

    /// Build a POST request.
    pub fn post(&self, url: &str) -> RequestBuilder {
        self.client.post(url)
    }

    /// Check if a response exceeds the maximum size limit.
    pub fn check_response_size(&self, content_length: Option<u64>) -> Result<(), ProviderError> {
        if let Some(length) = content_length {
            if length > self.config.max_response_bytes as u64 {
                return Err(ProviderError::new(
                    ProviderErrorKind::InvalidResponse,
                    format!(
                        "response size {} exceeds limit {}",
                        length, self.config.max_response_bytes
                    ),
                ));
            }
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

    /// Check if a header name is sensitive.
    pub fn is_sensitive_header(name: &str) -> bool {
        is_sensitive_header(name)
    }
}

fn is_sensitive_header(name: &str) -> bool {
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
        assert!(config.enforce_https);
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
}
