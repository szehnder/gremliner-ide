# Design

## Context

ADR-0019 defines the pipeline, ADR-0020 the execution safety, ADR-0021 the privacy model. Inputs exist from earlier
changes: schema (`schema-discovery`), validation (`gremlin-language` + `autocomplete` schema warnings), LLM profiles.

## Goals / Non-Goals

**Goals:**
- Works with small local models (no tools, imperfect JSON) and gets better with strong ones.
- Typical end-to-end latency < 6 s with a fast cloud model, with streaming progress.

**Non-Goals:**
- Multi-step autonomous exploration (later `assistant-autonomous-agent`).

## Decisions

- **Pipeline state machine** in `assistant::nl`: `Generating → Validating → DryRunning → (Repairing(n) → Validating …) → Done | Failed`,
  emitting `PipelineEvent`s that the card renders.
- **Prompt (pure function, snapshot-tested):** system prompt with Gremlin rules (prefer `has(label,key,value)`, always
  bound results with `limit` unless aggregating, no lambdas, use `elementMap()`/`project()` for tabular asks, use
  `path()` or return elements for graph asks), the dialect (TinkerPop version, Neptune restrictions), the schema in a
  compact text form (`airport(code:String, city:String, runways:Int32 …) ~3.5k`, triples `airport -route-> airport`),
  few-shot examples, the conversation, and the output contract `{query, explanation, intent, expected_shape}`.
- **Structured output:** JSON schema output when the profile supports it; else instruct JSON and extract the first
  balanced JSON object or a fenced ```gremlin block as fallback.
- **Validation:** `gremlin_syntax::analyze` + `schema::check(query, schema, capabilities) -> Vec<Issue>`
  (unknown label/key, direction mismatch via triples, unsupported step).
- **Dry run:** `inject_limit(stmt, n)` inserts `.limit(n)` before the terminal/aggregate boundary (not after `count()`),
  `evaluationTimeout` set; runs on the tab's connection. Empty result triggers one repair with "returned 0 rows" when `expected_shape != scalar`.
- **Repair message:** previous query + issues/server error (+ preview rows only if privacy allows).
- **Preview:** reuses `TableModel`; element results render a small `graph-canvas` instance (≤ 200 nodes).
- **Evaluation:** `assistant/eval/golden.toml` (prompt, connection alias, assertion: non-empty / expected labels / expected count range);
  `cargo xtask eval-assistant --profile <p>` prints a pass rate and per-prompt report.

## Risks / Trade-offs

- [Dry runs load production databases] → limit + timeout + read-only classification; per-connection toggle "Allow assistant dry runs" (default on, off for production).
- [Model leaks lambdas/Groovy] → validation flags lambdas as unsupported on Neptune; the repair prompt explains.

## Verification

- Snapshot tests of prompts for each privacy mode.
- Pipeline tests with a scripted fake LLM (deterministic responses) against testenv: success, schema repair, server-error repair, give-up, mutation.
- `cargo run -p gremliner-cli -- nl --conn Local "count airports per country, top 5"`.
- `cargo xtask eval-assistant --profile <reference>` ≥ 90% (manual gate, needs an API key); Ollama result recorded.
