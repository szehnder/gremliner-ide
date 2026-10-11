# Design

## Context

Builds on the nl-to-gremlin pipeline (verification, privacy, prompt builders) and llm tool calling. See proposal.md; ADR-0019/0020/0021.

## Goals / Non-Goals

**Goals:**
- A single `assistant::Session` per tab with modes sharing context builders.

**Non-Goals:**
- Persisting conversations across restarts (v1 keeps them for the session; history may come later).

## Decisions

- **Modes:** `Mode::{Chat, Explain, Fix, Optimize, Describe}` each with a prompt template (snapshot-tested) and a context recipe.
- **Explain step mapping:** the model is asked to return JSON items `{steps: [step_index…], text}` using step indices we provide
  (we send the statement annotated with `[#n]` markers per step from `gremlin-syntax`); the UI maps indices back to spans.
- **Code-block verification:** fenced ```gremlin blocks in streamed Markdown are detected when closed and run through
  validation (+ dry run if read-only and allowed) asynchronously, adding badges in place.
- **Tools:** JSON-schema tool specs; executor enforces `is_mutating == false` and dry-run limits; tool loop capped at 6 calls per answer.
- **Optimize equivalence:** run `original.limit(K).count()` vs `candidate.limit(K).count()` for K=1000 (not proof; labelled "equivalent on sample").
- **Markdown:** gpui-kit Markdown renderer with a custom code-block component for actions.

## Risks / Trade-offs

- [Step-index JSON from weak models] → fallback to plain explanation without hover mapping.

## Verification

- Fake-LLM tests for each mode and tool behaviour (including mutation refusal).
- Headless UI tests for per-tab conversations, chips and code-block actions.
