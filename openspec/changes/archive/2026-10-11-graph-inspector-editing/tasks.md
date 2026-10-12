# Tasks

## 1. Inspector

- [x] 1.1 Inspector panel for vertices and edges with typed properties and endpoint navigation; verify `scenario_graph_editing_inspect_a_vertex` and `scenario_graph_editing_navigate_to_endpoint`

## 2. Editing

- [x] 2.1 `EditCommand` statement generation with bindings and inverses; verify insta snapshots
- [x] 2.2 Property edit with preview and local patch; verify `scenario_graph_editing_edit_a_property`
- [x] 2.3 Create vertex dialog and Alt-drag edge gesture; verify `scenario_graph_editing_draw_an_edge`
- [x] 2.4 Delete with counts; verify `scenario_graph_editing_delete_a_vertex`
- [x] 2.5 Undo/redo with preview; verify `scenario_graph_editing_undo_property_change`
- [x] 2.6 Production/read-only double confirmation; verify `scenario_graph_editing_production_confirmation`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green with testenv running
- [x] 3.2 `openspec validate graph-inspector-editing --strict` passes
