//! Test doubles (feature `testing`): a fake Neptune with optional TLS (self-signed for the cluster host
//! name), SigV4 verification of every request, `/status` from the recorded fixture and Gremlin over
//! WebSocket answering `[1]`; and a fake LocalStack answering `DescribeDBClusters`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::{SinkExt as _, StreamExt as _};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::Role;

pub const STATUS: &str = include_str!("../../../testenv/fixtures/neptune/status.json");

/// One request the server received.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    /// TLS SNI, when TLS was used.
    pub sni: Option<String>,
    /// The access key that signed it, when the signature verified.
    pub signed_by: Option<String>,
    pub signature_error: Option<String>,
    pub websocket: bool,
}

pub struct FakeNeptune {
    pub addr: std::net::SocketAddr,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    drop_tx: tokio::sync::watch::Sender<u32>,
}

impl FakeNeptune {
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// Close every open WebSocket, like an idle timeout or a failover.
    pub fn drop_connections(&self) {
        self.drop_tx.send_modify(|n| *n += 1);
    }
}

pub struct Options {
    /// Serve TLS with a self-signed certificate for this host.
    pub tls_host: Option<String>,
    /// Access key → secret, for verifying signatures. Empty: accept unsigned requests.
    pub keys: HashMap<String, String>,
    pub region: String,
    /// Answer `/status` (LocalStack's emulated clusters do not).
    pub status: bool,
}

