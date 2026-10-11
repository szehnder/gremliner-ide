# 0019. Code-driven, schema-grounded NL→Gremlin pipeline

- Status: accepted
- Date: 2026-10-11

## Context
A natural-language assistant that is not grounded in the connected schema produces plausible but wrong queries
(wrong labels, wrong edge direction). Local models (Ollama/LM Studio) often lack reliable tool calling.

## Decision
Our code orchestrates a fixed pipeline, independent of model tool support:
1. Build context (schema summary, dialect & unsupported steps, current query, recent result shape, conversation).
2. Generate structured `{query, explanation, intent, expected_shape}`.
3. Static validation: tree-sitter syntax + schema check (labels, property keys, edge direction per label pair,
   unsupported steps) + mutation detection.
4. Read-only dry run with injected limit and timeout (ADR-0020); capture count, sample, timing.
5. Repair loop (≤3 rounds) feeding diagnostics / empty result / server error back to the model.
6. Present a preview card with verification badges; user chooses Insert / Run / Refine.

Models with tool calling additionally get read-only tools (`get_schema`, `sample_values`, `validate`, `dry_run`).
A fully autonomous agent is a separate future change (`assistant-autonomous-agent`).

## Alternatives considered
- **Single-shot prompt (Compass-style)** — fast but unverified.
- **Agent loop only** — fails on models without tool calling; less predictable latency.

## Consequences
- Latency is higher than single-shot (one dry run, sometimes repairs); mitigated by streaming status in the card.
- Golden-prompt eval set on air-routes measures quality (`cargo xtask eval-assistant`).

## Revisit when
Local models universally support reliable tool calling.
