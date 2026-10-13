# Proposal

## Why

Performance is a headline requirement. Individual changes have benchmarks; this change makes the budgets
(ADR-0027) a permanent, enforced contract so regressions are caught automatically.

## What Changes

- `cargo xtask bench --check` runs all criterion benches and app-level measurements (cold start, idle memory, render bench, table scroll) and compares them with `perf-budgets.toml` (absolute, local) and a stored baseline (relative, CI).
- Startup optimisation pass to meet cold start < 300 ms (lazy panel construction, deferred schema/LLM init, font loading).
- Idle memory pass to meet < 150 MB RSS.
- CI job that fails on > 15% regression versus the main baseline; `spec-coverage` becomes enforcing for archived specs.

Out of scope: new features.

## Capabilities

### New Capabilities
- `performance-budgets`: measurable performance guarantees for startup, memory, input latency, rendering and data handling, enforced continuously.

### Modified Capabilities
- None.

## Impact

- `xtask`, CI workflow, startup code in `gremliner` and `ui-workspace`.
- ADRs: 0027.
