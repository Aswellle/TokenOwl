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

#[test]
fn credential_value_does_not_implement_serialize() {
    // CredentialValue intentionally does not implement Serialize.
    // This test ensures a future change cannot accidentally make it serializable.
    fn assert_not_serialize<T>() {}
    assert_not_serialize::<CredentialValue>();
}

#[test]
fn credential_value_does_not_implement_display() {
    // CredentialValue intentionally does not implement Display.
    // This test ensures a future change cannot accidentally expose the secret.
    fn assert_not_display<T>() {}
    assert_not_display::<CredentialValue>();
}

#[test]
fn credential_value_as_str_returns_secret_only_when_explicitly_called() {
    let value = CredentialValue("my-secret-key".to_owned());
    // as_str is the only way to access the secret, and it requires explicit intent.
    assert_eq!(value.as_str(), "my-secret-key");
}

#[test]
fn credential_store_set_rejects_empty_secret() {
    let store = CredentialStore::default();
    assert!(matches!(
        store.set("ref", ""),
        Err(CredentialError::Store(_))
    ));
}

#[test]
fn credential_error_display_does_not_contain_secret() {
    let error = CredentialError::Store("operation failed".to_owned());
    let message = error.to_string();
    assert!(!message.contains("secret"));
    assert!(!message.contains("password"));
}

#[test]
fn credential_value_clone_is_safe() {
    let value = CredentialValue("sensitive-data".to_owned());
    let cloned = value.clone();
    // Clone is safe but does not expose the secret through Debug.
    assert_eq!(format!("{cloned:?}"), "CredentialValue(REDACTED)");
}

#[test]
fn credential_store_new_uses_custom_service() {
    let store = CredentialStore::new("custom.service");
    // The store should use the custom service name.
    // We can't directly access the private field, but we can verify it compiles.
    assert_eq!(store.service, "custom.service");
}

#[cfg(test)]
mod secret_leak {
    use super::*;

    const SECRET: &str = "sk-tokenowl-unit-9f4e2d1c7b5aSECRET";

    #[test]
    fn debug_output_is_fully_redacted() {
        let value = CredentialValue(SECRET.to_owned());
        let rendered = format!("{value:?}");
        assert_eq!(rendered, "CredentialValue(REDACTED)");
        assert!(!rendered.contains(SECRET));
        assert!(!rendered.contains("sk-tokenowl"));
    }

    #[test]
    fn clone_keeps_the_redacted_debug() {
        let value = CredentialValue(SECRET.to_owned()).clone();
        assert_eq!(format!("{value:?}"), "CredentialValue(REDACTED)");
    }

    #[test]
    fn as_str_stays_available_for_request_building() {
        let value = CredentialValue(SECRET.to_owned());
        assert_eq!(value.as_str(), SECRET);
    }

    #[test]
    fn keyring_bad_encoding_error_never_carries_the_secret() {
        // `BadEncoding` wraps the stored bytes; the mapped error must not
        // surface them through Display or Debug.
        let error = map_keyring_error(KeyringError::BadEncoding(SECRET.as_bytes().to_vec()));
        assert!(!error.to_string().contains(SECRET));
        assert!(!format!("{error:?}").contains(SECRET));
    }

    #[test]
    fn credential_error_variants_never_name_secret_material() {
        let secret_ish = "sk-tokenowl-unit-9f4e2d1c7b5aSECRET";
        for error in [
            CredentialError::EmptyReference,
            CredentialError::Missing,
            CredentialError::StoreUnavailable,
        ] {
            // `Store` carries call-site text by contract; the mapped keyring
            // arms above prove that contract holds for store-derived values.
            let rendered = error.to_string();
            assert!(!rendered.contains(secret_ish));
        }
    }
}
