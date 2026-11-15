//! Shared loopback mock-server helper for adapter-level integration tests.
//! Everything runs against 127.0.0.1 TCP servers; no real provider endpoint
//! is contacted.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A loopback server that answers every accepted connection with the same
/// canned raw HTTP response, counting requests so tests can prove exactly
/// which endpoints were contacted.
pub struct RecordingServer {
    pub base_url: String,
    pub paths: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl RecordingServer {
    pub fn requested_paths(&self) -> Vec<String> {
        self.paths.lock().expect("paths lock").clone()
    }
}

/// Spawns a server answering every request with one canned response.
pub fn spawn_server(response: String) -> RecordingServer {
    spawn_router(move |path| (response.clone(), path.to_owned()))
}

/// Spawns a server that picks its response per request path via `route`.
/// The connection count feeds the shared path log; a per-request route lets
/// tests serve different endpoints from one process.
pub fn spawn_router(route: impl Fn(&str) -> (String, String) + Send + 'static) -> RecordingServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local address");
    let hits = Arc::new(AtomicUsize::new(0));
    let paths: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let counter = Arc::clone(&hits);
    let seen = Arc::clone(&paths);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0u8; 8192];
            let request = stream.read(&mut buffer).unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..request]).to_string();
            let path = head
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned();
            seen.lock().expect("paths lock").push(path.clone());
            let (response, _) = route(&path);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    RecordingServer {
        base_url: format!("http://{addr}"),
        paths,
    }
}

/// A canned JSON response with the given status code.
pub fn json_response(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
