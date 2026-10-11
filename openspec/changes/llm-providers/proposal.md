# Proposal

## Why

The assistant must work with whatever model the user prefers or is allowed to use: Anthropic, Google Gemini,
OpenAI, or local models through Ollama or LM Studio, configured in seconds and stored safely.

## What Changes

- `llm` crate with three wire formats (ADR-0018): Anthropic Messages, Gemini generateContent, OpenAI Chat Completions with configurable base URL.
- Provider profiles: name, kind, base URL, API key (keychain), model, temperature, max tokens, timeout, plus capability flags (streaming, tool calling, JSON schema output) auto-detected or overridden.
- Presets: Anthropic, Google Gemini, OpenAI, Ollama (`http://localhost:11434/v1`), LM Studio (`http://localhost:1234/v1`), OpenRouter, Custom OpenAI-compatible.
- Model list fetched from the provider (`/v1/models`, Gemini `models`, Ollama tags), with free-text override.
- "Test" action that streams a short completion and reports latency and tokens.
- Settings page "AI": profiles list, default profile, per-connection override, privacy toggles (ADR-0021 defaults).
- `gremliner-cli llm test --profile <name>`.

Out of scope: the NL→Gremlin pipeline and assistant UI (next two changes).

## Capabilities

### New Capabilities
- `llm-providers`: configuring and testing large language model providers used by the assistant.

### Modified Capabilities
- None.

## Impact

- Crates: `llm`, `storage` (llm_profiles), `ui-workspace` (AI settings page), `gremliner-cli`.
- Dependencies: reqwest (rustls), eventsource parsing (small, in-house).
- ADRs: 0018, 0021, 0022.
