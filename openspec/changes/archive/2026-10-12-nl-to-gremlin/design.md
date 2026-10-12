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

## As built

- **`assistant` crate.**
  - `context` builds the prompt (dialect, rules, one few-shot example, the compact schema, and the editor
    query). Its insta snapshots cover a structure-only prompt and one with samples on Neptune.
  - `answer` parses the reply: schema JSON, a forced-tool reply, JSON in prose or fences, a fenced
    ```gremlin block, inline code, or a bare `g.` line.
  - `check` validates the query: syntax, a single traversal from `g`, lambdas, unknown or unsupported
    steps, and schema labels and keys. It uses the autocomplete warnings plus the new
    `gremlin_syntax::complete::direction_issues`.
  - `dryrun` holds `inject_limit` (no limit after a top-level aggregate; before a terminal step otherwise)
    and the `DryRunner` trait, implemented for the Gremlin client.
  - `pipeline::run` is the state machine. Write intent, or any mutation that `is_mutating` detects, is
    never run, even when the model says "read". The best attempt is kept by score. Only one repair is
    spent on "0 rows".
  - `testing` provides the scripted LLM and database (feature `testing`).
- **Privacy.** Sample values reach the prompt only when the connection shares them (local models by
  default). Repair messages never contain dry-run rows.
- **Tool calling** (`get_schema`, `dry_run`, and so on) is not used yet. The code-driven pipeline covers
  every model, and the tools arrive with the assistant panel or the agent change.
- **UI.**
  - `ui_assistant::PromptBar` is the bar and card. The host is `EditorPanel`: Ctrl/Cmd+K, the toolbar's
    "Ask" button, or the palette command "Assistant: Ask in Plain English".
  - The tab runs the pipeline on the I/O runtime over the tab's connection session and the resolved LLM
    profile, and streams events into the card.
  - The card shows the highlighted query (`ui_editor::highlight_styles`), badges, a write warning,
    remaining issues, and the preview: an offline `GraphView` for elements, a mini table otherwise.
    Its actions are Insert (a new line after the cursor's line), Run (inserts, then runs without the
    dry-run limit; writes ask first), Copy and Refine.
- **Lessons.**
  - Insert-then-run must defer the run, because the editor re-analyses on a deferred change event.
  - "Open AI settings" reaches the workspace through `Workspace::for_window` (the root view), deferred
    with `window.defer`. An action dispatched from that click may not reach the workspace.
  - A long message in a flex row pushed the button off-screen until the text got `min_w_0`.
- **CLI.**
  - `gremliner-cli nl --conn C [--profile P] [--json] [--no-dry-run] "…"` exits with 2 when the query
    is not verified.
  - `gremliner-cli eval --profile P [--min 0.9]` and `cargo xtask eval-assistant --profile P` run the
    golden set (`crates/assistant/eval/golden.toml`: 30 air-routes and 10 modern prompts).
  - `scenario_nl_to_gremlin_golden_set_evaluation` runs only with `GREMLINER_EVAL_PROFILE` set.
- **Not yet measured:** the ≥ 90% golden-set gate and the Ollama score. No reference model or local
  model server was available while this was built.
