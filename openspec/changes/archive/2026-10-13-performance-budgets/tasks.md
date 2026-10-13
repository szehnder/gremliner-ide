# Tasks

## 1. Measurement

- [x] 1.1 `--measure-startup` and `--measure-idle-memory` flags; verify `scenario_performance_budgets_cold_start_measurement` and `scenario_performance_budgets_idle_memory_measurement`
- [x] 1.2 `cargo xtask bench --check` aggregating criterion and app measurements against `perf-budgets.toml` and baseline; verify `scenario_performance_budgets_regression_detected`

## 2. Optimisation

- [x] 2.1 Startup optimisation pass until under 300 ms; verify the cold-start measurement
- [x] 2.2 Idle memory pass until under 150 MB; verify the idle-memory measurement

- [x] 2.3 Cold start is ~2 s to first frame on the dev laptop: ~1 s enumerating every wgpu adapter (NVIDIA Vulkan + GL) and ~1 s before the first frame. Investigate restricting backends / adapter probing and pipeline caching (measured 2026-10-11, `RUST_LOG=gpui_wgpu=info`). Done: ADR-0039 hides the unused NVIDIA drivers on hybrid laptops (first frame ~200 ms, idle ~110 MB); GL enumeration stays (fallback)

## 3. CI

- [x] 3.1 Baseline job on main and relative check on PRs; verify the workflow
- [x] 3.2 Make `spec-coverage` enforcing for archived specs; verify `scenario_performance_budgets_missing_scenario_test`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate performance-budgets --strict` passes
