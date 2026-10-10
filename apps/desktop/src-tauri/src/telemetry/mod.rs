//! Logging and telemetry with secret redaction.
//!
//! All log output passes through redaction to prevent accidental exposure
//! of credentials, tokens, or other sensitive values.

use std::fmt;

/// Log levels for telemetry events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// A telemetry event with structured context.
#[derive(Debug, Clone)]
pub struct TelemetryEvent {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: Vec<(String, String)>,
}

impl TelemetryEvent {
    /// Create a new telemetry event.
    pub fn new(level: LogLevel, target: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level,
            target: target.into(),
            message: message.into(),
            fields: Vec::new(),
        }
    }

    /// Add a key-value field to the event.
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        let value = if is_sensitive_field(&key) {
            "***REDACTED***".to_string()
        } else {
            value
        };
        self.fields.push((key, value));
        self
    }

    /// Format the event as a log line.
    pub fn format(&self) -> String {
        let fields = self
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        if fields.is_empty() {
            format!("[{}] {}: {}", self.level, self.target, self.message)
        } else {
            format!(
                "[{}] {}: {} {}",
                self.level, self.target, self.message, fields
            )
        }
    }
}

/// Check if a field key contains sensitive data.
pub fn is_sensitive_field(key: &str) -> bool {
    let lower = key.to_lowercase();
    let normalized: String = lower.chars().filter(char::is_ascii_alphanumeric).collect();
    matches!(
        normalized.as_str(),
        "authorization"
            | "apikey"
            | "apisecretkey"
            | "secretkey"
            | "accesstoken"
            | "refreshtoken"
            | "clientsecret"
            | "password"
            | "secret"
            | "sessiontoken"
            | "privatekey"
            | "token"
            | "credential"
    )
}

/// Redact a string value that might contain sensitive data.
pub fn redact_value(key: &str, value: &str) -> String {
    if is_sensitive_field(key) {
        if value.is_empty() {
            String::new()
        } else {
            "***".to_string()
        }
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_display() {
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }

    #[test]
    fn telemetry_event_format_no_fields() {
        let event = TelemetryEvent::new(LogLevel::Info, "test", "hello");
        assert_eq!(event.format(), "[INFO] test: hello");
    }

    #[test]
    fn telemetry_event_format_with_fields() {
        let event =
            TelemetryEvent::new(LogLevel::Info, "test", "hello").with_field("provider", "openai");
        assert_eq!(event.format(), "[INFO] test: hello provider=openai");
    }

    #[test]
    fn telemetry_event_redacts_sensitive_fields() {
        let event = TelemetryEvent::new(LogLevel::Info, "test", "auth")
            .with_field("authorization", "Bearer secret123");
        assert!(!event.format().contains("secret123"));
    }

    #[test]
    fn is_sensitive_field_detects_keys() {
        assert!(is_sensitive_field("authorization"));
        assert!(is_sensitive_field("api_key"));
        assert!(is_sensitive_field("access_token"));
        assert!(is_sensitive_field("password"));
        assert!(!is_sensitive_field("provider_id"));
    }

    #[test]
    fn redact_value_masks_sensitive() {
        assert_eq!(redact_value("authorization", "Bearer sk-1234"), "***");
        assert_eq!(redact_value("provider", "openai"), "openai");
    }

    #[test]
    fn redact_value_short_input_returns_asterisks() {
        assert_eq!(redact_value("token", "abc"), "***");
    }
}
