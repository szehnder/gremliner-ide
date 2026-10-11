# Tasks

## 1. Editor core

- [x] 1.1 Spike the gpui-kit code editor with the custom Gremlin language registered; verify `scenario_query_editor_highlight_a_query` and record API notes in `docs/gpui-notes.md`
- [x] 1.2 Wire the incremental `Document` and publish diagnostics plus the problems panel; verify `scenario_query_editor_show_an_error` and `scenario_query_editor_jump_to_problem`
- [x] 1.3 Hover provider from the step catalogue; verify `scenario_query_editor_hover_a_step`
- [x] 1.4 Format and toggle comment actions; verify `scenario_query_editor_format_document`

## 2. Execution entry

- [x] 2.1 Current-statement highlight and `RunRequested` for cursor or selection; verify `scenario_query_editor_run_second_statement` and `scenario_query_editor_run_selection`

## 3. Tabs

- [x] 3.1 Connection picker per tab and default to last used; verify `scenario_query_editor_new_tab_defaults`
- [x] 3.2 Reorder, split, dirty marker; verify `scenario_query_editor_split_editor`
- [x] 3.3 Persist and restore tabs; verify `scenario_query_editor_restore_tabs`
- [x] 3.4 Show the bound connection's colour and environment badge on each tab (production in the danger colour, `connections::style::environment_badge`); extend `scenario_connections_production_badge` to assert the tab badge

## 4. Performance

- [x] 4.1 Large-document typing benchmark; verify `scenario_query_editor_large_file_typing`

## 5. Verification

- [x] 5.1 `cargo xtask ci` is green
- [x] 5.2 `openspec validate query-editor --strict` passes
- [x] 5.3 Screenshot review of the editor in both themes
