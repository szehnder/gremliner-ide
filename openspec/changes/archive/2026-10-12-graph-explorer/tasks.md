# Tasks

## 1. Model and generation

- [x] 1.1 Explorer model and query generation with bindings; verify `scenario_graph_explorer_build_a_two_hop_path` and insta snapshots
- [x] 1.2 Operator table by type; verify `scenario_graph_explorer_numeric_filter_operators` and `scenario_graph_explorer_text_filter`

## 2. UI

- [x] 2.1 Builder UI with validation; verify `scenario_graph_explorer_path_must_end_on_a_vertex`
- [x] 2.2 Options and running into the graph view; verify `scenario_graph_explorer_limit_applied`
- [x] 2.3 Live code preview and open-in-editor; verify `scenario_graph_explorer_open_in_editor`
- [x] 2.4 "Search path from here" integration; verify `scenario_graph_explorer_start_from_a_node`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green with testenv running
- [x] 3.2 `openspec validate graph-explorer --strict` passes
