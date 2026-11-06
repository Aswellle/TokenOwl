//! LAN consent and trust command surface, split out of the former single-file module.

use super::{
    AppRuntime, Arc, CommandError, LanConsentLedger, LocalRepository, State, StorageError,
    unix_seconds,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LanConsentRecord {
    pub(super) target: String,
    pub(super) granted_at: u64,
    pub(super) consumed: bool,
}

/// How long a confirmation stays valid before the account is created.
pub(super) const LAN_CONSENT_TTL_SECS: u64 = 600;

impl LanConsentLedger {
    /// Issues a one-time token for `target`, pruning stale entries first so
    /// the session map stays bounded.
    pub(super) fn begin(&self, target: &str, now: u64) -> String {
        let mut issued = self.issued.lock().expect("consent ledger lock");
        issued.retain(|_, record| {
            !record.consumed && now.saturating_sub(record.granted_at) <= LAN_CONSENT_TTL_SECS
        });
        let token = nanoid::nanoid!();
        issued.insert(
            token.clone(),
            LanConsentRecord {
                target: target.to_owned(),
                granted_at: now,
                consumed: false,
            },
        );
        token
    }

    /// Consumes the token issued for exactly `target`, or explains why the
    /// creation must not proceed. Every failure keeps the record untouched,
    /// so a rejected attempt never burns a still-valid confirmation.
    pub(super) fn authorize(
        &self,
        target: &str,
        token: Option<&str>,
        now: u64,
    ) -> Result<(), CommandError> {
        let Some(token) = token else {
            return Err(CommandError::InvalidInput(
                "this target is on the local network; confirm the LAN notice before creating the account"
                    .to_owned(),
            ));
        };
        let mut issued = self.issued.lock().expect("consent ledger lock");
        match issued.get_mut(token) {
            Some(record) if record.target != target => Err(CommandError::InvalidInput(
                "the LAN confirmation was issued for a different URL; confirm again".to_owned(),
            )),
            Some(record) if record.consumed => Err(CommandError::InvalidInput(
                "the LAN confirmation was already used; confirm again".to_owned(),
            )),
            Some(record) if now.saturating_sub(record.granted_at) > LAN_CONSENT_TTL_SECS => {
                Err(CommandError::InvalidInput(
                    "the LAN confirmation expired; confirm again".to_owned(),
                ))
            }
            Some(record) => {
                record.consumed = true;
                Ok(())
            }
            None => Err(CommandError::InvalidInput(
                "no LAN confirmation on record for this target; confirm the notice first"
                    .to_owned(),
            )),
        }
    }

    /// Test-only view of the ledger size (used by the pruning test).
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.issued.lock().expect("consent ledger lock").len()
    }
}

/// Fails closed when `target` is on the local network and no valid, unused
/// confirmation token was presented. Classification re-runs here — not in
/// the frontend — so a UI bug or a stale dialog can never substitute for
/// consent, and a classifier outage surfaces as a rejected creation instead
/// of a silently public-looking LAN account.
///
/// The user's explicitly trusted LAN targets (managed, revocable, see the
/// `lan_trust_*` commands) bypass the per-creation prompt; everything else
/// still demands the one-time token.
pub(super) fn authorize_lan_target(
    ledger: &LanConsentLedger,
    trusted: &[String],
    target: &str,
    token: Option<&str>,
) -> Result<(), CommandError> {
    if !resolves_to_lan(target) {
        return Ok(());
    }
    if trusted.iter().any(|entry| entry == target) {
        return Ok(());
    }
    ledger.authorize(target, token, unix_seconds())
}

/// Tells the setup form whether a candidate account points at the LAN.
///
/// The form calls this before submitting so it can show the `docs/03` §12
/// consent prompt. Classification lives in Rust because the webview has no
/// network stack and must not grow one just to parse a host. DNS resolution
/// is blocking, so it runs off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn account_target_is_lan(base_url: String) -> bool {
    let trimmed = base_url.trim().to_owned();
    tauri::async_runtime::spawn_blocking(move || resolves_to_lan(&trimmed))
        .await
        .unwrap_or(false)
}

/// Settings key for LAN targets the user explicitly trusts. Hostnames are
/// not secrets, so these ride the settings table; the trust is per exact
/// trimmed base URL, revocable at any time.
pub(super) const LAN_TRUST_SETTING_KEY: &str = "lan_trust_targets";

pub(super) fn load_lan_trusts(repository: &LocalRepository) -> Result<Vec<String>, CommandError> {
    match repository.get_setting(LAN_TRUST_SETTING_KEY)? {
        None => Ok(Vec::new()),
        Some(value) => Ok(serde_json::from_value(value).map_err(StorageError::from)?),
    }
}

pub(super) fn save_lan_trusts(
    repository: &LocalRepository,
    targets: Vec<String>,
) -> Result<(), CommandError> {
    repository.set_setting(
        LAN_TRUST_SETTING_KEY,
        &serde_json::to_value(&targets).map_err(StorageError::from)?,
    )?;
    Ok(())
}

