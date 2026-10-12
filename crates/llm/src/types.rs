//! The normalised model every wire format maps to (ADR-0018): profiles, requests, streamed events and
//! errors.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    Gemini,
    /// OpenAI Chat Completions at any base URL (OpenAI, Ollama, LM Studio, OpenRouter, vLLM, …).
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::Gemini => "Google Gemini",
            ProviderKind::OpenAiCompatible => "OpenAI-compatible",
        }
    }
}

/// What a profile can do. Defaults come from the provider and model; each can be overridden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub streaming: bool,
    pub tools: bool,
    /// Structured output constrained by a JSON schema.
    pub json_schema: bool,
    /// Plain JSON mode (any JSON object), when there is no schema support.
    pub json_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CapabilityOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<bool>,
}

fn default_temperature() -> f32 {
    0.2
}

fn default_max_tokens() -> u32 {
    2048
}

fn default_timeout_ms() -> u64 {
    60_000
}

/// A saved LLM configuration. The API key lives in the credential store under [`Profile::key_account`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub kind: ProviderKind,
    /// Without a trailing slash: `https://api.anthropic.com`, `http://localhost:11434/v1`.
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// An API key is stored for this profile.
    #[serde(default)]
    pub has_key: bool,
    /// The preset it was created from, for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default)]
    pub overrides: CapabilityOverrides,
}

impl Profile {
    pub fn new(name: impl Into<String>, kind: ProviderKind, base_url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind,
            base_url: base_url.into(),
            model: String::new(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            timeout_ms: default_timeout_ms(),
            has_key: false,
            preset: None,
            overrides: CapabilityOverrides::default(),
        }
    }

    /// The credential-store account of this profile's API key.
    pub fn key_account(&self) -> String {
        format!("llm/{}/api-key", self.id)
    }

    /// The base URL without trailing slashes.
    pub fn base(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }

    /// Served from this machine (Ollama, LM Studio, …): data never leaves it.
    pub fn is_local(&self) -> bool {
        let host = self
            .base()
            .split("://")
            .nth(1)
            .unwrap_or_default()
            .split(['/', ':'])
            .next()
            .unwrap_or_default();
        matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "0.0.0.0")
    }

    /// Whether requests need an API key.
    pub fn requires_key(&self) -> bool {
        match self.kind {
            ProviderKind::Anthropic | ProviderKind::Gemini => true,
            ProviderKind::OpenAiCompatible => !self.is_local(),
        }
    }

    /// Defaults for the provider and model, with this profile's overrides applied.
    pub fn capabilities(&self) -> Capabilities {
        let mut caps = crate::presets::default_capabilities(self);
        if let Some(v) = self.overrides.streaming {
            caps.streaming = v;
        }
        if let Some(v) = self.overrides.tools {
            caps.tools = v;
        }
        if let Some(v) = self.overrides.json_schema {
            caps.json_schema = v;
        }
        caps
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    /// A tool call the assistant made (kept in history so the reply can refer to it).
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    /// The result of running a tool, sent back in a user turn.
    ToolResult {
        id: String,
        name: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            parts: vec![Part::Text { text: text.into() }],
        }
    }

    /// The text parts joined.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// A function the model may call. `parameters` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Ask for output matching a JSON Schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSchema {
    pub name: String,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatRequest {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub output_schema: Option<OutputSchema>,
    /// Overrides the profile's temperature.
    pub temperature: Option<f32>,
    /// Overrides the profile's maximum output tokens.
    pub max_tokens: Option<u32>,
}

impl ChatRequest {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::user(text)],
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Other(String),
}

/// One streamed event.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    TextDelta(String),
    /// A complete tool call (arguments as JSON text).
    ToolCall {
        id: String,
        name: String,
        args_json: String,
    },
    Usage(Usage),
    Done(StopReason),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LlmError {
    #[error("authentication failed ({status}): {message}")]
    Auth { status: u16, message: String },
    #[error("rate limited: {message}")]
    RateLimited { message: String },
    #[error("the provider returned {status}: {message}")]
    Http { status: u16, message: String },
    #[error("could not reach the provider: {0}")]
    Network(String),
    #[error("the provider did not answer in time")]
    Timeout,
    #[error("unexpected response: {0}")]
    Protocol(String),
    #[error("no model is selected for this profile")]
    NoModel,
    #[error("an API key is required for this profile")]
    MissingKey,
}

impl LlmError {
    pub fn is_auth(&self) -> bool {
        matches!(self, LlmError::Auth { .. } | LlmError::MissingKey)
    }
}

/// A finished (non-streamed or collected) response.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatResponse {
    pub text: String,
    pub tool_calls: Vec<(String, String, String)>,
    pub usage: Usage,
    pub stop_reason: Option<StopReason>,
}

impl ChatResponse {
    pub fn push(&mut self, event: ChatEvent) {
        match event {
            ChatEvent::TextDelta(t) => self.text.push_str(&t),
            ChatEvent::ToolCall {
                id,
                name,
                args_json,
            } => self.tool_calls.push((id, name, args_json)),
            ChatEvent::Usage(u) => {
                self.usage.input_tokens = u.input_tokens.or(self.usage.input_tokens);
                self.usage.output_tokens = u.output_tokens.or(self.usage.output_tokens);
            }
            ChatEvent::Done(reason) => self.stop_reason = Some(reason),
        }
    }

    /// The structured output: the forced tool's arguments (Anthropic), else JSON found in the text.
    pub fn output_json(&self, schema_name: &str) -> Option<Value> {
        if let Some((_, _, args)) = self.tool_calls.iter().find(|(_, n, _)| n == schema_name) {
            return serde_json::from_str(args).ok();
        }
        crate::json::extract_json(&self.text)
    }
}
