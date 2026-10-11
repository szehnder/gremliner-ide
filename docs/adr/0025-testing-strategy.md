# 0025. Testing strategy

- Status: accepted
- Date: 2026-10-11

## Context
We want agents to iterate safely with fast, trustworthy feedback, and to trace every spec scenario to a test.

## Decision
- **Unit tests** in each crate; pure crates aim for high coverage.
- **Snapshot tests** with `insta` for GraphSON decoding, formatter output, parse trees, prompt builders, profile parsing, layouts (seeded).
- **Integration tests** (`tests/` in `gremlin-client`, `providers`, `schema`, `assistant`, `debugger`) against the
  Docker test environment; skipped with a clear message when `GREMLINER_TESTENV` is not reachable.
- **UI tests** with `#[gpui_kit::test]` headless windows for interaction logic (dock, editor actions, tables).
- **LLM tests** use recorded fixtures by default; live tests opt-in via `GREMLINER_LIVE_LLM=1`.
- **Benchmarks** with criterion; budgets in `perf-budgets.toml` checked by `cargo xtask bench --check`.
- **Scenario traceability:** each OpenSpec `#### Scenario:` maps to a test named `scenario_<capability>_<slug>`;
  `cargo xtask spec-coverage` reports unmapped scenarios.

## Alternatives considered
- **End-to-end GUI automation** — reserved for the screenshot smoke test only.

## Consequences
- `cargo xtask ci` = fmt check, clippy -D warnings, layering check, token check, tests, `openspec validate --strict`.

## Revisit when
Test time exceeds 5 minutes locally.
