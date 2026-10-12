//! The HTTP side: send a request for a profile, stream decoded events, list models, and run the
//! profile "Test". Retries 429/5xx twice with backoff (honouring `retry-after`) before any byte streams.

use std::collections::VecDeque;
use std::pin::Pin;
use std::time::{Duration, Instant};

use futures::{Stream, StreamExt as _};

use crate::sse::SseParser;
use crate::types::{ChatEvent, ChatRequest, ChatResponse, LlmError, Profile, Usage};
use crate::wire::{self, Decoder};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);

pub type EventStream = Pin<Box<dyn Stream<Item = Result<ChatEvent, LlmError>> + Send>>;

/// A profile plus its API key, ready to make requests.
#[derive(Clone)]
pub struct LlmClient {
    profile: Profile,
    key: Option<String>,
    http: reqwest::Client,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmClient")
            .field("profile", &self.profile.name)
            .field("key", &self.key.as_ref().map(|_| "••••••"))
            .finish()
    }
}

/// Remove the key, and anything that looks like part of it, from provider messages (OpenAI echoes a
/// masked prefix and suffix of a wrong key).
pub fn scrub(message: &str, key: Option<&str>) -> String {
    let mut out = message.to_owned();
    if let Some(key) = key.filter(|k| k.len() >= 4) {
        out = out.replace(key, "••••••");
        let head: String = key.chars().take(6).collect();
        let tail: String = key
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        out = out
            .split(' ')
            .map(|word| {
                let w =
                    word.trim_matches(|c: char| matches!(c, '.' | ',' | ':' | ';' | '"' | '\''));
                let looks_like_key = (w.len() >= 8 && head.len() == 6 && w.starts_with(&head))
                    || (w.len() >= 8 && w.contains("***"))
                    || (w.len() >= 12
                        && w.ends_with(&tail)
                        && w.starts_with(&key[..3.min(key.len())]));
                if looks_like_key {
                    word.replace(w, "••••••")
                } else {
                    word.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    out
}

fn map_status(status: u16, body: &str, key: Option<&str>) -> LlmError {
    let message = scrub(&wire::error_message(body), key);
    match status {
        401 | 403 => LlmError::Auth { status, message },
        429 => LlmError::RateLimited { message },
        _ => LlmError::Http { status, message },
    }
}

fn map_reqwest(err: &reqwest::Error) -> LlmError {
    if err.is_timeout() {
        LlmError::Timeout
    } else {
        // reqwest errors include the URL, never headers, so the key cannot leak here.
        LlmError::Network(err.to_string())
    }
}

fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    resp.headers()
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .map(|s| Duration::from_secs_f64(s.max(0.0)).min(MAX_RETRY_WAIT))
}

impl LlmClient {
    pub fn new(profile: Profile, key: Option<String>) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(Duration::from_millis(profile.timeout_ms.max(1_000)))
            .user_agent(concat!("gremliner/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        if let Some(k) = key.as_deref() {
            storage::secrets::register_secret(k);
        }
        Ok(Self {
            profile,
            key: key.filter(|k| !k.is_empty()),
            http,
        })
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Send with retries; returns the successful response.
    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        headers: &[(String, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response, LlmError> {
        let mut attempt = 0;
        loop {
            let mut rb = self.http.request(method.clone(), url);
            for (k, v) in headers {
                rb = rb.header(k, v);
            }
            if let Some(body) = body {
                rb = rb.json(body);
            }
            let resp = rb.send().await.map_err(|e| map_reqwest(&e))?;
            let status = resp.status().as_u16();
            if resp.status().is_success() {
                return Ok(resp);
            }
            let retryable = status == 429 || (500..600).contains(&status);
            if retryable && attempt < MAX_RETRIES {
                let wait = retry_after(&resp)
                    .unwrap_or_else(|| Duration::from_millis(500 * 3u64.pow(attempt)));
                attempt += 1;
                log::info!("LLM request returned {status}; retrying in {wait:?}");
                tokio::time::sleep(wait).await;
                continue;
            }
            let text = resp.text().await.unwrap_or_default();
            return Err(map_status(status, &text, self.key.as_deref()));
        }
    }

    /// Stream the reply to `req`. Profiles without streaming get one response decoded into events.
    pub async fn stream(&self, req: &ChatRequest) -> Result<EventStream, LlmError> {
        let streaming = self.profile.capabilities().streaming;
        let built = wire::build(&self.profile, self.key.as_deref(), req, streaming)?;
        let resp = self
            .send(
                reqwest::Method::POST,
                &built.url,
                &built.headers,
                Some(&built.body),
            )
            .await?;
        let kind = self.profile.kind;
        if !streaming {
            let text = resp.text().await.map_err(|e| map_reqwest(&e))?;
            let events = wire::decode_full(kind, &text)?;
            return Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))));
        }
        let key = self.key.clone();
        struct State {
            bytes: Pin<Box<dyn Stream<Item = reqwest::Result<bytes::Bytes>> + Send>>,
            parser: SseParser,
            decoder: Decoder,
            pending: VecDeque<Result<ChatEvent, LlmError>>,
            ended: bool,
        }
        let state = State {
            bytes: Box::pin(resp.bytes_stream()),
            parser: SseParser::new(),
            decoder: Decoder::new(kind),
            pending: VecDeque::new(),
            ended: false,
        };
        let stream = futures::stream::unfold(state, move |mut s| {
            let key = key.clone();
            async move {
                loop {
                    if let Some(next) = s.pending.pop_front() {
                        if next.is_err() {
                            // An error ends the stream.
                            s.ended = true;
                            s.pending.clear();
                        }
                        return Some((next, s));
                    }
                    if s.ended {
                        return None;
                    }
                    let events = match s.bytes.next().await {
                        Some(Ok(chunk)) => s.parser.feed(&chunk),
                        Some(Err(e)) => {
                            s.pending.push_back(Err(map_reqwest(&e)));
                            continue;
                        }
                        None => {
                            for ev in s.parser.finish() {
                                match s.decoder.event(&ev) {
                                    Ok(e) => s.pending.extend(e.into_iter().map(Ok)),
                                    Err(e) => {
                                        s.pending.push_back(Err(scrub_err(e, key.as_deref())))
                                    }
                                }
                            }
                            s.pending.extend(s.decoder.finish().into_iter().map(Ok));
                            s.ended = true;
                            continue;
                        }
                    };
                    for ev in events {
                        match s.decoder.event(&ev) {
                            Ok(e) => s.pending.extend(e.into_iter().map(Ok)),
                            Err(e) => s.pending.push_back(Err(scrub_err(e, key.as_deref()))),
                        }
                    }
                    if s.decoder.is_done() {
                        s.ended = true;
                    }
                }
            }
        });
        Ok(Box::pin(stream))
    }

    /// Run `req` to completion.
    pub async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, LlmError> {
        let mut stream = self.stream(req).await?;
        let mut out = ChatResponse::default();
        while let Some(event) = stream.next().await {
            out.push(event?);
        }
        Ok(out)
    }

    /// The provider's model ids.
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        if self.profile.requires_key() && self.key.is_none() {
            return Err(LlmError::MissingKey);
        }
        let (url, headers) = wire::models_request(&self.profile, self.key.as_deref());
        let resp = self
            .send(reqwest::Method::GET, &url, &headers, None)
            .await?;
        let text = resp.text().await.map_err(|e| map_reqwest(&e))?;
        wire::parse_models(self.profile.kind, &text)
    }

    /// The profile "Test": a short prompt, streamed to `on_text`, timed.
    pub async fn test(&self, mut on_text: impl FnMut(&str) + Send) -> Result<TestReport, LlmError> {
        let started = Instant::now();
        let mut first_token = None;
        let mut report = TestReport::default();
        let req = ChatRequest {
            system: Some("You are a helpful assistant. Answer briefly.".into()),
            max_tokens: Some(200),
            ..ChatRequest::user(
                "Say hello and name one thing graph databases are good at, in two short sentences.",
            )
        };
        let mut stream = self.stream(&req).await?;
        while let Some(event) = stream.next().await {
            match event? {
                ChatEvent::TextDelta(t) => {
                    first_token.get_or_insert_with(|| started.elapsed());
                    report.deltas += 1;
                    report.text.push_str(&t);
                    on_text(&t);
                }
                ChatEvent::Usage(u) => {
                    report.usage.input_tokens = u.input_tokens.or(report.usage.input_tokens);
                    report.usage.output_tokens = u.output_tokens.or(report.usage.output_tokens);
                }
                _ => {}
            }
        }
        report.first_token = first_token;
        report.total = started.elapsed();
        Ok(report)
    }
}

fn scrub_err(err: LlmError, key: Option<&str>) -> LlmError {
    match err {
        LlmError::Auth { status, message } => LlmError::Auth {
            status,
            message: scrub(&message, key),
        },
        LlmError::Http { status, message } => LlmError::Http {
            status,
            message: scrub(&message, key),
        },
        LlmError::RateLimited { message } => LlmError::RateLimited {
            message: scrub(&message, key),
        },
        other => other,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TestReport {
    pub text: String,
    /// Text events received (more than one: the reply streamed).
    pub deltas: usize,
    pub first_token: Option<Duration>,
    pub total: Duration,
    pub usage: Usage,
}

impl TestReport {
    /// `First token 320 ms · total 1.4 s · 12 in / 38 out tokens`.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ft) = self.first_token {
            parts.push(format!("first token {} ms", ft.as_millis()));
        }
        parts.push(format!("total {:.1} s", self.total.as_secs_f64()));
        if let (Some(i), Some(o)) = (self.usage.input_tokens, self.usage.output_tokens) {
            parts.push(format!("{i} in / {o} out tokens"));
        } else if let Some(o) = self.usage.output_tokens {
            parts.push(format!("{o} output tokens"));
        }
        let mut s = parts.join(" \u{b7} ");
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_key_and_masked_echoes() {
        let key = "sk-proj-abcdefghijklmnop1234";
        let msg = "Incorrect API key provided: sk-proj-************************1234. You can find your API key at https://platform.openai.com.";
        let out = scrub(msg, Some(key));
        assert!(!out.contains("sk-proj"), "{out}");
        assert!(out.contains("platform.openai.com"));
        assert_eq!(scrub(&format!("bad {key}"), Some(key)), "bad ••••••");
    }
}
