//! TinkerPop WebSocket wire client (ADR-0011, ADR-0012, ADR-0013).
//!
//! ```no_run
//! # async fn demo() -> Result<(), gremlin_client::Error> {
//! use gremlin_client::{Client, ClientConfig};
//! let client = Client::connect(ClientConfig::new("ws://localhost:8182/gremlin")).await?;
//! let result = client.execute("g.V().count()", Default::default()).await?;
//! println!("{:?} in {:?}", result.items, result.elapsed);
//! # Ok(()) }
//! ```
//!
//! Requests are sessionless `eval` scripts with GraphSON v3 bindings. Responses stream in batches
//! ([`ResultStream`]); each request has a client-side deadline and a server-side `evaluationTimeout`,
//! can be cancelled by dropping or calling [`ResultStream::cancel`], and connections are pooled and
//! re-established on demand.

mod connection;
mod error;
mod tls;

#[cfg(any(test, feature = "testenv"))]
pub mod testenv;

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::Stream;
use gremlin_model::GValue;
use indexmap::IndexMap;
use tokio::sync::{Mutex, mpsc};

pub use connection::HandshakeSigner;
pub use error::Error;

use connection::{Connection, Outgoing};

/// Parameter bindings sent with a request (ADR-0030).
pub type Bindings = IndexMap<String, GValue>;

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct ClientConfig {
    /// `ws://` or `wss://` URL including the path (usually `/gremlin`).
    pub url: String,
    pub credentials: Option<Credentials>,
    /// Adds headers to the WebSocket upgrade request (e.g. SigV4 for Neptune IAM).
    pub signer: Option<Arc<dyn HandshakeSigner>>,
    /// Maps traversal source names in scripts to server-side aliases, e.g. `g` → `gModern`.
    pub aliases: HashMap<String, String>,
    pub pool_size: usize,
    pub connect_timeout: Duration,
    /// Default per-request timeout (client deadline and server `evaluationTimeout`).
    pub request_timeout: Duration,
    /// Server result batch size (`batchSize` argument); `None` uses the server default.
    pub batch_size: Option<u32>,
    /// Keep the raw JSON of each response frame (for console/JSON views).
    pub keep_raw: bool,
    /// Skip TLS certificate verification (`wss://` to development servers with self-signed certs).
    pub accept_invalid_certs: bool,
    /// Open the socket to this `host:port` instead of the URL's, e.g. a local tunnel. The URL's host is
    /// still used for the Host header, TLS SNI and certificate checks.
    pub connect_addr: Option<String>,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("url", &self.url)
            .field("credentials", &self.credentials)
            .field("signer", &self.signer.as_ref().map(|_| "<signer>"))
            .field("aliases", &self.aliases)
            .field("pool_size", &self.pool_size)
            .field("connect_addr", &self.connect_addr)
            .finish_non_exhaustive()
    }
}

impl ClientConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            credentials: None,
            signer: None,
            aliases: HashMap::new(),
            pool_size: 2,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            batch_size: None,
            keep_raw: false,
            accept_invalid_certs: false,
            connect_addr: None,
        }
    }

    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.credentials = Some(Credentials {
            username: username.into(),
            password: password.into(),
        });
        self
    }

    /// Route scripts' `g` to the server-side traversal source `alias`.
    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.aliases.insert("g".to_string(), alias.into());
        self
    }
}

/// One request.
#[derive(Clone, Debug)]
pub struct Request {
    pub script: String,
    pub bindings: Bindings,
    pub timeout: Option<Duration>,
    pub batch_size: Option<u32>,
}

impl Request {
    pub fn new(script: impl Into<String>) -> Self {
        Self {
            script: script.into(),
            bindings: Bindings::new(),
            timeout: None,
            batch_size: None,
        }
    }

    pub fn binding(mut self, name: impl Into<String>, value: GValue) -> Self {
        self.bindings.insert(name.into(), value);
        self
    }

    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = Some(t);
        self
    }

    pub fn batch_size(mut self, n: u32) -> Self {
        self.batch_size = Some(n);
        self
    }
}

/// A batch of results from one response frame.
#[derive(Clone, Debug)]
pub struct Batch {
    pub items: Vec<GValue>,
    /// HTTP-like status: 206 = more to come, 200/204 = final.
    pub status: u16,
    /// Status attributes from the final frame (server-specific metadata).
    pub attributes: Option<GValue>,
    pub raw: Option<String>,
}

