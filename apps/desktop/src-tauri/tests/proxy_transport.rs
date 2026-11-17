//! The explicit proxy setting must route provider traffic, not merely persist
//! a string. A loopback "proxy" records the absolute request URI; the origin
//! server must never see the request.

mod common;

use common::{json_response, spawn_router};
use tokenowl_lib::networking::{HttpClient, HttpClientConfig};

#[test]
fn an_explicit_proxy_receives_the_absolute_request_uri() {
    let proxy = spawn_router(|path| (json_response("200 OK", r#"{"ok":true}"#), path.to_owned()));
    let origin = spawn_router(|_| (json_response("200 OK", r#"{"origin":true}"#), String::new()));
    let target = format!("{}/v1/quota", origin.base_url);

    let client = HttpClient::new(HttpClientConfig {
        proxy: Some(proxy.base_url.clone()),
        ..HttpClientConfig::default()
    })
    .expect("proxy client builds");
    let response = tauri::async_runtime::block_on(async {
        // A single configured proxy URL keeps the historical meaning: every
        // route resolves to it, so an international request uses it.
        let http = client.route(&tokenowl_lib::domain::provider::ProviderRegion::Global);
        let request = http.get(&target);
        http.send_capped(request)
            .await
            .expect("proxied request succeeds")
    });

    assert_eq!(response.status, 200);
    assert_eq!(response.body, r#"{"ok":true}"#);
    let seen = proxy.requested_paths();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0], target,
        "an HTTP proxy receives the absolute target URI"
    );
    assert!(
        origin.requested_paths().is_empty(),
        "the origin must not be contacted directly when a proxy is configured"
    );
}

/// The routing policy is the real feature: a domestic provider must reach
/// the origin directly while an international one leaves through the proxy,
/// and neither request may take the other's path.
#[test]
fn region_routing_keeps_domestic_direct_and_international_proxied() {
    use std::sync::Arc;
    use tokenowl_lib::domain::provider::ProviderRegion;
    use tokenowl_lib::networking::{ProxyRoute, ProxyRouting, ProxyState};

    let proxy = spawn_router(|path| {
        (
            json_response("200 OK", r#"{"via":"proxy"}"#),
            path.to_owned(),
        )
    });
    let origin = spawn_router(|_| {
        (
            json_response("200 OK", r#"{"via":"origin"}"#),
            String::new(),
        )
    });
    let target = format!("{}/v1/quota", origin.base_url);

    let state = Arc::new(ProxyState::new(
        ProxyRouting {
            domestic: ProxyRoute::Direct,
            international: ProxyRoute::Custom,
        },
        Some(proxy.base_url.clone()),
    ));
    let client = HttpClient::with_state(HttpClientConfig::default(), Some(Arc::clone(&state)))
        .expect("routed client builds");

    let domestic = tauri::async_runtime::block_on(async {
        let http = client.route(&ProviderRegion::China);
        let request = http.get(&target);
        http.send_capped(request)
            .await
            .expect("direct request succeeds")
    });
    assert_eq!(domestic.body, r#"{"via":"origin"}"#);
    assert_eq!(origin.requested_paths().len(), 1);
    assert!(
        proxy.requested_paths().is_empty(),
        "domestic traffic must not touch the proxy"
    );

    let international = tauri::async_runtime::block_on(async {
        let http = client.route(&ProviderRegion::Global);
        let request = http.get(&target);
        http.send_capped(request)
            .await
            .expect("proxied request succeeds")
    });
    assert_eq!(international.body, r#"{"via":"proxy"}"#);
    assert_eq!(proxy.requested_paths().len(), 1);
    assert_eq!(
        proxy.requested_paths()[0],
        target,
        "the proxy receives the absolute target URI"
    );
    assert_eq!(
        origin.requested_paths().len(),
        1,
        "the international request never reached the origin directly"
    );
}

/// Changing the policy mid-session applies to the very next request: the
/// adapters share one live state and never need a restart.
#[test]
fn a_routing_change_applies_to_the_next_request() {
    use std::sync::Arc;
    use tokenowl_lib::domain::provider::ProviderRegion;
    use tokenowl_lib::networking::{ProxyRoute, ProxyRouting, ProxyState};

    let proxy = spawn_router(|path| {
        (
            json_response("200 OK", r#"{"via":"proxy"}"#),
            path.to_owned(),
        )
    });
    let origin = spawn_router(|_| {
        (
            json_response("200 OK", r#"{"via":"origin"}"#),
            String::new(),
        )
    });
    let target = format!("{}/v1/quota", origin.base_url);

    let state = Arc::new(ProxyState::new(
        ProxyRouting {
            domestic: ProxyRoute::Direct,
            international: ProxyRoute::Direct,
        },
        Some(proxy.base_url.clone()),
    ));
    let client = HttpClient::with_state(HttpClientConfig::default(), Some(Arc::clone(&state)))
        .expect("routed client builds");

    let first = tauri::async_runtime::block_on(async {
        let http = client.route(&ProviderRegion::Global);
        http.send_capped(http.get(&target))
            .await
            .expect("first request")
    });
    assert_eq!(first.body, r#"{"via":"origin"}"#);

    state.set_routing(ProxyRouting {
        domestic: ProxyRoute::Direct,
        international: ProxyRoute::Custom,
    });

    let second = tauri::async_runtime::block_on(async {
        let http = client.route(&ProviderRegion::Global);
        http.send_capped(http.get(&target))
            .await
            .expect("second request")
    });
    assert_eq!(second.body, r#"{"via":"proxy"}"#);
    assert_eq!(proxy.requested_paths().len(), 1);
}

/// The adapter must resolve the route from the account's own region: the
/// classification is what makes "domestic direct, international proxied"
/// true for real refreshes, not just for transport-level calls.
#[test]
fn the_adapter_uses_the_route_for_the_accounts_region() {
    use std::sync::Arc;
    use tokenowl_lib::credentials::CredentialStore;
    use tokenowl_lib::domain::provider::{AuthMethod, OpenAiCompatibleConfig, ProviderRegion};
    use tokenowl_lib::networking::{ProxyRoute, ProxyRouting, ProxyState};
    use tokenowl_lib::providers::builtin::openai::OpenAiCompatibleAdapter;

    let proxy = spawn_router(|path| (json_response("200 OK", r#"{"used":1}"#), path.to_owned()));
    let origin = spawn_router(|path| (json_response("200 OK", r#"{"used":2}"#), path.to_owned()));

    let state = Arc::new(ProxyState::new(
        ProxyRouting {
            domestic: ProxyRoute::Direct,
            international: ProxyRoute::Custom,
        },
        Some(proxy.base_url.clone()),
    ));

    let mut config =
        OpenAiCompatibleConfig::new("fixture-region", "Fixture", origin.base_url.clone());
    config.auth_method = AuthMethod::None;
    config.region = ProviderRegion::China;
    config.quota_endpoint = Some("/quota".to_owned());
    let adapter = OpenAiCompatibleAdapter::with_shared_proxy(config.clone(), Arc::clone(&state))
        .expect("adapter");

    let store = CredentialStore::default();
    let domestic = tauri::async_runtime::block_on(adapter.fetch(&config, "account-1", &store))
        .expect("domestic fetch");
    assert_eq!(domestic.snapshots.len(), 1);
    assert_eq!(origin.requested_paths().len(), 1);
    assert!(proxy.requested_paths().is_empty(), "China must stay direct");

    // The same adapter, same client, same config except the region: the
    // international account leaves through the proxy.
    let mut overseas = config.clone();
    overseas.region = ProviderRegion::Global;
    tauri::async_runtime::block_on(adapter.fetch(&overseas, "account-1", &store))
        .expect("international fetch");
    assert_eq!(proxy.requested_paths().len(), 1);
    assert_eq!(
        origin.requested_paths().len(),
        1,
        "the proxied request did not reach the origin directly"
    );
}
