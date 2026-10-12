//! The three wire formats (ADR-0018): building requests, decoding streamed and whole responses, model
//! lists and error bodies. Pure functions over JSON, so recorded fixtures test them without a network.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::sse::SseEvent;
use crate::types::{
    ChatEvent, ChatRequest, LlmError, Part, Profile, ProviderKind, Role, StopReason, Usage,
};

pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// An HTTP request ready to send.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

fn auth_headers(profile: &Profile, key: Option<&str>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    match profile.kind {
        ProviderKind::Anthropic => {
            if let Some(k) = key {
                headers.push(("x-api-key".into(), k.into()));
            }
            headers.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
        }
        ProviderKind::Gemini => {
            if let Some(k) = key {
                headers.push(("x-goog-api-key".into(), k.into()));
            }
        }
        ProviderKind::OpenAiCompatible => {
            if let Some(k) = key.filter(|k| !k.is_empty()) {
                headers.push(("authorization".into(), format!("Bearer {k}")));
            }
        }
    }
    headers
}

/// Gemini model ids from the model list carry a `models/` prefix.
fn gemini_model(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

/// Build the chat request for `profile`. `stream` asks for a streamed response.
pub fn build(
    profile: &Profile,
    key: Option<&str>,
    req: &ChatRequest,
    stream: bool,
) -> Result<HttpRequest, LlmError> {
    if profile.model.trim().is_empty() {
        return Err(LlmError::NoModel);
    }
    if profile.requires_key() && key.is_none_or(str::is_empty) {
        return Err(LlmError::MissingKey);
    }
    let caps = profile.capabilities();
    let tools = if caps.tools {
        req.tools.as_slice()
    } else {
        &[]
    };
    let temperature = req.temperature.unwrap_or(profile.temperature);
    let max_tokens = req.max_tokens.unwrap_or(profile.max_tokens);
    let base = profile.base();
    let mut headers = auth_headers(profile, key);
    headers.push(("content-type".into(), "application/json".into()));
    let (url, body) = match profile.kind {
        ProviderKind::Anthropic => {
            let mut body = json!({
                "model": profile.model,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "messages": anthropic_messages(req),
            });
            if let Some(system) = &req.system {
                body["system"] = json!(system);
            }
            let mut tool_list: Vec<Value> = tools
                .iter()
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.parameters}))
                .collect();
            if let Some(schema) = &req.output_schema {
                // Structured output: a forced tool whose input is the answer.
                tool_list.push(json!({
                    "name": schema.name,
                    "description": "Return the answer in exactly this structure.",
                    "input_schema": schema.schema,
                }));
                body["tool_choice"] = json!({"type": "tool", "name": schema.name});
            }
            if !tool_list.is_empty() {
                body["tools"] = Value::Array(tool_list);
            }
            if stream {
                body["stream"] = json!(true);
            }
            (format!("{base}/v1/messages"), body)
        }
        ProviderKind::Gemini => {
            let model = gemini_model(&profile.model);
            let url = if stream {
                format!("{base}/v1beta/models/{model}:streamGenerateContent?alt=sse")
            } else {
                format!("{base}/v1beta/models/{model}:generateContent")
            };
            let mut config = json!({"temperature": temperature, "maxOutputTokens": max_tokens});
            let mut body = json!({"contents": gemini_contents(req)});
            if let Some(system) = &req.system {
                body["systemInstruction"] = json!({"parts": [{"text": system}]});
            }
            match &req.output_schema {
                Some(schema) if caps.json_schema => {
                    config["responseMimeType"] = json!("application/json");
                    config["responseSchema"] = gemini_schema(&schema.schema);
                }
                _ if !tools.is_empty() => {
                    body["tools"] = json!([{"functionDeclarations": tools.iter().map(|t| json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": gemini_schema(&t.parameters),
                    })).collect::<Vec<_>>()}]);
                }
                _ => {}
            }
            body["generationConfig"] = config;
            (url, body)
        }
        ProviderKind::OpenAiCompatible => {
            let mut messages = Vec::new();
            if let Some(system) = &req.system {
                messages.push(json!({"role": "system", "content": system}));
            }
            messages.extend(openai_messages(req));
            let tokens_field = if base.contains("api.openai.com") {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            let mut body = json!({
                "model": profile.model,
                "messages": messages,
                "temperature": temperature,
            });
            body[tokens_field] = json!(max_tokens);
            if stream {
                body["stream"] = json!(true);
                body["stream_options"] = json!({"include_usage": true});
            }
            if !tools.is_empty() {
                body["tools"] = Value::Array(
                    tools
                        .iter()
                        .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}}))
                        .collect(),
                );
            }
            if let Some(schema) = &req.output_schema {
                if caps.json_schema {
                    body["response_format"] = json!({
                        "type": "json_schema",
                        "json_schema": {"name": schema.name, "schema": schema.schema, "strict": false},
                    });
                } else if caps.json_mode {
                    body["response_format"] = json!({"type": "json_object"});
                }
            }
            (format!("{base}/chat/completions"), body)
        }
    };
    Ok(HttpRequest { url, headers, body })
}

