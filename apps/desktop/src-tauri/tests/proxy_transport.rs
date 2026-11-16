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
        client
            .send_capped(client.get(&target))
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
