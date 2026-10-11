# Design

## Context

ADR-0027 defines the budgets. Benchmarks exist per change (criterion + app-level). CI runners are noisy.

## Goals / Non-Goals

**Goals:**
- A single command reporting all budgets with pass/fail.

**Non-Goals:**
- Cross-machine comparability of absolute numbers in CI.

## Decisions

- **Measurements:** criterion JSON output (`target/criterion/**/estimates.json`); app-level via flags: `--measure-startup`
  (prints time from process start to first frame using GPUI's first-frame callback), `--measure-idle-memory`
  (reads `/proc/self/statm` on Linux, task_info on macOS, GetProcessMemoryInfo on Windows), render bench (existing).
- **Baseline:** `perf-baseline.json` committed from main by a CI job; PR runs compare relative.
- **Startup work:** construct only visible panels; schema/LLM/model-list work deferred to after first frame; fonts loaded on the background thread before first text layout if possible.

## Risks / Trade-offs

- [Flaky CI timing] → 3 runs, use the median; a 15% threshold.

## Verification

- `cargo xtask bench --check` locally passes all absolute budgets.
- Artificially slow down a bench in a scratch commit to see CI fail (then revert).
