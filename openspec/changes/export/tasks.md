# Tasks

## 1. Writers

- [ ] 1.1 SVG writer from the canvas scene; verify `scenario_export_svg_contents`
- [ ] 1.2 PNG via resvg/tiny-skia with scale and background options (add ADR-0031); verify `scenario_export_whole_graph_at_2x`
- [ ] 1.3 GraphML and GraphSON writers; verify `scenario_export_graphml_round_trip` against testenv
- [ ] 1.4 JSON Lines results export; verify `scenario_export_json_lines`

## 2. UI

- [ ] 2.1 Export menu and dialog in graph and results toolbars; verify headless UI test
- [ ] 2.2 Clipboard copy of image and selection JSON; verify `scenario_export_copy_selection_as_json`

## 3. Verification

- [ ] 3.1 `cargo xtask ci` is green
- [ ] 3.2 `openspec validate export --strict` passes
