# Tasks

## 1. Storage

- [ ] 1.1 History table with FTS5 and retention; verify `scenario_query_history_entry_recorded`, `scenario_query_history_search_speed`, `scenario_query_history_retention_limit`
- [ ] 1.2 Folders and saved queries tables; verify unit tests for CRUD and nesting

## 2. UI

- [ ] 2.1 History sidebar section and palette with search and filters; verify `scenario_query_history_search_history` and `scenario_query_history_re_run_from_history`
- [ ] 2.2 Save tab with settings and reopen; verify `scenario_saved_queries_save_and_reopen`
- [ ] 2.3 Folder tree with drag and drop; verify `scenario_saved_queries_move_into_folder`
- [ ] 2.4 Dirty indicator and update on save; verify `scenario_saved_queries_dirty_indicator`

## 3. Verification

- [ ] 3.1 `cargo xtask ci` is green
- [ ] 3.2 `openspec validate query-history-saved --strict` passes
