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
