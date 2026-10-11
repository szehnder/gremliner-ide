//! Apache TinkerPop Gremlin Server (and compatible endpoints).

use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use gremlin_client::{Bindings, Client, ClientConfig, Request};

use crate::config::{GremlinServerConfig, ProviderKind};
use crate::error::ProviderError;
use crate::provider::{Capabilities, GraphProvider, ServerInfo, Session};

/// How long version detection may take before it is skipped.
const DETECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct GremlinServerProvider {
    config: GremlinServerConfig,
    password: Option<String>,
    timeout: Duration,
}

impl GremlinServerProvider {
    pub fn new(config: GremlinServerConfig, password: Option<String>, timeout: Duration) -> Self {
        Self {
            config,
            password,
            timeout,
        }
    }

    pub fn client_config(&self) -> ClientConfig {
        let mut cfg = ClientConfig::new(self.config.url());
        cfg.request_timeout = self.timeout;
        cfg.accept_invalid_certs = self.config.accept_invalid_certs;
        // The console's raw mode shows responses exactly as received.
        cfg.keep_raw = true;
        if let Some(user) = self.config.username.as_deref().filter(|u| !u.is_empty()) {
            cfg = cfg.credentials(user, self.password.clone().unwrap_or_default());
        }
        let alias = self.config.alias.trim();
        if !alias.is_empty() && alias != "g" {
            cfg = cfg.alias(alias);
        }
        cfg
    }

    fn endpoint(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }

    async fn open(&self) -> Result<Session, ProviderError> {
        let explain = |e: &gremlin_client::Error| {
            ProviderError::from_client(e, &self.endpoint(), &self.config.path)
        };
        let client = Client::connect(self.client_config())
            .await
            .map_err(|e| explain(&e))?;
        // A trivial traversal proves the alias resolves and the credentials work, and measures latency.
        let started = Instant::now();
        client
            .execute_request(Request::new("g.inject(1)").timeout(self.timeout))
            .await
            .map_err(|e| explain(&e))?;
        let latency = started.elapsed();
        let tinkerpop_version = detect_version(&client).await;
        let capabilities = Capabilities::gremlin_server(tinkerpop_version.as_deref());
        Ok(Session {
            client,
            info: ServerInfo {
                tinkerpop_version,
                latency,
            },
            capabilities,
        })
    }
}

impl GraphProvider for GremlinServerProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::GremlinServer
    }

    fn connect(&self) -> BoxFuture<'_, Result<Session, ProviderError>> {
        Box::pin(self.open())
    }
}

/// `Gremlin.version()` is a Groovy call, so it works where scripts are evaluated with Gremlin-Groovy.
/// Servers that refuse it simply report no version.
async fn detect_version(client: &Client) -> Option<String> {
    let request = Request::new("Gremlin.version()").timeout(DETECT_TIMEOUT);
    match client.execute_request(request).await {
        Ok(result) => result
            .items
            .first()
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        Err(err) => {
            log::debug!("version detection failed: {err}");
            None
        }
    }
}

/// Run a script on a session, for callers that only need values.
pub async fn execute(
    session: &Session,
    script: &str,
) -> Result<Vec<gremlin_model::GValue>, ProviderError> {
    session
        .client
        .execute(script, Bindings::default())
        .await
        .map(|r| r.items)
        .map_err(|e| ProviderError::from_client(&e, "server", "/gremlin"))
}
