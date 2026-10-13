//! Neptune emulated by LocalStack (ADR-0017): find clusters with a signed `DescribeDBClusters` Query call,
//! then connect to the cluster's endpoint and port with the Neptune transport.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use futures::future::BoxFuture;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::NeptuneProvider;
use super::aws::CredentialCache;
use crate::config::{LocalStackConfig, NeptuneConfig, ProviderKind};
use crate::error::{FailureKind, ProviderError};
use crate::provider::{GraphProvider, Session};

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// One emulated cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub id: String,
    pub engine_version: String,
    pub endpoint: String,
    pub port: u16,
    pub status: String,
    pub iam_auth: bool,
}

/// Why discovery found nothing to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    NotRunning {
        url: String,
        detail: String,
    },
    /// The running LocalStack has no Neptune (Neptune needs LocalStack Pro).
    NotAvailable {
        detail: String,
    },
    NoClusters {
        region: String,
    },
    Other {
        detail: String,
    },
}

/// The command that creates a cluster, for the "no clusters" message.
pub fn create_command(region: &str) -> String {
    format!(
        "awslocal neptune create-db-cluster --db-cluster-identifier my-graph --engine neptune --region {region}"
    )
}

impl From<DiscoveryError> for ProviderError {
    fn from(err: DiscoveryError) -> Self {
        match err {
            DiscoveryError::NotRunning { url, detail } => ProviderError {
                kind: FailureKind::Unreachable,
                reason: format!("LocalStack is not reachable at {url}."),
                suggestion: "Start it with `localstack start`, or check the LocalStack URL.".into(),
                detail,
            },
            DiscoveryError::NotAvailable { detail } => ProviderError {
                kind: FailureKind::Unsupported,
                reason: "This LocalStack does not provide Neptune.".into(),
                suggestion: "Neptune needs LocalStack Pro: set LOCALSTACK_AUTH_TOKEN and restart LocalStack.".into(),
                detail,
            },
            DiscoveryError::NoClusters { region } => ProviderError {
                kind: FailureKind::Other,
                reason: format!("No Neptune clusters exist in LocalStack ({region})."),
                suggestion: format!("Create one with `{}`.", create_command(&region)),
                detail: String::new(),
            },
            DiscoveryError::Other { detail } => ProviderError {
                kind: FailureKind::Server,
                reason: "LocalStack could not list Neptune clusters.".into(),
                suggestion: String::new(),
                detail,
            },
        }
    }
}

