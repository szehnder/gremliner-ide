//! Provider presets and capability defaults. Model ids change often, so presets only suggest a model;
//! the profile editor replaces the suggestion with the provider's own model list.

use crate::types::{Capabilities, Profile, ProviderKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ProviderKind,
    pub base_url: &'static str,
    /// A starting model; the list fetched from the provider replaces it.
    pub suggested_model: &'static str,
    pub needs_key: bool,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com",
        suggested_model: "claude-sonnet-5-5",
        needs_key: true,
    },
    Preset {
        id: "gemini",
        name: "Google Gemini",
        kind: ProviderKind::Gemini,
        base_url: "https://generativelanguage.googleapis.com",
        suggested_model: "gemini-2.5-flash",
        needs_key: true,
    },
    Preset {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::OpenAiCompatible,
        base_url: "https://api.openai.com/v1",
        suggested_model: "gpt-4.1-mini",
        needs_key: true,
    },
    Preset {
        id: "ollama",
        name: "Ollama",
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://localhost:11434/v1",
        suggested_model: "qwen2.5-coder",
        needs_key: false,
    },
    Preset {
        id: "lmstudio",
        name: "LM Studio",
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://localhost:1234/v1",
        suggested_model: "",
        needs_key: false,
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::OpenAiCompatible,
        base_url: "https://openrouter.ai/api/v1",
        suggested_model: "",
        needs_key: true,
    },
    Preset {
        id: "custom",
        name: "Custom (OpenAI-compatible)",
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://localhost:8000/v1",
        suggested_model: "",
        needs_key: false,
    },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl Preset {
    /// A new profile from this preset.
    pub fn profile(&self, name: impl Into<String>) -> Profile {
        let mut p = Profile::new(name, self.kind, self.base_url);
        p.model = self.suggested_model.to_owned();
        p.preset = Some(self.id.to_owned());
        p
    }
}

/// Local model families that handle OpenAI-style tool calls reliably.
const LOCAL_TOOL_MODELS: &[&str] = &[
    "llama3.1",
    "llama3.2",
    "llama3.3",
    "llama4",
    "qwen2.5",
    "qwen3",
    "qwq",
    "mistral",
    "mixtral",
    "command-r",
    "hermes",
    "firefunction",
    "gpt-oss",
    "granite3",
    "granite4",
    "devstral",
    "phi4",
];

/// Capability defaults by provider, endpoint and model name.
pub fn default_capabilities(profile: &Profile) -> Capabilities {
    let model = profile.model.to_lowercase();
    let base = profile.base().to_lowercase();
    match profile.kind {
        ProviderKind::Anthropic | ProviderKind::Gemini => Capabilities {
            streaming: true,
            tools: true,
            json_schema: true,
            json_mode: true,
        },
        ProviderKind::OpenAiCompatible if base.contains("api.openai.com") => Capabilities {
            streaming: true,
            tools: true,
            json_schema: true,
            json_mode: true,
        },
        ProviderKind::OpenAiCompatible if base.contains("openrouter.ai") => Capabilities {
            streaming: true,
            tools: true,
            json_schema: false,
            json_mode: true,
        },
        ProviderKind::OpenAiCompatible if profile.is_local() => Capabilities {
            streaming: true,
            tools: LOCAL_TOOL_MODELS.iter().any(|m| model.contains(m)),
            // Ollama (0.5+) and LM Studio accept `response_format: json_schema`.
            json_schema: base.contains(":11434") || base.contains(":1234"),
            json_mode: true,
        },
        ProviderKind::OpenAiCompatible => Capabilities {
            streaming: true,
            tools: false,
            json_schema: false,
            json_mode: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_llm_providers_ollama_preset() {
        let p = preset("ollama").unwrap().profile("Local");
        assert_eq!(p.kind, ProviderKind::OpenAiCompatible);
        assert_eq!(p.base_url, "http://localhost:11434/v1");
        assert!(!p.requires_key());
        assert!(p.is_local());
        let cloud = preset("anthropic").unwrap().profile("Claude");
        assert!(cloud.requires_key() && !cloud.is_local());
    }

    #[test]
    fn local_tool_support_follows_the_model() {
        let mut p = preset("ollama").unwrap().profile("Local");
        p.model = "qwen2.5-coder:7b".into();
        assert!(p.capabilities().tools);
        p.model = "tinyllama".into();
        assert!(!p.capabilities().tools);
    }

    #[test]
    fn scenario_llm_providers_override_tool_calling() {
        let mut p = preset("ollama").unwrap().profile("Local");
        p.model = "llama3.1:8b".into();
        assert!(p.capabilities().tools);
        p.overrides.tools = Some(false);
        assert!(!p.capabilities().tools);
        // The request builder leaves tools out for such a profile.
        let req = crate::types::ChatRequest {
            tools: vec![crate::types::ToolSpec {
                name: "get_schema".into(),
                description: "d".into(),
                parameters: serde_json::json!({"type": "object"}),
            }],
            ..crate::types::ChatRequest::user("hi")
        };
        let built = crate::wire::build(&p, None, &req, true).unwrap();
        assert!(built.body.get("tools").is_none(), "{}", built.body);
    }
}
