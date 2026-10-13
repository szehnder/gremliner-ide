//! Amazon Neptune (ADR-0015, ADR-0016): IAM SigV4 on the WebSocket handshake and on the HTTP endpoints
//! (`/status`, `/gremlin/profile`, `/gremlin/explain`, `/propertygraph/statistics/summary`), tunnels with
//! the real endpoint as Host and SNI, and Neptune's Gremlin restrictions.

pub mod aws;
pub mod compliance;
pub mod localstack;
pub mod sign;
pub mod status;
pub mod versions;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use futures::future::BoxFuture;
use gremlin_client::{Client, ClientConfig, Request};

use crate::config::{NeptuneConfig, ProviderKind};
use crate::error::{FailureKind, ProviderError};
use crate::provider::{Capabilities, GraphProvider, ServerApi, ServerInfo, Session};
pub use aws::{AwsFiles, AwsProfile, CredentialCache, list_profiles, parse_profiles};
pub use sign::NeptuneSigner;
pub use status::NeptuneStatus;

/// `/status` should answer quickly; a cluster that does not is reported without its version.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

pub struct NeptuneProvider {
    config: NeptuneConfig,
    timeout: Duration,
    /// Where AWS profiles are read from; `None` uses the usual locations.
    files: Option<AwsFiles>,
    /// Set to reuse credentials across connects (and in tests, to supply them).
    credentials: Option<Arc<CredentialCache>>,
    /// Engine version known without `/status` (LocalStack discovery); maps to a TinkerPop version.
    engine_hint: Option<String>,
    kind: ProviderKind,
}

impl NeptuneProvider {
    pub fn new(config: NeptuneConfig, timeout: Duration) -> Self {
        Self {
            config,
            timeout,
            files: None,
            credentials: None,
            engine_hint: None,
            kind: ProviderKind::NeptuneAws,
        }
    }

    /// The engine version when `/status` cannot tell (LocalStack's emulated clusters).
    pub fn with_engine_hint(mut self, engine: Option<String>) -> Self {
        self.engine_hint = engine;
        self
    }

    pub(crate) fn with_kind(mut self, kind: ProviderKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn with_files(mut self, files: AwsFiles) -> Self {
        self.files = Some(files);
        self
    }

    pub fn with_credentials(mut self, credentials: Arc<CredentialCache>) -> Self {
        self.credentials = Some(credentials);
        self
    }

    fn endpoint(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }

    fn region(&self) -> Result<String, ProviderError> {
        self.config.signing_region().ok_or_else(|| ProviderError {
            kind: FailureKind::Other,
            reason: "IAM authentication needs the cluster's AWS region.".into(),
            suggestion: "Enter the region, e.g. us-east-1.".into(),
            detail: String::new(),
        })
    }

    async fn credentials(&self) -> Result<Option<Arc<CredentialCache>>, ProviderError> {
        if !self.config.iam_auth {
            return Ok(None);
        }
        let cache = match &self.credentials {
            Some(c) => c.clone(),
            None => {
                CredentialCache::resolve(
                    self.config.profile.as_deref(),
                    self.config.signing_region().as_deref(),
                    self.files.as_ref(),
                )
                .await
            }
        };
        // Resolve now so an expired SSO session is reported as such, not as a failed handshake.
        cache.get().await?;
        Ok(Some(cache))
    }

    pub fn client_config(
        &self,
        credentials: Option<Arc<CredentialCache>>,
    ) -> Result<ClientConfig, ProviderError> {
        let mut cfg = ClientConfig::new(self.config.url());
        cfg.request_timeout = self.timeout;
        cfg.keep_raw = true;
        cfg.accept_invalid_certs = self.config.accept_invalid_certs;
        cfg.connect_addr = self.config.tunnel_addr().map(str::to_owned);
        if let Some(credentials) = credentials {
            cfg.signer = Some(Arc::new(NeptuneSigner {
                credentials,
                region: self.region()?,
            }));
        }
        Ok(cfg)
    }

    async fn open(&self) -> Result<Session, ProviderError> {
        let credentials = self.credentials().await?;
        let cfg = self.client_config(credentials.clone())?;
        let explain = |e: &gremlin_client::Error| neptune_error(e, &self.endpoint());
        let client = Client::connect(cfg).await.map_err(|e| explain(&e))?;
        let started = Instant::now();
        client
            .execute_request(Request::new("g.inject(1)").timeout(self.timeout))
            .await
            .map_err(|e| explain(&e))?;
        let latency = started.elapsed();
        let http = Arc::new(
            NeptuneHttp::new(&self.config, credentials, self.region().ok(), self.timeout).await?,
        );
        let status = match http.status().await {
            Ok(s) => Some(s),
            Err(err) => {
                log::info!("Neptune status unavailable on {}: {err}", self.endpoint());
                None
            }
        };
        let engine_version = status
            .as_ref()
            .map(|s| s.engine_version.clone())
            .or_else(|| self.engine_hint.clone());
        let tinkerpop_version = status
            .as_ref()
            .and_then(|s| s.tinkerpop_version.clone())
            .or_else(|| engine_version.as_deref().and_then(versions::tinkerpop_for));
        let capabilities = Capabilities::neptune(tinkerpop_version.as_deref(), status.is_some());
        Ok(Session {
            client,
            info: ServerInfo {
                tinkerpop_version,
                engine_version,
                latency,
            },
            capabilities,
            api: status.is_some().then_some(http as Arc<dyn ServerApi>),
        })
    }
}

impl GraphProvider for NeptuneProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn connect(&self) -> BoxFuture<'_, Result<Session, ProviderError>> {
        Box::pin(self.open())
    }
}

