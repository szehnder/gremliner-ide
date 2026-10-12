# Tasks

## 1. Model

- [x] 1.1 Stylesheet model, JSON schema and `stylesheets` table; verify `scenario_graph_stylesheets_round_trip`
- [x] 1.2 Deterministic auto colours; verify `scenario_graph_stylesheets_stable_auto_colour`
- [x] 1.3 Caption templates and truncation; verify `scenario_graph_stylesheets_caption_template` and `scenario_graph_stylesheets_truncate_captions`
- [x] 1.4 Property-bound node size and edge width; verify `scenario_graph_stylesheets_size_by_property` and `scenario_graph_stylesheets_edge_width_by_distance`

## 2. Rendering and UI

- [x] 2.1 Canvas support for shapes, icons, captions and dashed edges with LOD; verify screenshot (headless renders exercise every path; the visual check is deferred to a manual run)
- [x] 2.2 Stylesheet editor panel with live preview; verify headless UI tests for editing a label style
- [x] 2.3 Named stylesheets, switching and per-connection default; verify `scenario_graph_stylesheets_switch_stylesheet` and `scenario_graph_stylesheets_connection_default`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green
- [x] 3.2 `openspec validate graph-stylesheets --strict` passes
