//! Helpers for integration tests that need the Docker test environment (ADR-0026).
//!
//! Start it with `cargo xtask testenv up && cargo xtask testenv seed`. Tests call
//! [`require!`](crate::require_testenv) and skip (pass with a message) when the server is not reachable,
//! so `cargo test` stays green on machines without Docker. CI always runs with the testenv up.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Gremlin Server without authentication. Traversal sources: `g` (air-routes), `gModern`, `gCrew`, `gSynth`.
pub const DEFAULT_URL: &str = "ws://localhost:8182/gremlin";
/// Gremlin Server with SASL PLAIN over WebSockets.
pub const AUTH_URL: &str = "ws://localhost:8183/gremlin";
pub const AUTH_USER: &str = "stephen";
pub const AUTH_PASSWORD: &str = "password";

/// The WebSocket URL of the test server, overridable with `GREMLINER_TESTENV`.
pub fn url() -> String {
    std::env::var("GREMLINER_TESTENV").unwrap_or_else(|_| DEFAULT_URL.to_string())
}

/// True when something is listening on the test server's host and port.
pub fn is_up(url: &str) -> bool {
    host_port(url)
        .and_then(|hp| hp.to_socket_addrs().ok()?.next())
        .is_some_and(|addr: SocketAddr| {
            TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
        })
}

fn host_port(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    Some(rest.split('/').next()?.to_string())
}

/// Skip the current test (by returning early) when the testenv is not reachable.
#[macro_export]
macro_rules! require_testenv {
    () => {
        $crate::require_testenv!($crate::testenv::url())
    };
    ($url:expr) => {{
        let url: String = ($url).to_string();
        if !$crate::testenv::is_up(&url) {
            eprintln!("SKIPPED: testenv not reachable at {url}; run `cargo xtask testenv up && cargo xtask testenv seed`");
            return;
        }
        url
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_port() {
        assert_eq!(
            host_port("ws://localhost:8182/gremlin").as_deref(),
            Some("localhost:8182")
        );
        assert_eq!(
            host_port("wss://db.example.com:8182").as_deref(),
            Some("db.example.com:8182")
        );
    }

    #[test]
    fn closed_port_is_not_up() {
        assert!(!is_up("ws://127.0.0.1:9/gremlin"));
    }

    #[test]
    fn skips_or_connects() {
        let url = crate::require_testenv!();
        assert!(is_up(&url));
    }
}
