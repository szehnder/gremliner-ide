//! The provider abstraction (ADR-0015) and the connected [`Session`].
//!
//! Providers connect and detect capabilities; provider-specific HTTP features (Neptune's profile, explain
//! and statistics endpoints) are reached through the session's [`ServerApi`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use gremlin_client::Client;
use gremlin_syntax::dialect::Restrictions;

use crate::config::{ConnectionConfig, ProviderConfig, ProviderKind};
use crate::error::ProviderError;
use crate::gremlin_server::GremlinServerProvider;
use crate::neptune::{NeptuneProvider, compliance};

/// What the rest of the IDE may use on this connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// `.profile()` step / profile endpoint.
    pub profile: bool,
    /// `.explain()` step / explain endpoint.
    pub explain: bool,
    /// Gremlin-Groovy script evaluation (`eval` requests).
    pub script_eval: bool,
    /// A statistics summary for fast schema discovery (Neptune).
    pub statistics: bool,
    /// Amazon Neptune (dialect restrictions, HTTP profile/explain).
    pub neptune: bool,
    /// Lambdas, unsupported steps, and steps newer than the server.
    pub restrictions: Restrictions,
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
            statistics: false,
            neptune: false,
            restrictions: compliance::gremlin_server(version),
        }
    }

    /// Neptune: `api` is true when its HTTP endpoints answered (profile, explain, statistics).
    pub fn neptune(version: Option<&str>, api: bool) -> Self {
        Self {
            profile: true,
            explain: true,
            script_eval: true,
            statistics: api,
            neptune: true,
            restrictions: compliance::neptune(version),
        }
    }
}

/// Restrictions known before connecting, from the kind of connection alone.
pub fn restrictions_for(config: &ConnectionConfig) -> Restrictions {
    match config.provider.kind() {
        ProviderKind::GremlinServer => Restrictions::none(),
        ProviderKind::NeptuneAws => compliance::neptune(None),
        ProviderKind::NeptuneLocalStack => compliance::neptune(
            match &config.provider {
                ProviderConfig::NeptuneLocalStack(c) => c
                    .engine_version
                    .as_deref()
                    .and_then(crate::neptune::versions::tinkerpop_for),
                _ => None,
            }
            .as_deref(),
        ),
    }
}

/// Provider HTTP features beyond the Gremlin protocol. Each returns the endpoint's raw response body.
pub trait ServerApi: Send + Sync {
    /// Neptune `/propertygraph/statistics/summary?mode=detailed`.
    fn statistics_summary(&self) -> BoxFuture<'_, Result<String, ProviderError>>;
    /// Neptune `/gremlin/profile`.
    fn profile<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<String, ProviderError>>;
    /// Neptune `/gremlin/explain`.
    fn explain<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<String, ProviderError>>;
}

/// What a connection learned about its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// e.g. `3.7.3`, when the server reveals it.
    pub tinkerpop_version: Option<String>,
    /// Neptune engine release, e.g. `1.4.5.0.R2`.
    pub engine_version: Option<String>,
    /// Round trip of a trivial traversal.
    pub latency: Duration,
}

/// A live connection.
#[derive(Clone)]
pub struct Session {
    pub client: Client,
    pub info: ServerInfo,
    pub capabilities: Capabilities,
    /// Provider HTTP endpoints, when the server has them.
    pub api: Option<Arc<dyn ServerApi>>,
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
        ProviderConfig::NeptuneAws(neptune) => Ok(Box::new(NeptuneProvider::new(
            neptune.clone(),
            Duration::from_millis(config.timeout_ms),
        ))),
        ProviderConfig::NeptuneLocalStack(localstack) => Ok(Box::new(
            crate::neptune::localstack::LocalStackProvider::new(
                localstack.clone(),
                Duration::from_millis(config.timeout_ms),
            ),
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
