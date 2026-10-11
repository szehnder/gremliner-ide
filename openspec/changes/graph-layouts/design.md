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