fn anthropic_messages(req: &ChatRequest) -> Vec<Value> {
    req.messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            let content: Vec<Value> = m
                .parts
                .iter()
                .map(|p| match p {
                    Part::Text { text } => json!({"type": "text", "text": text}),
                    Part::ToolCall { id, name, args } => {
                        json!({"type": "tool_use", "id": id, "name": name, "input": args})
                    }
                    Part::ToolResult {
                        id,
                        content,
                        is_error,
                        ..
                    } => json!({"type": "tool_result", "tool_use_id": id, "content": content, "is_error": is_error}),
                })
                .collect();
            json!({"role": role, "content": content})
        })
        .collect()
}

fn gemini_contents(req: &ChatRequest) -> Vec<Value> {
    req.messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "model",
            };
            let parts: Vec<Value> = m
                .parts
                .iter()
                .map(|p| match p {
                    Part::Text { text } => json!({"text": text}),
                    Part::ToolCall { name, args, .. } => {
                        json!({"functionCall": {"name": name, "args": args}})
                    }
                    Part::ToolResult { name, content, .. } => {
                        json!({"functionResponse": {"name": name, "response": {"content": content}}})
                    }
                })
                .collect();
            json!({"role": role, "parts": parts})
        })
        .collect()
}

/// Gemini accepts an OpenAPI subset: drop JSON Schema keywords it rejects.
pub fn gemini_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "additionalProperties" | "$schema" | "$id" | "title" | "default" | "examples"
                ) {
                    continue;
                }
                out.insert(k.clone(), gemini_schema(v));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(gemini_schema).collect()),
        other => other.clone(),
    }
}

fn openai_messages(req: &ChatRequest) -> Vec<Value> {
    let mut out = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::Assistant => {
                let text = m.text();
                let calls: Vec<Value> = m
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::ToolCall { id, name, args } => Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {"name": name, "arguments": args.to_string()},
                        })),
                        _ => None,
                    })
                    .collect();
                let mut msg = json!({"role": "assistant", "content": text});
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                out.push(msg);
            }
            Role::User => {
                for p in &m.parts {
                    if let Part::ToolResult { id, content, .. } = p {
                        out.push(json!({"role": "tool", "tool_call_id": id, "content": content}));
                    }
                }
                let text = m.text();
                if !text.is_empty() {
                    out.push(json!({"role": "user", "content": text}));
                }
            }
        }
    }
    out
}

// ---- decoding ----------------------------------------------------------------------------------

fn stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" | "stop" | "STOP" | "stop_sequence" => StopReason::EndTurn,
        "max_tokens" | "length" | "MAX_TOKENS" => StopReason::MaxTokens,
        "tool_use" | "tool_calls" | "function_call" => StopReason::ToolUse,
        other => StopReason::Other(other.to_owned()),
    }
}

fn parse(data: &str) -> Result<Value, LlmError> {
    serde_json::from_str(data).map_err(|e| LlmError::Protocol(format!("bad JSON event: {e}")))
}

/// An error object inside a stream or body (`{"error": {...}}` or `{"type": "error", ...}`).
fn embedded_error(v: &Value) -> Option<LlmError> {
    let err = v.get("error")?;
    if err.is_null() {
        return None;
    }
    let message = error_text(err);
    let status = err
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|c| u16::try_from(c).ok())
        .unwrap_or(500);
    Some(match (status, err.get("type").and_then(Value::as_str)) {
        (401 | 403, _) | (_, Some("authentication_error" | "permission_error")) => LlmError::Auth {
            status: if status == 500 { 401 } else { status },
            message,
        },
        (429, _) | (_, Some("rate_limit_error")) => LlmError::RateLimited { message },
        _ => LlmError::Http { status, message },
    })
}

fn error_text(err: &Value) -> String {
    match err {
        Value::String(s) => s.clone(),
        other => other
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| other.to_string()),
    }
}

/// The human message in an error body, whatever the format.
pub fn error_message(body: &str) -> String {
    match serde_json::from_str::<Value>(body) {
        Ok(v) => match v.get("error") {
            Some(err) => error_text(err),
            None => v
                .get("message")
                .and_then(Value::as_str)
                .map_or_else(|| truncate(body), str::to_owned),
        },
        Err(_) => truncate(body),
    }
}

