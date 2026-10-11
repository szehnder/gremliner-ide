# Tasks

## 1. Measurement

- [ ] 1.1 `--measure-startup` and `--measure-idle-memory` flags; verify `scenario_performance_budgets_cold_start_measurement` and `scenario_performance_budgets_idle_memory_measurement`
- [ ] 1.2 `cargo xtask bench --check` aggregating criterion and app measurements against `perf-budgets.toml` and baseline; verify `scenario_performance_budgets_regression_detected`

## 2. Optimisation

- [ ] 2.1 Startup optimisation pass until under 300 ms; verify the cold-start measurement
- [ ] 2.2 Idle memory pass until under 150 MB; verify the idle-memory measurement

## 3. CI

- [ ] 3.1 Baseline job on main and relative check on PRs; verify the workflow
- [ ] 3.2 Make `spec-coverage` enforcing for archived specs; verify `scenario_performance_budgets_missing_scenario_test`

## 4. Verification

- [ ] 4.1 `cargo xtask ci` is green
- [ ] 4.2 `openspec validate performance-budgets --strict` passes
