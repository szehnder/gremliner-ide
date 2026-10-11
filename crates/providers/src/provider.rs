//! The provider abstraction (ADR-0015) and the connected [`Session`].
//!
//! This change implements connect, capability detection and connection testing. `profile`, `explain` and
//! `schema_summary` join the trait with the changes that use them (query-profile-explain, schema-discovery).

use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use gremlin_client::Client;

use crate::config::{ConnectionConfig, ProviderConfig, ProviderKind};
use crate::error::ProviderError;
use crate::gremlin_server::GremlinServerProvider;

/// What the rest of the IDE may use on this connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `.profile()` step / profile endpoint.
    pub profile: bool,
    /// `.explain()` step / explain endpoint.
    pub explain: bool,
    /// Gremlin-Groovy script evaluation (`eval` requests).
    pub script_eval: bool,
}

impl Capabilities {
    /// TinkerPop 3.x Gremlin Server: profile and explain exist on every 3.x release we support.
    pub fn gremlin_server(version: Option<&str>) -> Self {
        let major = version.and_then(|v| v.split('.').next()?.parse::<u32>().ok());
        let known_3x = major.is_none_or(|m| m == 3);
        Self {
            profile: known_3x,
            explain: known_3x,
            script_eval: true,
        }
    }
}

/// What a connection learned about its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// e.g. `3.7.3`, when the server reveals it.
    pub tinkerpop_version: Option<String>,
    /// Round trip of a trivial traversal.
    pub latency: Duration,
}

/// A live connection.
#[derive(Clone)]
pub struct Session {
    pub client: Client,
    pub info: ServerInfo,
    pub capabilities: Capabilities,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("info", &self.info)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

pub trait GraphProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    /// Open a connection and detect what the server supports.
    fn connect(&self) -> BoxFuture<'_, Result<Session, ProviderError>>;
}

/// Build the provider for a saved connection. `password` comes from the credential store.
pub fn provider_for(
    config: &ConnectionConfig,
    password: Option<String>,
) -> Result<Box<dyn GraphProvider>, ProviderError> {
    match &config.provider {
        ProviderConfig::GremlinServer(server) => Ok(Box::new(GremlinServerProvider::new(
            server.clone(),
            password,
            Duration::from_millis(config.timeout_ms),
        ))),
        ProviderConfig::NeptuneAws(_) => Err(ProviderError::unsupported(
            "Amazon Neptune connections are not available in this build yet.",
        )),
    }
}

/// The outcome of "Test connection".
#[derive(Debug, Clone)]
pub struct TestReport {
    pub result: Result<ServerInfo, ProviderError>,
    pub elapsed: Duration,
}

/// Connect, detect, and disconnect. Never saves anything.
pub async fn test_connection(config: &ConnectionConfig, password: Option<String>) -> TestReport {
    let started = Instant::now();
    let result = match provider_for(config, password) {
        Ok(provider) => provider.connect().await.map(|session| session.info),
        Err(err) => Err(err),
    };
    TestReport {
        result,
        elapsed: started.elapsed(),
    }
}
