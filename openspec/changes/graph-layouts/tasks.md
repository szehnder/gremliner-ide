# Tasks

## 1. Algorithms

- [ ] 1.1 Implement the `Layout` trait, seeded init and FA2 with Barnes-Hut; verify `scenario_graph_layouts_same_seed_same_result` and `scenario_graph_layouts_pinned_node_stays`
- [ ] 1.2 Implement Sugiyama hierarchical; verify `scenario_graph_layouts_hierarchical_layout_of_a_tree`
- [ ] 1.3 Implement radial, circular, grid, concentric; verify `scenario_graph_layouts_radial_around_selection` plus unit tests for the others

## 2. Worker and UI

- [ ] 2.1 Layout worker with streamed frames, stop and tweening; verify `scenario_graph_layouts_stop_layout` and `scenario_graph_layouts_interaction_during_layout`
- [ ] 2.2 Incremental placement on expand; verify `scenario_graph_layouts_expand_keeps_context`
- [ ] 2.3 Toolbar layout picker with options, persisted per tab; verify via headless UI test

## 3. Benchmarks

- [ ] 3.1 `gremliner-cli layout-bench` and criterion `fa2_iteration_50k`; verify `scenario_graph_layouts_air_routes_converges_quickly`

## 4. Verification

- [ ] 4.1 `cargo xtask ci` is green
- [ ] 4.2 `openspec validate graph-layouts --strict` passes
- [ ] 4.3 Screenshots of air-routes under FA2 and of a hierarchical tree