/// Client errors in Neptune terms: a refused upgrade is almost always IAM.
pub fn neptune_error(err: &gremlin_client::Error, endpoint: &str) -> ProviderError {
    let base = ProviderError::from_client(err, endpoint, "/gremlin");
    let lower = base.detail.to_ascii_lowercase();
    match err {
        gremlin_client::Error::Auth(msg) => ProviderError {
            kind: if lower.contains("sso") { FailureKind::AuthExpired } else { FailureKind::Auth },
            reason: msg.trim_start_matches("signing failed: ").to_owned(),
            suggestion: String::new(),
            detail: base.detail,
        },
        gremlin_client::Error::Connect { .. } if lower.contains("403") || lower.contains("401") => {
            ProviderError {
                kind: FailureKind::Auth,
                reason: format!("{endpoint} refused the connection (HTTP 403)."),
                suggestion: "With IAM authentication on the cluster, turn on IAM here and check the region and that your identity may call neptune-db:connect. Without IAM, turn it off here.".into(),
                detail: base.detail,
            }
        }
        _ => base,
    }
}

/// Neptune's HTTP endpoints, signed like the WebSocket.
pub struct NeptuneHttp {
    client: reqwest::Client,
    /// The endpoint's base URL: what is signed and sent as Host.
    base: String,
    /// Through a tunnel: the same host on the tunnel's port (reqwest always dials the URL's port).
    wire_base: Option<String>,
    region: Option<String>,
    credentials: Option<Arc<CredentialCache>>,
}