/// Lists the LAN base URLs the user has explicitly trusted.
#[tauri::command]
pub fn lan_trust_list(state: State<'_, Arc<AppRuntime>>) -> Result<Vec<String>, CommandError> {
    load_lan_trusts(&state.repository()?)
}

/// Adds a LAN target to the trust list so future account creations against
/// the exact same URL skip the confirmation prompt. Only classifiable LAN
/// targets are accepted: a public URL needs no trust, and an unclassifiable
/// one fails closed.
#[tauri::command]
pub async fn lan_trust_add(
    url: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    let target = url.trim().to_owned();
    if target.is_empty() {
        return Err(CommandError::InvalidInput(
            "a base URL is required to trust a LAN target".to_owned(),
        ));
    }
    let classify_target = target.clone();
    let is_lan = tauri::async_runtime::spawn_blocking(move || resolves_to_lan(&classify_target))
        .await
        .map_err(|error| CommandError::InvalidInput(error.to_string()))?;
    if !is_lan {
        return Err(CommandError::InvalidInput(
            "only local-network targets can be trusted; public URLs never ask for consent"
                .to_owned(),
        ));
    }
    let repository = state.repository()?;
    let mut trusted = load_lan_trusts(&repository)?;
    if !trusted.iter().any(|entry| entry == &target) {
        trusted.push(target);
        save_lan_trusts(&repository, trusted)?;
    }
    Ok(())
}

/// Removes a target from the trust list; creations against it ask for
/// confirmation again. Removing an unlisted URL succeeds quietly.
#[tauri::command]
pub fn lan_trust_remove(
    url: String,
    state: State<'_, Arc<AppRuntime>>,
) -> Result<(), CommandError> {
    let repository = state.repository()?;
    let mut trusted = load_lan_trusts(&repository)?;
    // Mirror the add path's trimming: a padded remove request would
    // otherwise report success while leaving the trust entry in place.
    let target = url.trim();
    trusted.retain(|entry| entry != target);
    save_lan_trusts(&repository, trusted)
}

/// Whether a host targets the local machine or a private network.
///
/// `docs/06` blocks LAN access by default and `docs/03` §12 requires an
/// explicit prompt the first time a provider points at one. This is the
/// classification half; the consent itself is collected by the account form.
///
/// Loopback and link-local ranges are always local. Private ranges are
/// detected numerically rather than by prefix matching, so `172.16.0.1` is
/// private while `172.32.0.1` (public) is not. IPv6 unique-local addresses
/// (`fc00::/7`) and IPv4-mapped IPv6 addresses are classified by the same
/// rules — the v6 form must not bypass the v4 classification.
pub(crate) fn is_lan_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.localdomain")
    {
        return true;
    }
    // `.local` is mDNS: resolution stays on the local link.
    if host.to_ascii_lowercase().ends_with(".local") {
        return true;
    }
    let Ok(address) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    ip_is_lan(&address)
}

/// Numeric LAN classification of one address, shared by literal hosts and
/// DNS answers so both sides follow identical rules.
pub(super) fn ip_is_lan(address: &std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            // `::ffff:a.b.c.d` carries an IPv4 address; classify by it so the
            // mapped form cannot sidestep the IPv4 private-range rules.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_is_lan(&std::net::IpAddr::V4(mapped));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                // fe80::/10 link-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // fc00::/7 unique-local (fd00::/8 in practice)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Whether a host is this machine only (loopback), not the wider LAN. Used
/// for the plaintext-HTTP exception: `http` is tolerated for local
/// development proxies, never for remote or LAN targets.
pub(super) fn is_loopback_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case("localhost.localdomain")
    {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(std::net::IpAddr::V6(v6)) => v6.is_loopback(),
        Err(_) => false,
    }
}

/// One private answer among the DNS results is enough to classify the name
/// as LAN: split-horizon setups routinely mix public and private records.
pub(super) fn host_resolves_to_lan(addresses: &[std::net::IpAddr]) -> bool {
    addresses.iter().any(ip_is_lan)
}

/// True when the resolved account would talk to the LAN or this machine.
///
/// Classification is layered: literal hosts are checked numerically, and a
/// plain DNS name is resolved once so a public-looking name that points
/// inside the network still triggers the consent prompt. This is a
/// best-effort classification for the consent UX — not a connect-time
/// guarantee against DNS rebinding (the resolver answer and the later
/// connection are not pinned together); the transport's URL policy remains
/// the enforcement layer.
pub(super) fn resolves_to_lan(base_url: &str) -> bool {
    use std::net::ToSocketAddrs as _;

    let Some(host) = url::Url::parse(base_url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_owned))
    else {
        return false;
    };
    if is_lan_host(&host) {
        return true;
    }
    // `ToSocketAddrs` needs a port; only the addresses matter here, so the
    // port is a placeholder and never dialed.
    let host_for_lookup = host.trim_start_matches('[').trim_end_matches(']');
    match (host_for_lookup, 0).to_socket_addrs() {
        Ok(addresses) => host_resolves_to_lan(&addresses.map(|addr| addr.ip()).collect::<Vec<_>>()),
        Err(_) => false,
    }
}