fn truncate(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 300 {
        format!("{}…", s.chars().take(300).collect::<String>())
    } else {
        s.to_owned()
    }
}

/// Turns streamed SSE events into [`ChatEvent`]s.
#[derive(Debug)]
pub struct Decoder {
    kind: ProviderKind,
    /// Tool calls being assembled: index → (id, name, arguments).
    tools: BTreeMap<u64, (String, String, String)>,
    stop: Option<StopReason>,
    usage: Usage,
    done: bool,
    calls: usize,
}

impl Decoder {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            tools: BTreeMap::new(),
            stop: None,
            usage: Usage::default(),
            done: false,
            calls: 0,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn event(&mut self, ev: &SseEvent) -> Result<Vec<ChatEvent>, LlmError> {
        if self.done {
            return Ok(Vec::new());
        }
        if ev.data.trim() == "[DONE]" {
            return Ok(self.finish());
        }
        let v = parse(&ev.data)?;
        if let Some(err) = embedded_error(&v) {
            return Err(err);
        }
        match self.kind {
            ProviderKind::Anthropic => Ok(self.anthropic(ev.event.as_deref(), &v)),
            ProviderKind::Gemini => Ok(self.gemini(&v)),
            ProviderKind::OpenAiCompatible => Ok(self.openai(&v)),
        }
    }

    /// The stream ended: flush tool calls, usage and the stop reason (once).
    pub fn finish(&mut self) -> Vec<ChatEvent> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        let mut out = Vec::new();
        for (_, (id, name, args)) in std::mem::take(&mut self.tools) {
            out.push(ChatEvent::ToolCall {
                id,
                name,
                args_json: if args.trim().is_empty() {
                    "{}".into()
                } else {
                    args
                },
            });
        }
        if self.usage != Usage::default() {
            out.push(ChatEvent::Usage(self.usage));
        }
        let stop = self.stop.take().unwrap_or(if self.calls > 0 {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        });
        out.push(ChatEvent::Done(stop));
        out
    }

    fn anthropic(&mut self, event: Option<&str>, v: &Value) -> Vec<ChatEvent> {
        let kind = event
            .or_else(|| v.get("type").and_then(Value::as_str))
            .unwrap_or("");
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                if let Some(n) = v
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64)
                {
                    self.usage.input_tokens = Some(n);
                }
            }
            "content_block_start" => {
                let block = &v["content_block"];
                if block["type"] == "tool_use" {
                    let index = v["index"].as_u64().unwrap_or(0);
                    self.calls += 1;
                    self.tools.insert(
                        index,
                        (
                            block["id"].as_str().unwrap_or_default().to_owned(),
                            block["name"].as_str().unwrap_or_default().to_owned(),
                            String::new(),
                        ),
                    );
                }
            }
            "content_block_delta" => {
                let delta = &v["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        if let Some(t) = delta["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(ChatEvent::TextDelta(t.to_owned()));
                        }
                    }
                    Some("input_json_delta") => {
                        let index = v["index"].as_u64().unwrap_or(0);
                        if let Some(entry) = self.tools.get_mut(&index) {
                            entry
                                .2
                                .push_str(delta["partial_json"].as_str().unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = v["index"].as_u64().unwrap_or(0);
                if let Some((id, name, args)) = self.tools.remove(&index) {
                    out.push(ChatEvent::ToolCall {
                        id,
                        name,
                        args_json: if args.trim().is_empty() {
                            "{}".into()
                        } else {
                            args
                        },
                    });
                }
            }
            "message_delta" => {
                if let Some(r) = v.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(stop_reason(r));
                }
                if let Some(n) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    self.usage.output_tokens = Some(n);
                }
            }
            "message_stop" => out.extend(self.finish()),
            _ => {}
        }
        out
    }

    fn gemini(&mut self, v: &Value) -> Vec<ChatEvent> {
        let mut out = Vec::new();
        if let Some(parts) = v
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
        {
            for part in parts {
                if part.get("thought").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                if let Some(t) = part
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                {
                    out.push(ChatEvent::TextDelta(t.to_owned()));
                }
                if let Some(call) = part.get("functionCall") {
                    self.calls += 1;
                    out.push(ChatEvent::ToolCall {
                        id: format!("call_{}", self.calls),
                        name: call["name"].as_str().unwrap_or_default().to_owned(),
                        args_json: call.get("args").cloned().unwrap_or(json!({})).to_string(),
                    });
                }
            }
        }
        if let Some(r) = v
            .pointer("/candidates/0/finishReason")
            .and_then(Value::as_str)
        {
            self.stop = Some(if self.calls > 0 {
                StopReason::ToolUse
            } else {
                stop_reason(r)
            });
        }
        if let Some(meta) = v.get("usageMetadata") {
            self.usage.input_tokens = meta["promptTokenCount"]
                .as_u64()
                .or(self.usage.input_tokens);
            self.usage.output_tokens = meta["candidatesTokenCount"]
                .as_u64()
                .or(self.usage.output_tokens);
        }
        out
    }

    fn openai(&mut self, v: &Value) -> Vec<ChatEvent> {
        let mut out = Vec::new();
        if let Some(choice) = v.pointer("/choices/0") {
            let delta = choice.get("delta").or_else(|| choice.get("message"));
            if let Some(delta) = delta {
                if let Some(t) = delta
                    .get("content")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                {
                    out.push(ChatEvent::TextDelta(t.to_owned()));
                }
                for call in delta
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let index = call["index"].as_u64().unwrap_or(self.tools.len() as u64);
                    let entry = self.tools.entry(index).or_insert_with(|| {
                        self.calls += 1;
                        (String::new(), String::new(), String::new())
                    });
                    if let Some(id) = call["id"].as_str() {
                        entry.0 = id.to_owned();
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        entry.1.push_str(name);
                    }
                    if let Some(args) = call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        entry.2.push_str(args);
                    }
                }
            }
            if let Some(r) = choice.get("finish_reason").and_then(Value::as_str) {
                self.stop = Some(stop_reason(r));
            }
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            self.usage.input_tokens = u["prompt_tokens"].as_u64().or(self.usage.input_tokens);
            self.usage.output_tokens = u["completion_tokens"].as_u64().or(self.usage.output_tokens);
        }
        out
    }
}

