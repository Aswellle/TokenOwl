// OS credential store integration
// Original content moved from src-tauri/src/credentials.rs
//! OS credential-store integration.
//!
//! Credential references are safe to persist in `SQLite`. Secret values are resolved only
//! at request time and are never serialized, logged, or exposed through DTOs.

use keyring::{Entry, Error as KeyringError};
use thiserror::Error;

const DEFAULT_SERVICE: &str = "com.tokenowl.desktop";

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential reference is required")]
    EmptyReference,
    #[error("credential store initialization failed")]
    StoreUnavailable,
    #[error("credential is not present in the OS credential store")]
    Missing,
    #[error("OS credential store operation failed: {0}")]
    Store(String),
}

/// A resolved secret with deliberately restricted formatting and serialization behavior.
#[derive(Clone)]
pub struct CredentialValue(String);

impl CredentialValue {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CredentialValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialValue(REDACTED)")
    }
}

/// Small service wrapper over the platform-native keyring/keychain implementation.
#[derive(Clone, Debug)]
pub struct CredentialStore {
    service: String,
}

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new(DEFAULT_SERVICE)
    }
}

impl CredentialStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, credential_ref: &str) -> Result<Entry, CredentialError> {
        let credential_ref = credential_ref.trim();
        if credential_ref.is_empty() {
            return Err(CredentialError::EmptyReference);
        }
        Entry::new(&self.service, credential_ref).map_err(map_keyring_error)
    }

    /// Stores a secret in the native OS credential store under the supplied reference.
    pub fn set(&self, credential_ref: &str, secret: &str) -> Result<(), CredentialError> {
        if secret.is_empty() {
            return Err(CredentialError::Store(
                "credential secret must not be empty".to_owned(),
            ));
        }
        self.entry(credential_ref)?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    /// Resolves a secret just before a provider request. The returned value is not serializable.
    pub fn resolve(&self, credential_ref: &str) -> Result<CredentialValue, CredentialError> {
        let secret = self
            .entry(credential_ref)?
            .get_password()
            .map_err(map_keyring_error)?;
        Ok(CredentialValue(secret))
    }

    pub fn delete(&self, credential_ref: &str) -> Result<(), CredentialError> {
        match self.entry(credential_ref)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

fn map_keyring_error(error: KeyringError) -> CredentialError {
    match error {
        KeyringError::NoEntry => CredentialError::Missing,
        KeyringError::NoStorageAccess(_) => CredentialError::StoreUnavailable,
        KeyringError::PlatformFailure(inner) => CredentialError::Store(inner.to_string()),
        KeyringError::TooLong(attribute, limit) => CredentialError::Store(format!(
            "credential attribute {attribute} exceeds the platform limit of {limit}"
        )),
        KeyringError::Invalid(attribute, reason) => CredentialError::Store(format!(
            "credential attribute {attribute} is invalid: {reason}"
        )),
        // `BadEncoding` carries the stored bytes and `Ambiguous` carries matching
        // entries; neither is ever formatted, so no secret value reaches a message.
        KeyringError::BadEncoding(_) | KeyringError::Ambiguous(_) => {
            CredentialError::Store("credential store returned an unusable entry".to_owned())
        }
        _ => CredentialError::Store("credential store operation failed".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_references_are_rejected_before_store_access() {
        let store = CredentialStore::default();
        assert!(matches!(
            store.resolve("  "),
            Err(CredentialError::EmptyReference)
        ));
        assert!(matches!(
            store.delete(""),
            Err(CredentialError::EmptyReference)
        ));
    }

    #[test]
    fn resolved_values_are_redacted_in_debug() {
        let value = CredentialValue("fixture-secret".to_owned());
        assert_eq!(format!("{value:?}"), "CredentialValue(REDACTED)");
        assert!(!format!("{value:?}").contains("fixture-secret"));
    }
}
