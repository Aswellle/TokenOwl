//! Settings command surface, split out of the former single-file module.

use super::{CredentialStore, CommandError, State, Arc, AppRuntime};

/// OS credential-store reference for the user-configured proxy URL. It may
/// embed proxy credentials (`http://user:pass@host:port`), so it rides the
/// keyring like any other secret instead of the settings table.
pub(super) const PROXY_CREDENTIAL_REF: &str = "transport/proxy";

/// Reads the stored proxy URL; `None` means direct connection. A keyring
/// failure is indistinguishable from "not set" by design — the transport
/// falls back to direct rather than surfacing store errors at startup.
pub(super) fn proxy_setting(credentials: &CredentialStore) -> Option<String> {
    credentials
        .resolve(PROXY_CREDENTIAL_REF)
        .ok()
        .map(|value| value.as_str().to_owned())
}

/// Validates a user-entered proxy URL. Messages stay constant: the URL (and
/// anything embedded in it) is never echoed back into an error string.
pub(super) fn validate_proxy_url(value: &str) -> Result<String, CommandError> {
    let trimmed = value.trim();
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| {
        CommandError::InvalidInput("proxy URL is not a valid absolute URL".to_owned())
    })?;
    match parsed.scheme() {
        "http" | "https" => Ok(trimmed.to_owned()),
        _ => Err(CommandError::InvalidInput(
            "proxy URL must use the http or https scheme".to_owned(),
        )),
    }
}

/// Returns the configured proxy URL for the settings page (the user's own
/// input, needed to pre-fill the edit field). `None` means direct.
#[tauri::command]
pub fn proxy_get(state: State<'_, Arc<AppRuntime>>) -> Result<Option<String>, CommandError> {
    Ok(proxy_setting(&state.credentials))
}

/// Stores or clears the proxy URL. Takes effect on the next app start — the
/// adapters build their HTTP client once at boot — and the frontend says so.
#[tauri::command]
pub fn proxy_set(
    url: Option<String>,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    match url {
        Some(value) => {
            let validated = validate_proxy_url(&value)?;
            state.credentials.set(PROXY_CREDENTIAL_REF, &validated)?;
        }
        None => {
            state.credentials.delete(PROXY_CREDENTIAL_REF)?;
        }
    }
    Ok(())
}

/// Settings key for the dashboard's collapsed bottom dock. A layout
/// preference, not a secret — it rides the settings table.
pub(super) const DOCK_COLLAPSED_KEY: &str = "dock_collapsed";

/// Only an explicit `true` collapses; a missing or malformed entry means the
/// dock is shown, so a corrupted value can never hide the controls.
pub(super) fn dock_collapsed_from(value: Option<serde_json::Value>) -> bool {
    matches!(value, Some(serde_json::Value::Bool(true)))
}

/// Returns whether the user last collapsed the dashboard's bottom dock.
#[tauri::command]
pub fn dock_state_get(state: State<'_, Arc<AppRuntime>>) -> Result<bool, CommandError> {
    let value = state.repository()?.get_setting(DOCK_COLLAPSED_KEY)?;
    Ok(dock_collapsed_from(value))
}

/// Persists the dock's collapsed state for the next launch.
#[tauri::command]
pub fn dock_state_set(
    collapsed: bool,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    state
        .repository()?
        .set_setting(DOCK_COLLAPSED_KEY, &serde_json::Value::Bool(collapsed))?;
    Ok(())
}