/// Neptune clusters in a `DescribeDBClusters` response (other engines are skipped).
pub fn parse_clusters(xml: &str) -> Result<Vec<Cluster>, String> {
    #[derive(Default)]
    struct Partial {
        id: String,
        engine: String,
        engine_version: String,
        endpoint: String,
        port: String,
        status: String,
        iam: String,
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut current: Option<Partial> = None;
    let mut saw_clusters = false;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "DBClusters" {
                    saw_clusters = true;
                }
                if name == "DBCluster" && path.last().is_some_and(|p| p == "DBClusters") {
                    current = Some(Partial::default());
                }
                path.push(name);
            }
            Event::Empty(e) => {
                if e.local_name().as_ref() == b"DBClusters" {
                    saw_clusters = true;
                }
            }
            Event::Text(t) => {
                let in_cluster = path.len() >= 2 && path[path.len() - 2] == "DBCluster";
                if let (true, Some(c)) = (in_cluster, current.as_mut()) {
                    let text = t.xml10_content().map_err(|e| e.to_string())?.to_string();
                    match path.last().map(String::as_str) {
                        Some("DBClusterIdentifier") => c.id = text,
                        Some("Engine") => c.engine = text,
                        Some("EngineVersion") => c.engine_version = text,
                        Some("Endpoint") => c.endpoint = text,
                        Some("Port") => c.port = text,
                        Some("Status") => c.status = text,
                        Some("IAMDatabaseAuthenticationEnabled") => c.iam = text,
                        _ => {}
                    }
                }
            }
            Event::End(_) => {
                if path.pop().as_deref() == Some("DBCluster")
                    && let Some(c) = current.take()
                    && c.engine.eq_ignore_ascii_case("neptune")
                {
                    out.push(Cluster {
                        port: c.port.parse().unwrap_or(8182),
                        id: c.id,
                        engine_version: c.engine_version,
                        endpoint: c.endpoint,
                        status: c.status,
                        iam_auth: c.iam.eq_ignore_ascii_case("true"),
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_clusters {
        return Err("not a DescribeDBClusters response".into());
    }
    Ok(out)
}

/// The `<Message>` (or JSON `message`) of an error response.
fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(m) = v
            .get("message")
            .or(v.get("Message"))
            .and_then(|m| m.as_str())
    {
        return m.to_owned();
    }
    let between = |open: &str, close: &str| {
        let start = body.find(open)? + open.len();
        let end = body[start..].find(close)? + start;
        Some(body[start..end].to_owned())
    };
    between("<Message>", "</Message>")
        .or_else(|| between("<Code>", "</Code>"))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// List the Neptune clusters LocalStack emulates.
pub async fn discover(config: &LocalStackConfig) -> Result<Vec<Cluster>, DiscoveryError> {
    let url = config
        .localstack_url
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let body = "Action=DescribeDBClusters&Version=2014-10-31\
                &Filters.Filter.1.Name=engine&Filters.Filter.1.Values.Value.1=neptune";
    let mut request = http::Request::builder()
        .method("POST")
        .uri(format!("{url}/"))
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded; charset=utf-8",
        )
        .body(())
        .map_err(|e| DiscoveryError::Other {
            detail: format!("invalid LocalStack URL {url}: {e}"),
        })?;
    let creds = Credentials::new(
        config.access_key.clone(),
        config.secret_key.clone(),
        None,
        None,
        "localstack",
    );
    super::sign::sign_service(
        &mut request,
        body.as_bytes(),
        &creds,
        &config.region,
        "rds",
        SystemTime::now(),
    )
    .map_err(|detail| DiscoveryError::Other { detail })?;
    let client = reqwest::Client::builder()
        .timeout(DISCOVERY_TIMEOUT)
        .build()
        .map_err(|e| DiscoveryError::Other {
            detail: e.to_string(),
        })?;
    let mut builder = client.post(format!("{url}/"));
    for (k, v) in request.headers() {
        builder = builder.header(k, v);
    }
    let response = builder.body(body).send().await.map_err(|e| {
        if e.is_connect() || e.is_timeout() {
            DiscoveryError::NotRunning {
                url: url.clone(),
                detail: e.to_string(),
            }
        } else {
            DiscoveryError::Other {
                detail: e.to_string(),
            }
        }
    })?;
    let code = response.status();
    let text = response.text().await.unwrap_or_default();
    if !code.is_success() {
        let message = error_message(&text);
        let lower = format!("{message} {text}").to_ascii_lowercase();
        let unavailable = lower.contains("pro feature")
            || lower.contains("not yet implemented")
            || lower.contains("license")
            || lower.contains("invalidaction")
            || code == http::StatusCode::NOT_IMPLEMENTED;
        return Err(if unavailable {
            DiscoveryError::NotAvailable { detail: message }
        } else {
            DiscoveryError::Other {
                detail: format!("{code}: {message}"),
            }
        });
    }
    let clusters = parse_clusters(&text).map_err(|detail| DiscoveryError::Other { detail })?;
    if clusters.is_empty() {
        return Err(DiscoveryError::NoClusters {
            region: config.region.clone(),
        });
    }
    Ok(clusters)
}

pub struct LocalStackProvider {
    config: LocalStackConfig,
    timeout: Duration,
}

impl LocalStackProvider {
    pub fn new(config: LocalStackConfig, timeout: Duration) -> Self {
        Self { config, timeout }
    }

    async fn open(&self) -> Result<Session, ProviderError> {
        let mut target = self.config.clone();
        // A picked cluster is looked up again: LocalStack assigns ports when it (re)starts.
        if let Some(id) = &self.config.cluster_id {
            let clusters = discover(&self.config).await.map_err(ProviderError::from)?;
            let cluster =
                clusters
                    .into_iter()
                    .find(|c| &c.id == id)
                    .ok_or_else(|| ProviderError {
                        kind: FailureKind::Other,
                        reason: format!("The LocalStack cluster \"{id}\" no longer exists."),
                        suggestion: format!(
                            "Pick another cluster, or create it with `{}`.",
                            create_command(&self.config.region).replace("my-graph", id)
                        ),
                        detail: String::new(),
                    })?;
            target.host = cluster.endpoint;
            target.port = cluster.port;
            target.engine_version = Some(cluster.engine_version).filter(|v| !v.is_empty());
        }
        let neptune = NeptuneConfig {
            host: target.host.clone(),
            port: target.port,
            region: target.region.clone(),
            iam_auth: target.iam_auth,
            profile: None,
            tunnel: None,
            accept_invalid_certs: false,
            tls: target.tls,
        };
        let mut provider = NeptuneProvider::new(neptune, self.timeout)
            .with_engine_hint(target.engine_version.clone())
            .with_kind(ProviderKind::NeptuneLocalStack);
        if target.iam_auth {
            let creds = Credentials::new(
                target.access_key.clone(),
                target.secret_key.clone(),
                None,
                None,
                "localstack",
            );
            provider = provider.with_credentials(Arc::new(CredentialCache::new(
                SharedCredentialsProvider::new(creds),
                None,
                false,
            )));
        }
        provider.connect().await
    }
}

impl GraphProvider for LocalStackProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::NeptuneLocalStack
    }

    fn connect(&self) -> BoxFuture<'_, Result<Session, ProviderError>> {
        Box::pin(self.open())
    }
}
