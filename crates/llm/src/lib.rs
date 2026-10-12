//! LLM provider layer (ADR-0018): Anthropic Messages, Gemini and OpenAI-compatible Chat Completions
//! behind one normalised request/event model, with streaming, tools, structured output, model lists,
//! presets, capability defaults and the saved-profile store.

pub mod client;
pub mod json;
pub mod presets;
pub mod sse;
pub mod store;
pub mod types;
pub mod wire;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use client::{EventStream, LlmClient, TestReport, scrub};
pub use presets::{PRESETS, Preset, preset};
pub use store::{KeyChange, ProfileStore};
pub use types::*;
