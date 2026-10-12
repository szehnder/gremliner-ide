# Proposal

## Why

Debugging graph data means inspecting element details and fixing them on the spot. G.V() lets users view
properties in a side panel and create, update and delete vertices, edges and properties; we do the same with undo
and strong safety on production connections.

## What Changes

- Inspector panel (right dock) showing the selected element: id, label, properties (with types, multi/meta-properties), in/out degree, and, for edges, endpoints with navigation.
- Editing: add/edit/delete properties (typed value editor), create vertex (label + properties), draw an edge by dragging from a node with Alt held (choose label), delete elements; all as generated Gremlin shown in a confirmation preview before running.
- Undo/redo of edits within the session where the inverse operation is known (e.g. property set → restore old value; created vertex → drop).
- Safety: production and read-only connections require explicit confirmation; bulk deletes show counts.

Out of scope: schema constraints validation (Neptune has none; JanusGraph later).

## Capabilities

### New Capabilities
- `graph-editing`: inspecting graph elements and changing graph data from the visual graph with previews, confirmation and undo.

### Modified Capabilities
- None.

## Impact

- Crates: `ui-results` (inspector), `graph-canvas` (edge drawing gesture), `graph-store` (apply edits), `gremlin-syntax` (literal formatting for generated queries).
- ADRs: 0020, 0030.