/// The complete result of [`Client::execute`].
#[derive(Clone, Debug)]
pub struct QueryResult {
    pub items: Vec<GValue>,
    pub elapsed: Duration,
    pub batches: usize,
    pub attributes: Option<GValue>,
    pub raw: Vec<String>,
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    config: ClientConfig,
    pool: Vec<Mutex<Option<Arc<Connection>>>>,
    next: AtomicUsize,
}

impl Client {
    /// Creates a client and opens one connection to validate the endpoint (and credentials, lazily on
    /// first request — Gremlin Server only challenges when a request is sent).
    pub async fn connect(config: ClientConfig) -> Result<Client, Error> {
        let pool = (0..config.pool_size.max(1))
            .map(|_| Mutex::new(None))
            .collect();
        let client = Client {
            inner: Arc::new(Inner {
                config,
                pool,
                next: AtomicUsize::new(0),
            }),
        };
        client.connection().await?;
        Ok(client)
    }

    pub fn config(&self) -> &ClientConfig {
        &self.inner.config
    }

    async fn connection(&self) -> Result<Arc<Connection>, Error> {
        let i = self.inner.next.fetch_add(1, Ordering::Relaxed) % self.inner.pool.len();
        let mut slot = self.inner.pool[i].lock().await;
        if let Some(conn) = slot.as_ref()
            && conn.is_alive()
        {
            return Ok(conn.clone());
        }
        let conn = Arc::new(Connection::open(&self.inner.config).await?);
        *slot = Some(conn.clone());
        Ok(conn)
    }

    /// Submits a request and returns a stream of result batches.
    pub async fn submit(&self, request: Request) -> Result<ResultStream, Error> {
        let conn = self.connection().await?;
        let timeout = request.timeout.unwrap_or(self.inner.config.request_timeout);
        let id = uuid::Uuid::new_v4();
        let (tx, rx) = mpsc::unbounded_channel();
        conn.send(Outgoing::Request {
            id,
            request,
            timeout,
            tx,
        })?;
        Ok(ResultStream {
            id,
            rx,
            conn: Some(conn),
            deadline: Box::pin(tokio::time::sleep(timeout + Duration::from_millis(500))),
            timeout,
            started: Instant::now(),
            done: false,
        })
    }

    /// Runs a script to completion.
    pub async fn execute(&self, script: &str, bindings: Bindings) -> Result<QueryResult, Error> {
        let mut req = Request::new(script);
        req.bindings = bindings;
        self.execute_request(req).await
    }

    pub async fn execute_request(&self, request: Request) -> Result<QueryResult, Error> {
        use futures::StreamExt as _;
        let mut stream = self.submit(request).await?;
        let mut out = QueryResult {
            items: Vec::new(),
            elapsed: Duration::ZERO,
            batches: 0,
            attributes: None,
            raw: Vec::new(),
        };
        while let Some(batch) = stream.next().await {
            let batch = batch?;
            out.items.extend(batch.items);
            out.batches += 1;
            if let Some(raw) = batch.raw {
                out.raw.push(raw);
            }
            if batch.attributes.is_some() {
                out.attributes = batch.attributes;
            }
        }
        out.elapsed = stream.elapsed();
        Ok(out)
    }
}

/// Streams result batches for one request. Dropping it cancels the request.
pub struct ResultStream {
    id: uuid::Uuid,
    rx: mpsc::UnboundedReceiver<Result<Batch, Error>>,
    conn: Option<Arc<Connection>>,
    deadline: Pin<Box<tokio::time::Sleep>>,
    timeout: Duration,
    started: Instant,
    done: bool,
}

impl ResultStream {
    pub fn request_id(&self) -> uuid::Uuid {
        self.id
    }

    /// Time since the request was submitted.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stops delivery of further batches. The connection stays usable.
    pub fn cancel(&mut self) {
        if !self.done {
            self.done = true;
            if let Some(conn) = &self.conn {
                let _ = conn.send(Outgoing::Cancel { id: self.id });
            }
        }
    }
}

impl Stream for ResultStream {
    type Item = Result<Batch, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(Ok(batch))) => {
                if batch.status != 206 {
                    self.done = true;
                }
                Poll::Ready(Some(Ok(batch)))
            }
            Poll::Ready(Some(Err(e))) => {
                self.done = true;
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                self.done = true;
                Poll::Ready(Some(Err(Error::Closed)))
            }
            Poll::Pending => match self.deadline.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    let timeout = self.timeout;
                    self.cancel();
                    Poll::Ready(Some(Err(Error::Timeout(timeout))))
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

impl Drop for ResultStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

use std::future::Future as _;