pub async fn start(opts: Options) -> FakeNeptune {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let acceptor = opts.tls_host.as_ref().map(|host| {
        let cert = rcgen::generate_simple_self_signed(vec![host.clone()]).unwrap();
        let key =
            rustls::pki_types::PrivateKeyDer::try_from(cert.signing_key.serialize_der()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.cert.der().clone()], key)
            .unwrap();
        tokio_rustls::TlsAcceptor::from(Arc::new(config))
    });
    let opts = Arc::new(opts);
    let log = seen.clone();
    let (drop_tx, drop_rx) = tokio::sync::watch::channel(0u32);
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let (opts, log, acceptor) = (opts.clone(), log.clone(), acceptor.clone());
            let mut drops = drop_rx.clone();
            drops.mark_unchanged();
            tokio::spawn(async move {
                match acceptor {
                    Some(acceptor) => {
                        let Ok(tls) = acceptor.accept(tcp).await else {
                            return;
                        };
                        let sni = tls.get_ref().1.server_name().map(str::to_owned);
                        serve(tls, sni, &opts, &log, drops).await;
                    }
                    None => serve(tcp, None, &opts, &log, drops).await,
                }
            });
        }
    });
    FakeNeptune {
        addr,
        seen,
        drop_tx,
    }
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    sni: Option<String>,
    opts: &Options,
    log: &Mutex<Vec<Seen>>,
    mut drops: tokio::sync::watch::Receiver<u32>,
) {
    let Some((method, target, headers, body)) = read_request(&mut stream).await else {
        return;
    };
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let verdict = if opts.keys.is_empty() {
        Ok(String::new())
    } else {
        verify(
            &method,
            path,
            query,
            &headers,
            &body,
            &opts.keys,
            &opts.region,
            "neptune-db",
        )
    };
    let websocket = headers
        .get("upgrade")
        .is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    log.lock().unwrap().push(Seen {
        method: method.clone(),
        path: path.to_owned(),
        headers: headers.clone(),
        sni,
        signed_by: verdict.as_ref().ok().filter(|k| !k.is_empty()).cloned(),
        signature_error: verdict.as_ref().err().cloned(),
        websocket,
    });
    if verdict.is_err() {
        let _ = stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        return;
    }
    if !websocket {
        let (code, body) = match path {
            "/status" if opts.status => ("200 OK", STATUS.to_owned()),
            _ => ("404 Not Found", r#"{"message":"not found"}"#.to_owned()),
        };
        let response = format!(
            "HTTP/1.1 {code}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return;
    }
    let key = headers
        .get("sec-websocket-key")
        .cloned()
        .unwrap_or_default();
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: {accept}\r\n\r\n"
    );
    if stream.write_all(response.as_bytes()).await.is_err() {
        return;
    }
    let mut ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
    loop {
        let msg = tokio::select! {
            msg = ws.next() => msg,
            _ = drops.changed() => {
                let _ = ws.close(None).await;
                return;
            }
        };
        let Some(Ok(msg)) = msg else { return };
        let bytes = match msg {
            Message::Binary(b) => b.to_vec(),
            Message::Text(t) => t.as_bytes().to_vec(),
            Message::Close(_) => return,
            _ => continue,
        };
        let skip = 1 + *bytes.first().unwrap_or(&0) as usize;
        let Ok(request) =
            serde_json::from_slice::<serde_json::Value>(&bytes[skip.min(bytes.len())..])
        else {
            continue;
        };
        let id = request["requestId"].as_str().unwrap_or_default();
        let reply = serde_json::json!({
            "requestId": id,
            "status": {"message": "", "code": 200, "attributes": {"@type": "g:Map", "@value": []}},
            "result": {"data": {"@type": "g:List", "@value": [{"@type": "g:Int32", "@value": 1}]},
                       "meta": {"@type": "g:Map", "@value": []}}
        });
        if ws
            .send(Message::Text(reply.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }
}

/// Read one HTTP request: method, target, lower-cased headers, body.
async fn read_request<S: AsyncRead + Unpin>(
    stream: &mut S,
) -> Option<(String, String, HashMap<String, String>, Vec<u8>)> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.ok()?;
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let method = first.next().unwrap_or_default().to_owned();
    let target = first.next().unwrap_or_default().to_owned();
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    if length > 0 {
        stream.read_exact(&mut body).await.ok()?;
    }
    Some((method, target, headers, body))
}

/// The recorded LocalStack responses.
pub const CLUSTERS_XML: &str =
    include_str!("../../../testenv/fixtures/neptune/describe-db-clusters.xml");
pub const NO_CLUSTERS_XML: &str =
    include_str!("../../../testenv/fixtures/neptune/describe-db-clusters-empty.xml");
pub const NOT_AVAILABLE_XML: &str =
    include_str!("../../../testenv/fixtures/neptune/rds-not-available.xml");

/// A fake LocalStack edge that answers every request with `body` and status `code`. Requests are recorded
/// with their body (as `path`) and checked for an `rds` signature with key `test`/`test`.
pub struct FakeLocalStack {
    pub addr: std::net::SocketAddr,
    pub seen: Arc<Mutex<Vec<(Seen, String)>>>,
}

impl FakeLocalStack {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn requests(&self) -> Vec<(Seen, String)> {
        self.seen.lock().unwrap().clone()
    }
}

pub async fn start_localstack(code: u16, body: String, region: &str) -> FakeLocalStack {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let region = region.to_owned();
    tokio::spawn(async move {
        loop {
            let Ok((mut tcp, _)) = listener.accept().await else {
                return;
            };
            let (log, body, region) = (log.clone(), body.clone(), region.clone());
            tokio::spawn(async move {
                let Some((method, target, headers, request_body)) = read_request(&mut tcp).await
                else {
                    return;
                };
                let keys = HashMap::from([("test".to_owned(), "test".to_owned())]);
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let verdict = verify(
                    &method,
                    path,
                    query,
                    &headers,
                    &request_body,
                    &keys,
                    &region,
                    "rds",
                );
                log.lock().unwrap().push((
                    Seen {
                        method,
                        path: path.to_owned(),
                        headers,
                        sni: None,
                        signed_by: verdict.as_ref().ok().cloned(),
                        signature_error: verdict.err(),
                        websocket: false,
                    },
                    String::from_utf8_lossy(&request_body).to_string(),
                ));
                let reason = if code == 200 { "OK" } else { "Error" };
                let response = format!(
                    "HTTP/1.1 {code} {reason}\r\ncontent-type: text/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = tcp.write_all(response.as_bytes()).await;
                let _ = tcp.flush().await;
            });
        }
    });
    FakeLocalStack { addr, seen }
}

type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).unwrap();
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// An independent SigV4 check (what Neptune does): recompute the signature from the request as received.
/// Returns the access key id on success.
pub fn verify(
    method: &str,
    path: &str,
    query: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    keys: &HashMap<String, String>,
    region: &str,
    expected_service: &str,
) -> Result<String, String> {
    let auth = headers
        .get("authorization")
        .ok_or("no Authorization header")?;
    let rest = auth
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or("not AWS4-HMAC-SHA256")?;
    let field = |name: &str| {
        rest.split(", ")
            .find_map(|p| p.strip_prefix(&format!("{name}=")))
            .map(str::to_owned)
            .ok_or(format!("no {name}"))
    };
    let credential = field("Credential")?;
    let signed_headers = field("SignedHeaders")?;
    let signature = field("Signature")?;
    let mut scope = credential.split('/');
    let access_key = scope.next().unwrap_or_default().to_owned();
    let date = scope.next().unwrap_or_default().to_owned();
    let scope_region = scope.next().unwrap_or_default();
    let service = scope.next().unwrap_or_default();
    if scope_region != region {
        return Err(format!(
            "signed for region {scope_region}, expected {region}"
        ));
    }
    if service != expected_service {
        return Err(format!("signed for service {service}"));
    }
    let secret = keys
        .get(&access_key)
        .ok_or(format!("unknown key {access_key}"))?;
    let amz_date = headers.get("x-amz-date").ok_or("no X-Amz-Date")?;
    let canonical_headers: String = signed_headers
        .split(';')
        .map(|h| {
            let v = headers.get(h).map(String::as_str).unwrap_or_default();
            format!(
                "{h}:{}\n",
                v.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect();
    let mut query_pairs: Vec<&str> = query.split('&').filter(|p| !p.is_empty()).collect();
    query_pairs.sort_unstable();
    let canonical = format!(
        "{method}\n{path}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        query_pairs.join("&"),
        hex::encode(Sha256::digest(body))
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{date}/{region}/{service}/aws4_request\n{}",
        hex::encode(Sha256::digest(canonical.as_bytes()))
    );
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), &date);
    let k_region = hmac(&k_date, region);
    let k_service = hmac(&k_region, service);
    let k_signing = hmac(&k_service, "aws4_request");
    let expected = hex::encode(hmac(&k_signing, &string_to_sign));
    if expected == signature {
        Ok(access_key)
    } else {
        Err(format!(
            "signature mismatch; canonical request was:\n{canonical}"
        ))
    }
}