impl NeptuneHttp {
    pub async fn new(
        config: &NeptuneConfig,
        credentials: Option<Arc<CredentialCache>>,
        region: Option<String>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let mut builder = reqwest::Client::builder()
            .timeout(timeout)
            .danger_accept_invalid_certs(config.accept_invalid_certs);
        let mut wire_base = None;
        if let Some(tunnel) = config.tunnel_addr() {
            // Same as the WebSocket: the socket goes to the tunnel, Host and SNI stay the endpoint's.
            let addr = tokio::net::lookup_host(tunnel)
                .await
                .ok()
                .and_then(|mut a| a.next())
                .ok_or_else(|| ProviderError {
                    kind: FailureKind::HostNotFound,
                    reason: format!("The tunnel address {tunnel} could not be resolved."),
                    suggestion: "Use host:port, e.g. localhost:8182.".into(),
                    detail: String::new(),
                })?;
            builder = builder.resolve(&config.host, addr);
            let tunnelled = NeptuneConfig {
                port: addr.port(),
                ..config.clone()
            };
            wire_base = Some(tunnelled.http_base());
        }
        let client = builder.build().map_err(|e| ProviderError {
            kind: FailureKind::Other,
            reason: "Could not set up HTTPS for Neptune.".into(),
            suggestion: String::new(),
            detail: e.to_string(),
        })?;
        Ok(Self {
            client,
            base: config.http_base(),
            wire_base,
            region,
            credentials,
        })
    }

    async fn send(
        &self,
        method: http::Method,
        path: &str,
        body: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<String, ProviderError> {
        let fail = |reason: String, detail: String| ProviderError {
            kind: FailureKind::Server,
            reason,
            suggestion: String::new(),
            detail,
        };
        let url = format!("{}{path}", self.base);
        let bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        let mut request = http::Request::builder()
            .method(method.clone())
            .uri(&url)
            .body(())
            .map_err(|e| fail("Invalid Neptune URL.".into(), e.to_string()))?;
        if !bytes.is_empty() {
            request.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
        }
        if let Some(cache) = &self.credentials {
            let creds = cache.get().await?;
            let region = self.region.as_deref().unwrap_or_default();
            sign::sign_request(&mut request, &bytes, &creds, region, SystemTime::now())
                .map_err(|e| fail("Could not sign the request.".into(), e))?;
        }
        let wire_url = match &self.wire_base {
            Some(wire) => format!("{wire}{path}"),
            None => url.clone(),
        };
        let mut builder = self.client.request(method, &wire_url);
        for (k, v) in request.headers() {
            builder = builder.header(k, v);
        }
        if self.wire_base.is_some()
            && let Some(authority) = request.uri().authority()
        {
            builder = builder.header(http::header::HOST, authority.as_str());
        }
        if let Some(t) = timeout {
            builder = builder.timeout(t);
        }
        let response = builder
            .body(bytes)
            .send()
            .await
            .map_err(|e| fail(format!("{path} did not answer."), e.to_string()))?;
        let code = response.status();
        let text = response.text().await.map_err(|e| {
            fail(
                format!("{path} sent an unreadable response."),
                e.to_string(),
            )
        })?;
        if !code.is_success() {
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("detailedMessage")
                        .or(v.get("message"))?
                        .as_str()
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| text.chars().take(300).collect());
            return Err(fail(format!("{path} returned {code}: {message}"), text));
        }
        Ok(text)
    }

    pub async fn status(&self) -> Result<NeptuneStatus, ProviderError> {
        let text = self
            .send(http::Method::GET, "/status", None, Some(STATUS_TIMEOUT))
            .await?;
        NeptuneStatus::parse(&text).map_err(|e| ProviderError {
            kind: FailureKind::NotGremlin,
            reason: "The endpoint's /status is not Neptune's.".into(),
            suggestion: String::new(),
            detail: e,
        })
    }
}

impl ServerApi for NeptuneHttp {
    fn statistics_summary(&self) -> BoxFuture<'_, Result<String, ProviderError>> {
        Box::pin(self.send(
            http::Method::GET,
            "/propertygraph/statistics/summary?mode=detailed",
            None,
            None,
        ))
    }

    fn profile<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<String, ProviderError>> {
        Box::pin(self.send(
            http::Method::POST,
            "/gremlin/profile",
            Some(serde_json::json!({ "gremlin": query })),
            None,
        ))
    }

    fn explain<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<String, ProviderError>> {
        Box::pin(self.send(
            http::Method::POST,
            "/gremlin/explain",
            Some(serde_json::json!({ "gremlin": query })),
            None,
        ))
    }
}
