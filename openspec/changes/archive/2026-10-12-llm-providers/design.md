# Design

## Context

ADR-0018 (own layer, three wire formats), ADR-0021 (privacy defaults), ADR-0022 (keychain). See proposal.md.
Model IDs change frequently, so nothing is hard-coded except preset suggestions that the model list replaces.

## Goals / Non-Goals

**Goals:**
- One normalised request type used by the assistant: messages (system/user/assistant/tool), tool specs (JSON Schema), output schema, stream flag.

**Non-Goals:**
- Embeddings, images, audio.

## Decisions

- **Normalised types:** `ChatRequest { system, messages, tools, output_schema, temperature, max_tokens }`;
  `ChatEvent::{TextDelta, ToolCall{id,name,args_json}, Usage, Done{stop_reason}, Error}`.
- **Anthropic:** POST `{base}/v1/messages`, headers `x-api-key`, `anthropic-version: 2023-06-01`; SSE events
  `content_block_delta` (text/input_json), `message_delta` (usage/stop_reason); tools native; structured output via
  a forced tool (`tool_choice: {type: "tool"}`) carrying the output schema.
- **Gemini:** POST `{base}/v1beta/models/{model}:streamGenerateContent?alt=sse`, header `x-goog-api-key`;
  `generationConfig.responseMimeType=application/json` + `responseSchema`; function calling via `tools.functionDeclarations`.
- **OpenAI-compatible:** POST `{base}/chat/completions` with `stream: true`; `response_format: {type:"json_schema"}`
  when supported, else `{type:"json_object"}`, else prompt-only JSON with a tolerant extractor; tools via `tools`/`tool_calls` deltas.
- **SSE parser:** small in-house line parser over `reqwest` byte stream (handles `data:` multi-line, `[DONE]`).
- **Capability defaults:** table keyed by kind + model-name patterns; overridable per profile. `list_models`:
  Anthropic `/v1/models`, Gemini `/v1beta/models`, OpenAI-compatible `/models`.
- **Retries:** 429/5xx with exponential backoff (max 2), honouring `retry-after`.

## Risks / Trade-offs

- [Provider API drift] → recorded fixture tests per format; live tests opt-in (`GREMLINER_LIVE_LLM=1`, plus Ollama when running locally).

## Verification

- `cargo test -p llm` with recorded SSE fixtures for each format (text, tool call, JSON output, errors).
- Mock HTTP server tests for model listing and auth errors.
- `cargo run -p gremliner-cli -- llm test --profile ollama` (when Ollama is running).

## As built

- **`llm` crate.**
  - `types` holds the normalised model: `Profile`, `ChatRequest`, `Message`/`Part`, `ToolSpec`, `OutputSchema`,
    `ChatEvent`, `ChatResponse::output_json` and `LlmError`.
  - `sse` is the SSE line parser.
  - `wire` is pure request building and decoding for the three formats, plus model lists and error bodies.
  - `client` is the reqwest transport: retries, streaming via `futures::stream::unfold`, the non-streaming
    fallback, `list_models` and `test`.
  - `presets` holds the presets and the capability table.
  - `store` is `ProfileStore`.
  - `json` is a tolerant JSON extractor, used when a model has no JSON mode.
- **Structured output.**
  - Anthropic uses a forced tool named after the schema.
  - Gemini uses `responseMimeType`/`responseSchema`, with `additionalProperties`, `$schema`, `title`, `default` and
    `examples` stripped. Tools are not sent alongside a response schema.
  - OpenAI-compatible uses `json_schema` (not strict) when the capability is on, otherwise `json_object`.
  - OpenAI's own endpoint uses `max_completion_tokens`; other endpoints use `max_tokens`.
- **Capability defaults.**
  - Anthropic, Gemini and OpenAI: everything on.
  - OpenRouter: no JSON schema.
  - Local endpoints: tool calling only for known tool-capable model families; JSON schema for Ollama and LM Studio ports.
  - Custom endpoints: streaming and JSON mode only.
  - Overrides are three-state (`Option<bool>`), and the request builder drops tools when tool calling is off.
- **Keys.**
  - Keys are stored at `llm/<id>/api-key` in the shared credential store. The app now passes one store to both
    connections and LLM profiles.
  - Keys are registered with the log redactor.
  - Provider error messages are scrubbed of the key and of masked echoes such as OpenAI's `sk-…****1234`.
- **Settings.**
  - Profiles live in the `llm_profiles` table (migration 6).
  - The default profile, per-connection overrides and per-connection "share sample data" choices are JSON in
    `settings` (`llm.default_profile`, `llm.connection_profiles`, `llm.share_samples`).
  - With no explicit choice, sample data is shared only with local profiles (ADR-0021).
  - The first saved profile becomes the default.
  - `resolve(connection)` picks the override, else the default, else the only profile.
- **UI.**
  - The settings tab has an "AI" page that hosts `ai::page::AiSettingsView`:
    - The profile list, with "Add profile" from presets, edit, delete (confirmed) and make default.
    - A row per connection, with a profile picker and a share-samples switch.
  - The profile dialog (`ai::form::ProfileForm`):
    - "List models" offers clickable model chips; a listing error says the id can be typed.
    - Capability switches.
    - "Test" streams the reply into the dialog and shows the first-token and total times and the token usage.
- **CLI.** `gremliner-cli llm list|add|models|test|remove` (`add --preset … [--key-stdin] [--default]`).
- **Tests.**
  - `llm::testing::MockServer` (feature `testing`) is a scripted HTTP/1.1 server that writes SSE event by
    event; the CLI and UI tests use it too.
  - Recorded fixtures for each format are in `crates/llm/tests/fixtures`.
  - Live-model checks are manual: `gremliner-cli llm add --name Local --preset ollama --model <m>`, then
    `llm test --profile Local`.
