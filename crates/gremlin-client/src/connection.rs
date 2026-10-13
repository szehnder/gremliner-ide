//! One WebSocket connection: a background task that owns the socket and multiplexes requests by id.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use futures::{SinkExt as _, StreamExt as _};
use gremlin_model::GValue;
use gremlin_model::graphson::encode;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tracing::{debug, warn};

use crate::{Batch, ClientConfig, Credentials, Error, Request};

const MIME: &str = "application/vnd.gremlin-v3.0+json";

/// Adds headers to the WebSocket upgrade request, e.g. AWS SigV4 for Neptune IAM (ADR-0016).
pub trait HandshakeSigner: Send + Sync {
    fn sign<'a>(
        &'a self,
        request: &'a mut http::Request<()>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

pub(crate) enum Outgoing {
    Request {
        id: uuid::Uuid,
        request: Request,
        timeout: Duration,
        tx: mpsc::UnboundedSender<Result<Batch, Error>>,
    },
    Cancel {
        id: uuid::Uuid,
    },
}

pub(crate) struct Connection {
    tx: mpsc::UnboundedSender<Outgoing>,
    alive: Arc<AtomicBool>,
}

struct Pending {
    tx: mpsc::UnboundedSender<Result<Batch, Error>>,
    auth_sent: bool,
    timeout: Duration,
}

struct Settings {
    credentials: Option<Credentials>,
    aliases: HashMap<String, String>,
    batch_size: Option<u32>,
    keep_raw: bool,
}

impl Connection {
    pub(crate) async fn open(config: &ClientConfig) -> Result<Connection, Error> {
        let connect_err = |reason: String| Error::Connect {
            url: config.url.clone(),
            reason,
        };
        let mut request = config
            .url
            .as_str()
            .into_client_request()
            .map_err(|e| connect_err(e.to_string()))?;
        if let Some(signer) = &config.signer {
            // Sign a header-only copy, then carry the headers over (tungstenite adds its own upgrade headers).
            let mut to_sign = http::Request::builder()
                .method("GET")
                .uri(request.uri().clone())
                .body(())
                .map_err(|e| connect_err(e.to_string()))?;
            signer
                .sign(&mut to_sign)
                .await
                .map_err(|e| Error::Auth(format!("signing failed: {e}")))?;
            for (k, v) in to_sign.headers() {
                request.headers_mut().insert(k.clone(), v.clone());
            }
        }
        let connector = config
            .accept_invalid_certs
            .then(|| tokio_tungstenite::Connector::Rustls(crate::tls::insecure_client_config()));
        let handshake = async {
            match &config.connect_addr {
                // Tunnel: the socket goes to `addr`, while Host and TLS SNI still come from the URL.
                Some(addr) => {
                    let tcp = tokio::net::TcpStream::connect(addr.as_str())
                        .await
                        .map_err(tokio_tungstenite::tungstenite::Error::Io)?;
                    let _ = tcp.set_nodelay(true);
                    tokio_tungstenite::client_async_tls_with_config(request, tcp, None, connector)
                        .await
                }
                None => {
                    tokio_tungstenite::connect_async_tls_with_config(
                        request, None, false, connector,
                    )
                    .await
                }
            }
        };
        let (ws, _resp) = tokio::time::timeout(config.connect_timeout, handshake)
            .await
            .map_err(|_| connect_err(format!("timed out after {:?}", config.connect_timeout)))?
            .map_err(|e| connect_err(e.to_string()))?;
        debug!(url = %config.url, "connected");

        let (tx, rx) = mpsc::unbounded_channel();
        let alive = Arc::new(AtomicBool::new(true));
        let settings = Settings {
            credentials: config.credentials.clone(),
            aliases: config.aliases.clone(),
            batch_size: config.batch_size,
            keep_raw: config.keep_raw,
        };
        tokio::spawn(run(ws, rx, settings, alive.clone()));
        Ok(Connection { tx, alive })
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub(crate) fn send(&self, msg: Outgoing) -> Result<(), Error> {
        self.tx.send(msg).map_err(|_| Error::Closed)
    }
}

fn frame(body: &Value) -> Message {
    let json = body.to_string();
    let mut bytes = Vec::with_capacity(1 + MIME.len() + json.len());
    bytes.push(MIME.len() as u8);
    bytes.extend_from_slice(MIME.as_bytes());
    bytes.extend_from_slice(json.as_bytes());
    Message::Binary(bytes.into())
}

fn request_body(id: uuid::Uuid, req: &Request, timeout: Duration, s: &Settings) -> Value {
    let bindings: serde_json::Map<String, Value> = req
        .bindings
        .iter()
        .map(|(k, v)| (k.clone(), encode(v)))
        .collect();
    let mut args = json!({
        "gremlin": req.script,
        "language": "gremlin-groovy",
        "bindings": bindings,
        "evaluationTimeout": timeout.as_millis() as u64,
    });
    if !s.aliases.is_empty() {
        args["aliases"] = json!(s.aliases);
    }
    if let Some(n) = req.batch_size.or(s.batch_size) {
        args["batchSize"] = json!(n);
    }
    json!({ "requestId": id.to_string(), "op": "eval", "processor": "", "args": args })
}

fn auth_body(id: uuid::Uuid, c: &Credentials) -> Value {
    let sasl = base64::engine::general_purpose::STANDARD
        .encode(format!("\0{}\0{}", c.username, c.password));
    json!({ "requestId": id.to_string(), "op": "authentication", "processor": "", "args": { "saslMechanism": "PLAIN", "sasl": sasl } })
}

/// A response frame, decoded in one streaming pass (`gremlin_model::fast`).
#[derive(serde::Deserialize)]
struct Response {
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    status: Status,
    #[serde(default)]
    result: Option<ResultBody>,
}

#[derive(serde::Deserialize)]
struct Status {
    code: u16,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    attributes: Option<GValue>,
}

#[derive(serde::Deserialize)]
struct ResultBody {
    #[serde(default)]
    data: Option<GValue>,
}

fn parse_response(bytes: Vec<u8>) -> Result<Response, String> {
    let mut bytes = bytes;
    simd_json::serde::from_slice::<Response>(&mut bytes).map_err(|e| e.to_string())
}

fn server_error(code: u16, message: String, attrs: Option<GValue>) -> Error {
    let lookup = |key: &str| match &attrs {
        Some(GValue::Map(m)) => m
            .get(&gremlin_model::GKey::String(key.to_string()))
            .cloned(),
        _ => None,
    };
    let exceptions = match lookup("exceptions") {
        Some(GValue::List(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    };
    let stack_trace = lookup("stackTrace").and_then(|v| v.as_str().map(String::from));
    Error::Server {
        code,
        message,
        exceptions,
        stack_trace,
    }
}

async fn run<S>(
    ws: S,
    mut rx: mpsc::UnboundedReceiver<Outgoing>,
    s: Settings,
    alive: Arc<AtomicBool>,
) where
    S: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let mut pending: HashMap<uuid::Uuid, Pending> = HashMap::new();
    let mut close_reason = Error::Closed;

    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Outgoing::Request { id, request, timeout, tx }) => {
                    let msg = frame(&request_body(id, &request, timeout, &s));
                    if let Err(e) = sink.send(msg).await {
                        let _ = tx.send(Err(Error::Protocol(format!("send failed: {e}"))));
                        close_reason = Error::Closed;
                        break;
                    }
                    pending.insert(id, Pending { tx, auth_sent: false, timeout });
                }
                Some(Outgoing::Cancel { id }) => {
                    pending.remove(&id);
                }
                None => break, // client dropped
            },
            incoming = stream.next() => {
                let bytes: Vec<u8> = match incoming {
                    Some(Ok(Message::Binary(b))) => b.to_vec(),
                    Some(Ok(Message::Text(t))) => t.as_bytes().to_vec(),
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => continue, // ping/pong handled by tungstenite
                    Some(Err(e)) => {
                        warn!(error = %e, "websocket error");
                        break;
                    }
                };
                let raw_text = s.keep_raw.then(|| String::from_utf8_lossy(&bytes).into_owned());
                let resp = match parse_response(bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "unparseable response frame");
                        continue;
                    }
                };
                let Some(id) = resp.request_id.as_deref().and_then(|s| uuid::Uuid::parse_str(s).ok()) else {
                    warn!(code = resp.status.code, "response without requestId");
                    continue;
                };
                let code = resp.status.code;
                let message = resp.status.message.clone().unwrap_or_default();
                let Some(p) = pending.get_mut(&id) else { continue }; // cancelled or unknown
                match code {
                    200 | 204 | 206 => {
                        let items = match resp.result.and_then(|r| r.data) {
                            Some(GValue::List(items)) => items,
                            None | Some(GValue::Null) => Vec::new(),
                            Some(other) => vec![other],
                        };
                        let attributes = if code == 206 { None } else { resp.status.attributes };
                        let _ = p.tx.send(Ok(Batch { items, status: code, attributes, raw: raw_text }));
                        if code != 206 {
                            pending.remove(&id);
                        }
                    }
                    407 => match (&s.credentials, p.auth_sent) {
                        (Some(c), false) => {
                            p.auth_sent = true;
                            if sink.send(frame(&auth_body(id, c))).await.is_err() {
                                break;
                            }
                        }
                        (None, _) => {
                            let _ = p.tx.send(Err(Error::Auth("the server requires credentials".into())));
                            pending.remove(&id);
                        }
                        (Some(_), true) => {
                            let _ = p.tx.send(Err(Error::Auth("credentials were rejected".into())));
                            pending.remove(&id);
                        }
                    },
                    401 => {
                        let _ = p.tx.send(Err(Error::Auth(if message.is_empty() { "unauthorized".into() } else { message })));
                        pending.remove(&id);
                    }
                    // SERVER_ERROR_TIMEOUT: the server's evaluationTimeout fired.
                    598 => {
                        let _ = p.tx.send(Err(Error::Timeout(p.timeout)));
                        pending.remove(&id);
                    }
                    _ => {
                        let _ = p.tx.send(Err(server_error(code, message, resp.status.attributes)));
                        pending.remove(&id);
                    }
                }
            }
        }
    }

    alive.store(false, Ordering::Release);
    for (_, p) in pending.drain() {
        let _ = p.tx.send(Err(close_reason.clone()));
    }
    let _ = sink.close().await;
}
