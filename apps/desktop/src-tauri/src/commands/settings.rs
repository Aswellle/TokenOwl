//! Settings command surface, split out of the former single-file module.

use super::{AppRuntime, Arc, CommandError, CredentialStore, State};

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

/// The sentinel that replaces a stored proxy password across IPC. `proxy_get`
/// masks the password so it never re-enters the webview; `proxy_set` splices
/// the stored password back when the submitted URL still carries the sentinel
/// (the user edited host or username but left the masked password alone).
const PROXY_PASSWORD_MASK: &str = "__tokenowl_masked__";

/// Splits `scheme://user:password@rest` into (prefix through `user:`,
/// password, `@rest`). `None` when the URL carries no non-empty password.
fn split_proxy_password(url: &str) -> Option<(String, String, String)> {
    let scheme_end = url.find("://")?;
    let authority_start = scheme_end + 3;
    let authority = &url[authority_start..];
    let at = authority.rfind('@')?;
    let userinfo = &authority[..at];
    let colon = userinfo.find(':')?;
    let password = userinfo[colon + 1..].to_owned();
    if password.is_empty() {
        return None;
    }
    Some((
        url[..=authority_start + colon].to_owned(),
        password,
        url[authority_start + at..].to_owned(),
    ))
}

/// Masks an embedded proxy password for the settings round-trip. A URL
/// without credentials passes through untouched.
pub(super) fn mask_proxy_password(url: &str) -> String {
    match split_proxy_password(url) {
        Some((prefix, _, suffix)) => format!("{prefix}{PROXY_PASSWORD_MASK}{suffix}"),
        None => url.to_owned(),
    }
}

/// Restores the real password when the submitted URL still carries the mask.
/// Without a stored password to restore, the sentinel degrades to empty
/// (`user:@host` is a valid credentialless userinfo).
pub(super) fn restore_masked_password(submitted: &str, stored: Option<&str>) -> String {
    let Some((prefix, password, suffix)) = split_proxy_password(submitted) else {
        return submitted.to_owned();
    };
    if password != PROXY_PASSWORD_MASK {
        return submitted.to_owned();
    }
    if let Some((_, real, _)) = stored.and_then(split_proxy_password) {
        return format!("{prefix}{real}{suffix}");
    }
    let prefix = prefix.strip_suffix(':').unwrap_or(&prefix);
    format!("{prefix}{suffix}")
}

/// Returns the configured proxy URL for the settings page, with any embedded
/// password masked so the secret never re-enters the webview. `None` means
/// direct.
#[tauri::command]
pub fn proxy_get(state: State<'_, Arc<AppRuntime>>) -> Result<Option<String>, CommandError> {
    Ok(proxy_setting(&state.credentials)
        .as_deref()
        .map(mask_proxy_password))
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
            let stored = proxy_setting(&state.credentials);
            let merged = restore_masked_password(value.trim(), stored.as_deref());
            let validated = validate_proxy_url(&merged)?;
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
