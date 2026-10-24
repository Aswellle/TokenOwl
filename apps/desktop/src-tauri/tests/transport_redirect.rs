//! Local mock-server tests for the unified provider transport (acceptance
//! matrix R-08/R-09/R-10). Everything runs against loopback TCP servers; no
//! real provider endpoint is contacted.
//!
//! The redirect tests are the security property: a 3xx must never be
//! followed, the second host must never receive a connection (and therefore
//! never the auth-bearing headers), and the fetch must not report success.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokenowl_lib::credentials::CredentialStore;
use tokenowl_lib::domain::provider::{AuthMethod, OpenAiCompatibleConfig, ProviderErrorKind};
use tokenowl_lib::providers::builtin::openai::OpenAiCompatibleAdapter;

/// A loopback server that answers every accepted connection with the same
/// canned raw HTTP response, counting connections so tests can prove the
/// second host of a redirect chain was never contacted.
struct MockServer {
    base_url: String,
    hits: Arc<AtomicUsize>,
}

fn spawn_server(response: String) -> MockServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local address");
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            // Consume the request head; these tests only send header-only GETs.
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    MockServer {
        base_url: format!("http://{addr}"),
        hits,
    }
}

fn redirect_response(status: &str, location: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn ok_json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Adapter whose single quota endpoint points at `base_url/usage` with no
/// credentials, so every fetch issues exactly one GET.
fn adapter_for(base_url: &str) -> OpenAiCompatibleAdapter {
    let config = OpenAiCompatibleConfig {
        auth_method: AuthMethod::None,
        quota_endpoint: Some("/usage".to_owned()),
        ..OpenAiCompatibleConfig::new("generic.test", "Test", base_url)
    };
    OpenAiCompatibleAdapter::with_default_limits(config).expect("adapter builds")
}

fn fetch(adapter: &OpenAiCompatibleAdapter, base_url: &str) -> Result<(), String> {
    let config = OpenAiCompatibleConfig {
        auth_method: AuthMethod::None,
        quota_endpoint: Some("/usage".to_owned()),
        ..OpenAiCompatibleConfig::new("generic.test", "Test", base_url)
    };
    tauri::async_runtime::block_on(adapter.fetch(
        &config,
        "account-test",
        &CredentialStore::default(),
    ))
    .map(|_| ())
    .map_err(|error| format!("{:?}", error.code))
}

#[test]
fn redirect_statuses_are_not_followed_and_never_reach_the_second_host() {
    for status in [
        "301 Moved Permanently",
        "302 Found",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        // The second host answers with a perfectly valid quota payload: if the
        // transport followed the redirect the fetch would succeed, which is
        // exactly the behavior these tests forbid.
        let second = spawn_server(ok_json_response(r#"{"used": 25.0, "limit": 100.0}"#));
        let first = spawn_server(redirect_response(
            status,
            &format!("{}/usage", second.base_url),
        ));

        let adapter = adapter_for(&first.base_url);
        let outcome = fetch(&adapter, &first.base_url);

        assert!(
            outcome.is_err(),
            "`{status}` must not be followed; the fetch unexpectedly succeeded"
        );
        assert_eq!(
            second.hits.load(Ordering::SeqCst),
            0,
            "`{status}` was followed: the second host received a connection"
        );
    }
}

#[test]
fn declared_content_length_over_the_cap_is_rejected_before_the_body() {
    // 5 MiB declared against the 1 MiB adapter cap; the body is never read.
    let oversized = "HTTP/1.1 200 OK\r\nContent-Length: 5242880\r\nConnection: close\r\n\r\n{}";
    let server = spawn_server(oversized.to_owned());

    let adapter = adapter_for(&server.base_url);
    let error = fetch(&adapter, &server.base_url).expect_err("oversized response must be rejected");
    assert!(
        error.contains(&format!("{:?}", ProviderErrorKind::InvalidResponse)),
        "expected a size-limit rejection, got: {error}"
    );
}

#[test]
fn chunked_body_over_the_cap_is_rejected_mid_stream() {
    // Three 384 KiB chunks (1.125 MiB total) without Content-Length: the cap
    // must trip while streaming, not only on a declared length.
    let mut response = String::with_capacity(3 * 384 * 1024 + 256);
    write!(
        response,
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    )
    .expect("write to String cannot fail");
    let chunk = "x".repeat(384 * 1024);
    for _ in 0..3 {
        write!(response, "{:x}\r\n{}\r\n", chunk.len(), chunk)
            .expect("write to String cannot fail");
    }
    response.push_str("0\r\n\r\n");
    let server = spawn_server(response);

    let adapter = adapter_for(&server.base_url);
    let error = fetch(&adapter, &server.base_url).expect_err("oversized stream must be rejected");
    assert!(
        error.contains(&format!("{:?}", ProviderErrorKind::InvalidResponse)),
        "expected a size-limit rejection, got: {error}"
    );
}
