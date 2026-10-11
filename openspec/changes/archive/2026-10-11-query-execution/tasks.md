# Tasks

## 1. Pipeline

- [x] 1.1 Implement `ensure_terminal` and the run pipeline with RunId; verify `scenario_query_execution_append_tolist` and `scenario_query_execution_keep_explicit_terminal`
- [x] 1.2 Stream batches into `ResultSet` and update the summary live; verify `scenario_query_execution_progressive_rendering` and `scenario_query_execution_summary_after_success`
- [x] 1.3 Cancel via Esc/toolbar; verify `scenario_query_execution_cancel_a_long_query`
- [x] 1.4 Result cap; verify `scenario_query_execution_truncated_result`
- [x] 1.5 Error view and editor range highlight; verify `scenario_query_execution_server_error_view`

- [x] 1.6 Enforce read-only connections with `providers::read_only::check` before submitting; show the explanation and a "Run anyway" button that requires confirmation and applies to that single execution; extend `scenario_connections_mutation_blocked` with the UI flow

## 2. Normalisation

- [x] 2.1 `ResultShape` classification and `TableModel`/`TreeModel` builders with insta tests; verify `scenario_query_execution_elements_open_in_table`

## 3. Views

- [x] 3.1 Table view: label groups, map/scalar tables; verify `scenario_results_table_group_by_label` and `scenario_results_table_valuemap_results`
- [x] 3.2 Table sort, filter, resize; verify `scenario_results_table_sort_by_column` and `scenario_results_table_quick_filter`
- [x] 3.3 Table copy/export; verify `scenario_results_table_export_csv`
- [x] 3.4 Table performance benchmark; verify `scenario_results_table_scroll_100k_rows`
- [x] 3.5 JSON tree view with lazy rendering and copy actions; verify `scenario_results_json_expand_a_path`, `scenario_results_json_huge_result_opens_quickly`, `scenario_results_json_copy_value`
- [x] 3.6 Console view with raw toggle and search; verify `scenario_results_console_console_formatting`, `scenario_results_console_raw_graphson`, `scenario_results_console_regex_search`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green with testenv running
- [x] 4.2 `openspec validate query-execution --strict` passes
- [x] 4.3 End-to-end screenshot: `cargo xtask screenshot -- --open-connection local --run "g.V().hasLabel('airport').limit(100)"` shows table results
