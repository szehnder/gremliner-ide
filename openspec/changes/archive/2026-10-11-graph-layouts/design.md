# Design

## Context

ADR-0009 (worker + streamed frames) and ADR-0010 (FA2 default, in-house algorithms). The store is SoA (ADR-0008).

## Goals / Non-Goals

**Goals:**
- FA2 iteration on 50k nodes < 50 ms using Barnes-Hut and rayon.

**Non-Goals:**
- GPU layout (not portable under ADR-0006).

## Decisions

- **API:** `trait Layout { fn step(&mut self, g: &LayoutGraph, pos: &mut [Vec2], pinned: &BitSet) -> StepResult }`;
  one-shot layouts (grid, circular, concentric, hierarchical, radial) compute targets and the worker tweens to them over 320 ms (ADR-0023 motion token).
- **FA2:** Barnes-Hut quadtree rebuilt each iteration (arena-allocated), repulsion by degree+1 mass, linear attraction, strong-gravity option, adaptive global speed (swinging/traction per the FA2 paper), convergence when global swing < ε·n.
- **Sugiyama:** greedy cycle removal (Eades), longest-path layering with dummy nodes for long edges, barycentric crossing reduction (4 sweeps), Brandes–Köpf coordinate assignment.
- **Worker:** `std::thread` + rayon pool; commands over crossbeam channel (`SetGraph`, `Start(LayoutKind, Options)`, `Stop`, `Pin`, `AddNodes`); frames published through a `triple_buffer` latest-wins slot; the canvas requests animation frames while frames keep arriving.
- **Incremental:** on `AddNodes`, existing nodes get a 10× mass and a damping factor for the first 50 iterations.
- **Seeded RNG:** `rand_chacha::ChaCha8Rng` for initial positions.

## Risks / Trade-offs

- [Barnes-Hut quality on dense hubs (air-routes has hubs with 300+ routes)] → FA2 "dissuade hubs" option; default tuned on air-routes.

## Verification

- `cargo test -p graph-layout` (determinism, pinned, hierarchical layer property, radial rings).
- `cargo run -p gremliner-cli --release -- layout-bench --dataset air-routes --layout fa2` reports convergence time (< 2 s).
- criterion `fa2_iteration_50k`.

## As built

- **Crate:** `graph-layout` depends only on rayon (no crossbeam, triple-buffer or rand): `LayoutGraph` (edges
  plus CSR adjacency), the `Layout` trait, `one_shot` for the target-based layouts, a SplitMix RNG for
  seeded starts, and `dataset::parse_graphml` for benchmarks.
- **FA2:** as designed (degree+1 masses, linear attraction, swinging/traction speed, Barnes-Hut θ = 1.2 with
  an arena quadtree rebuilt per iteration, parallel repulsion, sequential sums for determinism). Tuning
  found on air-routes: plain gravity lets leaves and small components drift forever (their equilibrium
  distance grows with total mass), so the default is **strong gravity 0.1** with scaling 60. Convergence is
  **relative**: mean displacement < 1e-4 of the layout's extent over the last 8 iterations. Small graphs
  jitter around that threshold, so movement cools by 0.99 per iteration after 400 iterations.
- **Measured:** air-routes (3,749 / 57,645) converges in 561 iterations, ~0.5 s (budget 2 s);
  synthetic 10k/20k in ~1.5 s; `fa2_iteration_50k` 9.9 ms (budget 50 ms); 10k-node canvas frames while a
  layout streams and the user pans/zooms: p95 0.9 ms headless.
- **Sugiyama:** cycle removal reverses DFS back edges (instead of Eades' greedy ordering); longest-path
  layering; dummy nodes unless the total span exceeds 20× the node count (dense cyclic graphs such as
  air-routes would need millions; 33 s → 18 ms); 8 barycentric sweeps; coordinates by 12 alternating passes
  that pull nodes to their neighbours' mean under the minimum gap (a simplification of Brandes–Köpf).
  Components are laid out side by side.
- **Worker:** `std::thread` + `std::sync::mpsc` commands (`Run`, `Stop`, `Pin`, `Shutdown`) and a
  `Mutex<Option<Frame>>` latest-wins slot, polled by the canvas in paint. Frames are paced at ~60 Hz:
  12 iterations per frame for graphs under 2k nodes (they visibly settle in about a second), otherwise as
  many as fit in 12 ms. One-shot layouts are computed on the worker and tweened over 320 ms (ease-out).
  Stop takes effect on the next frame: the canvas discards frames immediately.
- **Incremental placement:** instead of 10× masses, existing nodes are held fixed while the force layout
  places only the new ones (merge's ring placement is the starting point), so earlier nodes never move.
  This is used for expand, accumulated runs and follow-ups that add nodes, whatever layout is selected.
- **UI:** toolbar "Layout: …" dropdown (seven layouts, spacing Compact/Normal/Wide, FA2 gravity
  Weak/Normal/Strong), Re-run/Stop button, palette commands. New results are laid out with the tab's layout
  and the camera tweens to fit when it finishes (unless the user moved it). Dragging a node during a
  layout pins it in the worker. Settings persist per tab in `ui_state` under `graph.layout.<tab id>`.
- **CLI:** `gremliner-cli layout-bench --dataset air-routes --layout fa2` (any layout id, any GraphML path).
