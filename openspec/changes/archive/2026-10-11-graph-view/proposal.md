# Proposal

## Why

Seeing the graph is the heart of a Gremlin IDE. With the canvas proven (graph-canvas-spike), we turn query results
into an explorable, G.V()-style visualisation: expand neighbours, accumulate results, hide/pin/filter, and navigate fluidly.

## What Changes

- A **Graph** tab in the results dock (default for element/path results), fed by the shared `ElementSet` normalisation.
- Results merge into a per-tab graph store; a toggle chooses "clear on new query" (default) or "accumulate".
- Fetch edges between returned vertices automatically (per-tab option, on by default), and fetch missing endpoint vertices for edge-only results.
- Interactions: double-click to expand neighbours (both directions, capped and paged), context menu (expand in/out/both by edge label, hide, pin/unpin, select neighbours, copy id, open in table, search path from here), keyboard (Delete hides, Ctrl/Cmd+A select all, F fits).
- Search box that finds nodes by id or property and focuses them; a legend of labels with counts and visibility toggles; filter panel by label and property predicate.
- Toolbar: fit, zoom, layout picker (graph-layouts), stylesheet picker (graph-stylesheets), minimap toggle, export (PNG/SVG in `export`).
- The status line shows visible/total node and edge counts and the selection count.

Out of scope: layout algorithms beyond the spike's (graph-layouts), styling rules (graph-stylesheets), editing (graph-inspector-editing).

## Capabilities

### New Capabilities
- `graph-view`: visualising query results as an interactive node-link graph that users can expand, filter and navigate.

### Modified Capabilities
- None.

## Impact

- Crates: `graph-store` (merge, expand bookkeeping, filters), `graph-canvas`, `ui-results`, `gremlin-model` (ElementSet).
- ADRs: 0006, 0008, 0009.
