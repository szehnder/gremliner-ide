//! Provider errors in words a user can act on: what went wrong, and what to try.

use gremlin_client::Error as ClientError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Unreachable,
    Timeout,
    HostNotFound,
    Tls,
    NotGremlin,
    Auth,
    /// Temporary credentials need a fresh sign-in (e.g. an expired AWS SSO session).
    AuthExpired,
    Server,
    Unsupported,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct ProviderError {
    pub kind: FailureKind,
    /// One sentence: what happened.
    pub reason: String,
    /// One sentence: what to try next. Empty when there is nothing useful to suggest.
    pub suggestion: String,
    /// The underlying error text, for a details disclosure and logs.
    pub detail: String,
}

impl ProviderError {
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            kind: FailureKind::Unsupported,
            reason: reason.into(),
            suggestion: String::new(),
            detail: String::new(),
        }
    }

    /// Explain a client error for a connection to `endpoint` (`host:port`).
    pub fn from_client(err: &ClientError, endpoint: &str, path: &str) -> Self {
        let detail = err.to_string();
        let lower = detail.to_ascii_lowercase();
        let (kind, reason, suggestion) = match err {
            ClientError::Auth(_) => (
                FailureKind::Auth,
                "The server rejected the username or password.".to_owned(),
                "Check the credentials; Gremlin Server uses SASL PLAIN over the WebSocket.".to_owned(),
            ),
            ClientError::Server { message, .. } => (
                FailureKind::Server,
                format!("The server returned an error: {message}"),
                String::new(),
            ),
            ClientError::Timeout(d) => (
                FailureKind::Timeout,
                format!("The server did not answer within {} s.", d.as_secs()),
                "The server may be overloaded; try again or raise the timeout.".to_owned(),
            ),
            ClientError::Connect { .. } if lower.contains("refused") => (
                FailureKind::Unreachable,
                format!("Nothing is listening on {endpoint}."),
                "Check that the server is running and that the host and port are right.".to_owned(),
            ),
            ClientError::Connect { .. } if lower.contains("timed out") => (
                FailureKind::Timeout,
                format!("No response from {endpoint} before the connect timeout."),
                "Check the address and any VPN, firewall or security group between you and the server."
                    .to_owned(),
            ),
            ClientError::Connect { .. }
                if lower.contains("lookup")
                    || lower.contains("not known")
                    || lower.contains("no such host")
                    || lower.contains("dns") =>
            (
                FailureKind::HostNotFound,
                format!("The host in {endpoint} could not be found."),
                "Check the spelling of the host name.".to_owned(),
            ),
            ClientError::Connect { .. }
                if lower.contains("certificate") || lower.contains("tls") || lower.contains("handshake failure") =>
            (
                FailureKind::Tls,
                "The TLS connection failed.".to_owned(),
                "Check that TLS matches the server. For development servers with self-signed certificates, enable \"Accept invalid certificates\"."
                    .to_owned(),
            ),
            ClientError::Connect { .. } if lower.contains("http error") || lower.contains("404") => (
                FailureKind::NotGremlin,
                format!("{endpoint} answered, but not with a Gremlin WebSocket at {path}."),
                "Check the path (usually /gremlin) and whether the server expects TLS.".to_owned(),
            ),
            ClientError::Closed | ClientError::Protocol(_) => (
                FailureKind::NotGremlin,
                format!("{endpoint} closed the connection unexpectedly."),
                "Check that this is a Gremlin Server endpoint and that TLS matches the server.".to_owned(),
            ),
            _ => (
                FailureKind::Other,
                format!("Could not connect to {endpoint}."),
                String::new(),
            ),
        };
        Self {
            kind,
            reason,
            suggestion,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connect_err(reason: &str) -> ClientError {
        ClientError::Connect {
            url: "ws://h:1/gremlin".into(),
            reason: reason.into(),
        }
    }

    #[test]
    fn classifies_common_failures() {
        let cases = [
            (
                "IO error: Connection refused (os error 111)",
                FailureKind::Unreachable,
            ),
            ("timed out after 10s", FailureKind::Timeout),
            (
                "IO error: failed to lookup address information",
                FailureKind::HostNotFound,
            ),
            (
                "TLS error: invalid peer certificate: UnknownIssuer",
                FailureKind::Tls,
            ),
            ("HTTP error: 404 Not Found", FailureKind::NotGremlin),
        ];
        for (text, kind) in cases {
            let e = ProviderError::from_client(&connect_err(text), "h:1", "/gremlin");
            assert_eq!(e.kind, kind, "{text}");
            assert!(!e.suggestion.is_empty());
            assert_eq!(e.detail, connect_err(text).to_string());
        }
    }
}
