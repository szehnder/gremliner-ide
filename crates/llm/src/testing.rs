//! A scripted HTTP/1.1 server for tests: each route answers with a fixed status and body, SSE bodies
//! are written event by event so clients really see a stream. Requests are recorded.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Route {
    pub method: &'static str,
    /// Matched as a prefix of the request path (with query).
    pub path: String,
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    /// Bodies for successive requests (the last repeats); overrides `body` when not empty.
    pub sequence: Vec<String>,
    served: Arc<std::sync::atomic::AtomicUsize>,
}

impl Route {
    pub fn json(method: &'static str, path: &str, status: u16, body: &str) -> Self {
        Self {
            method,
            path: path.into(),
            status,
            content_type: "application/json",
            body: body.into(),
            sequence: Vec::new(),
            served: Arc::default(),
        }
    }

    pub fn sse(path: &str, body: &str) -> Self {
        Self {
            method: "POST",
            path: path.into(),
            status: 200,
            content_type: "text/event-stream",
            body: body.into(),
            sequence: Vec::new(),
            served: Arc::default(),
        }
    }

    /// An SSE route answering successive requests with `bodies` in order.
    pub fn sse_sequence(path: &str, bodies: Vec<String>) -> Self {
        Self {
            sequence: bodies,
            ..Self::sse(path, "")
        }
    }

    fn next_body(&self) -> String {
        if self.sequence.is_empty() {
            return self.body.clone();
        }
        let n = self
            .served
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.sequence[n.min(self.sequence.len() - 1)].clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub struct MockServer {
    pub base: String,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    /// Start on an ephemeral port on the current tokio runtime.
    pub async fn start(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let routes = routes.clone();
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut socket).await else {
                        return;
                    };
                    let route = routes
                        .iter()
                        .find(|r| r.method == req.method && req.path.starts_with(&r.path))
                        .cloned();
                    recorded
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(req);
                    let route = route.unwrap_or_else(|| {
                        Route::json("", "", 404, r#"{"error":{"message":"no route"}}"#)
                    });
                    let head = format!(
                        "HTTP/1.1 {} X\r\ncontent-type: {}\r\nconnection: close\r\n\r\n",
                        route.status, route.content_type
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    let body = route.next_body();
                    if route.content_type == "text/event-stream" {
                        for chunk in body.split_inclusive("\n\n") {
                            let _ = socket.write_all(chunk.as_bytes()).await;
                            let _ = socket.flush().await;
                            tokio::time::sleep(Duration::from_millis(2)).await;
                        }
                    } else {
                        let _ = socket.write_all(body.as_bytes()).await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { base, requests }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let n = socket.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_owned()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = socket.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..]).into_owned();
    Some(Recorded {
        method,
        path,
        headers,
        body,
    })
}

/// An OpenAI-compatible SSE body whose reply text is `content`, streamed in three pieces.
pub fn openai_stream(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let third = chars.len() / 3;
    let pieces = [
        chars[..third].iter().collect::<String>(),
        chars[third..2 * third].iter().collect::<String>(),
        chars[2 * third..].iter().collect::<String>(),
    ];
    let mut out = String::new();
    for p in pieces {
        let chunk = serde_json::json!({"choices": [{"index": 0, "delta": {"content": p}}]});
        out.push_str(&format!("data: {chunk}\n\n"));
    }
    out.push_str("data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
    out
}
