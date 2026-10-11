use std::time::Duration;

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("could not connect to {url}: {reason}")]
    Connect { url: String, reason: String },
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("server error {code}: {message}")]
    Server {
        code: u16,
        message: String,
        /// Exception class names reported by the server, outermost first.
        exceptions: Vec<String>,
        stack_trace: Option<String>,
    },
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("request cancelled")]
    Cancelled,
    #[error("connection closed")]
    Closed,
    #[error("protocol error: {0}")]
    Protocol(String),
}

impl Error {
    /// True for errors after which the connection should be discarded.
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Error::Connect { .. } | Error::Closed)
    }
}
