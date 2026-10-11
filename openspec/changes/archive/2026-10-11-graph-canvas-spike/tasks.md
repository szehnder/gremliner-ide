# Tasks

## 1. Store and math

- [x] 1.1 Implement the minimal SoA `graph-store` (add nodes/edges, positions, revision counter) with a seeded synthetic generator; verify unit tests for id interning and revision bumps
- [x] 1.2 Implement camera math (world↔screen, zoom about point, fit-to-view); verify `scenario_graph_rendering_zoom_keeps_point_under_cursor` and `scenario_graph_rendering_fit_to_view` as pure-function tests
- [x] 1.3 Implement the culling grid and hit-testing; verify `scenario_graph_rendering_click_selects_the_topmost_node` and `scenario_graph_rendering_rectangle_selection`

## 2. Canvas element

- [x] 2.1 Implement the `GraphCanvas` element painting node quads and batched edge paths through the `GraphRenderer` trait; verify a screenshot shows a 1k-node graph
- [x] 2.2 Add LOD tiers and the label cache; verify `scenario_graph_rendering_labels_hide_when_zoomed_out`
- [x] 2.3 Add pan/zoom/inertia input handling and node dragging with pinning; verify `scenario_graph_rendering_drag_pins_a_node` (headless input simulation)
- [x] 2.4 Add the minimap; verify `scenario_graph_rendering_navigate_via_minimap`

## 3. Benchmark gate

- [x] 3.1 Implement `--render-bench N,E` (scripted camera path, frame stats) and `gremliner-cli render-bench` (CPU scene build); verify both print p50/p95/p99/max
- [x] 3.2 Run the benchmarks on the reference machine; verify `scenario_graph_rendering_medium_graph_benchmark` and `scenario_graph_rendering_large_graph_benchmark` thresholds are met, or apply the mitigations in design.md in order
- [x] 3.3 Write `docs/benchmarks/graph-canvas-spike.md` with hardware, numbers and decision; set ADR-0006 to accepted, or write a superseding ADR that activates ADR-0007

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate graph-canvas-spike --strict` passes
