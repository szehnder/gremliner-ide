# 0010. Barnes-Hut ForceAtlas2 as the default layout

- Status: accepted
- Date: 2026-10-11

## Context
G.V() users expect an organic force layout by default, plus hierarchical, circular and grid options.

## Decision
Default: ForceAtlas2 with Barnes-Hut quadtree repulsion (O(n log n)), linlog off, gravity on, adaptive speed.
Also provided: hierarchical (Sugiyama: cycle removal, longest-path layering, barycentric crossing reduction,
Brandes–Köpf coordinates), radial (BFS from selection), circular, grid, concentric (by degree). All implemented
in-house in `graph-layout`, no external layout dependency.

## Alternatives considered
- **fdg / force-graph crates** — unmaintained or not tuned for 50k nodes.
- **Fruchterman–Reingold** — O(n²) without approximation.

## Consequences
- Deterministic seeding for tests (`seed` parameter), enabling snapshot tests of layouts.

## Revisit when
Users request constraint-based or geographic layouts (map layer: separate ADR).
