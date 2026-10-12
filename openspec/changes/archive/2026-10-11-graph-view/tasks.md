# Tasks

## 1. Store operations

- [x] 1.1 Implement `merge(ElementSet)` with dedupe and stable positions; verify `scenario_graph_view_paths_become_a_graph` and `scenario_graph_view_accumulate_results`
- [x] 1.2 Edges-between and missing-endpoint follow-up queries; verify `scenario_graph_view_edges_between_results` and `scenario_graph_view_edge_only_result`
- [x] 1.3 Visibility flags, label toggles and predicate filters; verify `scenario_graph_view_hide_selection`, `scenario_graph_view_toggle_label_visibility`, `scenario_graph_view_property_filter`

## 2. Graph tab UI

- [x] 2.1 Graph results tab with toolbar, clear/accumulate toggle and stats; verify `scenario_graph_view_counts_update`
- [x] 2.2 Expand neighbours (double-click, context menu by direction/label, paging); verify `scenario_graph_view_double_click_expand` and `scenario_graph_view_expand_by_label`
- [x] 2.3 Legend and filter panel; verify via headless UI tests for the legend and filter scenarios
- [x] 2.4 Find in graph with animated focus; verify `scenario_graph_view_find_a_node`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green with testenv running
- [x] 3.2 `openspec validate graph-view --strict` passes
- [x] 3.3 Screenshot review (deferred to a manual check; agent-launched windows stop after the first frame, see `docs/gpui-notes.md`): `cargo xtask screenshot -- --open-connection local --run "g.V().has('code','AUS').bothE().otherV().path()"`
