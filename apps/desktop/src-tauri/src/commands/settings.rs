//! Settings command surface, split out of the former single-file module.

use super::{AppRuntime, Arc, CommandError, CredentialStore, LocalRepository, State};
use crate::networking::{ProxyRoute, ProxyRouting};

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
/// Settings key for the per-region proxy policy. Non-secret: it names routes,
/// never a URL (the URL itself stays in the credential store).
pub(super) const PROXY_ROUTING_KEY: &str = "proxy_routing";

/// Reads the persisted routing policy. A missing or malformed value means the
/// safe defaults — domestic direct, international through the system proxy —
/// so a corrupted row can never silently route domestic traffic abroad.
pub(super) fn proxy_routing_from(repository: &LocalRepository) -> ProxyRouting {
    repository
        .get_setting(PROXY_ROUTING_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn proxy_routing_get(state: State<'_, Arc<AppRuntime>>) -> Result<ProxyRouting, CommandError> {
    Ok(state.proxy_state.routing())
}

/// Persists and applies the routing policy immediately: the transport reads
/// it per request, so no restart is involved.
#[tauri::command]
pub fn proxy_routing_set(
    domestic: ProxyRoute,
    international: ProxyRoute,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<ProxyRouting, CommandError> {
    let routing = ProxyRouting {
        domestic,
        international,
    };
    if routing.domestic == ProxyRoute::Custom && proxy_setting(&state.credentials).is_none() {
        return Err(CommandError::InvalidInput(
            "the custom route needs a proxy URL; save one first".to_owned(),
        ));
    }
    if routing.international == ProxyRoute::Custom && proxy_setting(&state.credentials).is_none() {
        return Err(CommandError::InvalidInput(
            "the custom route needs a proxy URL; save one first".to_owned(),
        ));
    }
    let value = serde_json::to_value(routing)
        .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
    state.repository()?.set_setting(PROXY_ROUTING_KEY, &value)?;
    state.proxy_state.set_routing(routing);
    Ok(routing)
}

#[tauri::command]
pub fn proxy_get(state: State<'_, Arc<AppRuntime>>) -> Result<Option<String>, CommandError> {
    Ok(proxy_setting(&state.credentials)
        .as_deref()
        .map(mask_proxy_password))
}

/// Stores or clears the proxy URL and applies it immediately: the custom
/// route resolves the URL per request, so no restart is needed.
#[tauri::command]
pub fn proxy_set(
    url: Option<String>,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    if let Some(value) = url {
        let stored = proxy_setting(&state.credentials);
        let merged = restore_masked_password(value.trim(), stored.as_deref());
        let validated = validate_proxy_url(&merged)?;
        state.credentials.set(PROXY_CREDENTIAL_REF, &validated)?;
        state.proxy_state.set_custom_url(Some(validated));
    } else {
        state.credentials.delete(PROXY_CREDENTIAL_REF)?;
        state.proxy_state.set_custom_url(None);
        // A cleared URL leaves the custom route without a target; fall
        // back to direct so no request silently bypasses the intent.
        let routing = state.proxy_state.routing();
        if routing.domestic == ProxyRoute::Custom || routing.international == ProxyRoute::Custom {
            let fallback = ProxyRouting {
                domestic: if routing.domestic == ProxyRoute::Custom {
                    ProxyRoute::Direct
                } else {
                    routing.domestic
                },
                international: if routing.international == ProxyRoute::Custom {
                    ProxyRoute::Direct
                } else {
                    routing.international
                },
            };
            let value = serde_json::to_value(fallback)
                .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
            state.repository()?.set_setting(PROXY_ROUTING_KEY, &value)?;
            state.proxy_state.set_routing(fallback);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_routing_means_domestic_direct_and_international_system() {
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        assert_eq!(proxy_routing_from(&repository), ProxyRouting::default());
    }

    #[test]
    fn malformed_routing_falls_back_to_the_safe_defaults() {
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        repository
            .set_setting(PROXY_ROUTING_KEY, &serde_json::json!({ "domestic": 7 }))
            .expect("store malformed value");
        assert_eq!(proxy_routing_from(&repository), ProxyRouting::default());
    }

    #[test]
    fn a_persisted_policy_round_trips() {
        let repository = LocalRepository::open_in_memory().expect("in-memory repository");
        let routing = ProxyRouting {
            domestic: ProxyRoute::System,
            international: ProxyRoute::Custom,
        };
        repository
            .set_setting(
                PROXY_ROUTING_KEY,
                &serde_json::to_value(routing).expect("encode"),
            )
            .expect("store");
        assert_eq!(proxy_routing_from(&repository), routing);
    }
}
