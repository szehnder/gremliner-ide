# 0018. Own thin LLM layer with three wire formats

- Status: accepted
- Date: 2026-10-11

## Context
Users must be able to choose Anthropic, Google Gemini, or anything OpenAI-compatible (OpenAI, Ollama, LM Studio,
vLLM, OpenRouter, Groq…). We need streaming, structured output, tool calling where available, and model listing.

## Decision
`llm` defines `trait LlmProvider { async fn complete(req) -> Stream<Event>; async fn list_models(); fn caps() }`
with three implementations over `reqwest` + SSE:
- **Anthropic Messages** (`/v1/messages`, `x-api-key`, `anthropic-version`), tools, streaming.
- **Gemini** (`generateContent` / `streamGenerateContent`), `responseSchema` for JSON, function calling.
- **OpenAI Chat Completions** with configurable base URL (presets: OpenAI, Ollama `http://localhost:11434/v1`,
  LM Studio `http://localhost:1234/v1`, OpenRouter), `response_format` JSON schema when supported, tools.

A normalised request/response model (messages, tool specs, JSON-schema output) sits on top. Capabilities are
probed/configured per model (tools, json_schema, streaming).

## Alternatives considered
- **genai / rig crates** — broad but add churn and abstraction we would fight for structured-output control.
- **OpenAI-compatible only** — Anthropic and Gemini offer compat endpoints but lose features (caching, schemas).

## Consequences
- ~1.5k lines we own; recorded fixtures test each format offline.

## Revisit when
A fourth wire format is requested (Bedrock, Vertex) — add another impl.
