# Proposal

## Why

Results and visualisations end up in documents, tickets and other tools. Users need to export graphs as images
and data in standard formats without leaving the IDE.

## What Changes

- Graph export: PNG (at 1×/2×/4× scale, transparent or theme background), SVG (vector, with captions), visible area or whole graph.
- Data export: CSV and JSON (already in tables) plus GraphML and GraphSON for the current graph, and JSON Lines for results.
- Copy to clipboard: graph image (PNG), selected elements as JSON.

Out of scope: database dumps or bulk export (use server-side tools).

## Capabilities

### New Capabilities
- `export`: exporting graph visualisations and result data to standard image and data formats.

### Modified Capabilities
- None.

## Impact

- Crates: `graph-canvas` (offscreen render to image, SVG writer), `gremlin-model` (GraphML/GraphSON writers), `ui-results`.
- ADRs: 0006.
