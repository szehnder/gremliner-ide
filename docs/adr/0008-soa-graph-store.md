# 0008. Struct-of-arrays graph store with dense u32 indices

- Status: accepted
- Date: 2026-10-11

## Context
Gremlin element ids may be longs, strings, UUIDs or composite (JanusGraph RelationIdentifier). Rendering, layout,
and hit-testing want dense, cache-friendly arrays. Results arrive incrementally and are merged (expand neighbours,
accumulate queries).

## Decision
`graph-store` interns external ids (`GValue` id → `NodeIx(u32)` / `EdgeIx(u32)`) via a hash map and stores data
in parallel vectors: positions `Vec<[f32;2]>`, velocity, label index, style index, flags (selected/hidden/pinned),
adjacency as CSR rebuilt lazily, properties in a side table. Deletion uses tombstones + periodic compaction.
A monotonically increasing `revision` lets the canvas and layout detect changes cheaply.

## Alternatives considered
- **petgraph `StableGraph`** — convenient, but AoS layout and per-node allocation hurt at 100k elements.
- **Store raw `GValue`s only** — forces re-walking JSON on every frame.

## Consequences
- Layout and canvas operate on slices; trivially parallel with rayon.
- Property display needs a side-table lookup (fine: only for inspected elements).

## Revisit when
We need multi-graph views sharing one store.
