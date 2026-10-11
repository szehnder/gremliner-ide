# Tasks

## 1. Query generation

- [ ] 1.1 Nested-context prefix generation and parent-kind table; verify insta tests and `scenario_query_debugger_step_into_a_nested_traversal` (generation level)
- [ ] 1.2 Count, simulation and inspection query generation; verify unit tests

## 2. Session

- [ ] 2.1 Session start rules and navigation; verify `scenario_query_debugger_start_debugging`, `scenario_query_debugger_mutating_query_refused`, `scenario_query_debugger_step_back`
- [ ] 2.2 Intermediate results with cancellation of stale requests; verify `scenario_query_debugger_results_at_a_step`
- [ ] 2.3 Inline counts; verify `scenario_query_debugger_counts_after_each_step`
- [ ] 2.4 Simulation with paging and hide-failed; verify `scenario_query_debugger_mark_failed_traversers`
- [ ] 2.5 Traverser inspection; verify `scenario_query_debugger_find_where_an_element_drops`
- [ ] 2.6 Modulator handling; verify `scenario_query_debugger_modulator_step`

## 3. UI

- [ ] 3.1 Debug mode in the editor (read-only, step highlight, keybindings, toolbar) and debug panel (simulation and inspection); verify headless UI tests

## 4. Verification

- [ ] 4.1 `cargo xtask ci` is green with testenv running
- [ ] 4.2 `openspec validate query-debugger --strict` passes
- [ ] 4.3 Screenshot review of a debug session