/// Decode a whole (non-streamed) response body.
pub fn decode_full(kind: ProviderKind, body: &str) -> Result<Vec<ChatEvent>, LlmError> {
    let v = parse(body)?;
    if let Some(err) = embedded_error(&v) {
        return Err(err);
    }
    let mut d = Decoder::new(kind);
    let mut out = match kind {
        ProviderKind::Anthropic => {
            let mut out = Vec::new();
            for block in v["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("text") => out.push(ChatEvent::TextDelta(
                        block["text"].as_str().unwrap_or_default().to_owned(),
                    )),
                    Some("tool_use") => {
                        d.calls += 1;
                        out.push(ChatEvent::ToolCall {
                            id: block["id"].as_str().unwrap_or_default().to_owned(),
                            name: block["name"].as_str().unwrap_or_default().to_owned(),
                            args_json: block["input"].to_string(),
                        });
                    }
                    _ => {}
                }
            }
            if let Some(r) = v["stop_reason"].as_str() {
                d.stop = Some(stop_reason(r));
            }
            d.usage.input_tokens = v.pointer("/usage/input_tokens").and_then(Value::as_u64);
            d.usage.output_tokens = v.pointer("/usage/output_tokens").and_then(Value::as_u64);
            out
        }
        ProviderKind::Gemini => d.gemini(&v),
        ProviderKind::OpenAiCompatible => d.openai(&v),
    };
    out.extend(d.finish());
    Ok(out)
}

// ---- model lists -------------------------------------------------------------------------------

/// GET request listing the provider's models.
pub fn models_request(profile: &Profile, key: Option<&str>) -> (String, Vec<(String, String)>) {
    let base = profile.base();
    let url = match profile.kind {
        ProviderKind::Anthropic => format!("{base}/v1/models?limit=100"),
        ProviderKind::Gemini => format!("{base}/v1beta/models?pageSize=200"),
        ProviderKind::OpenAiCompatible => format!("{base}/models"),
    };
    (url, auth_headers(profile, key))
}

/// Model ids from a model-list body, sorted.
pub fn parse_models(kind: ProviderKind, body: &str) -> Result<Vec<String>, LlmError> {
    let v = parse(body)?;
    if let Some(err) = embedded_error(&v) {
        return Err(err);
    }
    let mut ids: Vec<String> = match kind {
        ProviderKind::Gemini => v["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| {
                m["supportedGenerationMethods"]
                    .as_array()
                    .is_none_or(|ms| ms.iter().any(|x| x == "generateContent"))
            })
            .filter_map(|m| m["name"].as_str())
            .map(|n| gemini_model(n).to_owned())
            .collect(),
        _ => v["data"]
            .as_array()
            .or_else(|| v["models"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
            .map(str::to_owned)
            .collect(),
    };
    ids.sort();
    ids.dedup();
    Ok(ids)
}
