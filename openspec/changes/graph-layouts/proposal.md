# Proposal

## Why

A layout makes a graph readable or useless. G.V() users expect a good organic default plus hierarchical and other
layouts, applied smoothly and without freezing the UI, even on air-routes-sized graphs.

## What Changes

- `graph-layout` crate: ForceAtlas2 with Barnes-Hut (default), hierarchical (Sugiyama, top-down/left-right), radial
  (around selection), circular, grid, concentric (by degree).
- A layout worker running off the main thread that streams animated position frames (ADR-0009), honours pinned nodes, and places new nodes incrementally.
- Layout picker in the graph toolbar with per-layout options (e.g. FA2 gravity/scaling, hierarchical direction/spacing); "Re-run layout" and "Stop layout" actions; layout choice persists per tab.
- `gremliner-cli layout-bench --dataset air-routes --layout fa2`.

Out of scope: geographic/map layout (later), 3D.

## Capabilities

### New Capabilities
- `graph-layouts`: arranging graph nodes automatically with several algorithms, smoothly and without blocking interaction.

### Modified Capabilities
- None.

## Impact

- Crates: `graph-layout` (new code), `graph-canvas`, `ui-results`, `gremliner-cli`.
- ADRs: 0009, 0010, 0027.
