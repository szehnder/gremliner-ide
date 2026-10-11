# Proposal

## Why

The graph view is the riskiest part of the GPUI choice: upstream GPUI has no custom-shader hook (ADR-0006). We must
prove we can hit the frame-rate targets with GPUI primitives **before** building features on top, or switch to the
ADR-0007 fallback while it is still cheap.

## What Changes

- `graph-store` minimal SoA store (positions, edges, styles) and a synthetic loader.
- `graph-canvas` element: camera (pan, zoom to cursor, inertia), batched node quads, batched edge paths with
  arrowheads, LOD tiers, grid culling, label cache, hover/selection hit-testing, minimap.
- A `GraphRenderer` trait boundary so the fallback can be swapped in.
- `gremliner-cli render-bench` and an in-app "Render Bench" window (`gremliner --render-bench N,E`) that runs a
  scripted pan/zoom path and prints frame statistics.
- A gate report `docs/benchmarks/graph-canvas-spike.md` with results; ADR-0006 status updated to accepted, or ADR-0007 activated through a superseding ADR.

Out of scope: layouts beyond a cheap initial layout (random + a few FA2 iterations), styling rules, data from Gremlin.

## Capabilities

### New Capabilities
- `graph-rendering`: interactive, high-performance drawing of node-link graphs (camera, LOD, hit-testing, frame budgets).

### Modified Capabilities
- None.

## Impact

- Crates: `graph-store`, `graph-canvas`, `gremliner-cli`, `gremliner`.
- ADRs: 0006, 0007, 0008, 0027.
