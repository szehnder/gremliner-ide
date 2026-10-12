# Design

## Context

The canvas batches by style index (ADR-0006); theme tokens include a 12-colour graph palette (ADR-0023). See proposal.md.

## Goals / Non-Goals

**Goals:**
- Style changes re-render in one frame (style resolution is O(labels), not O(nodes), except property-bound sizes).

**Non-Goals:**
- Conditional rules (later).

## Decisions

- **Model:** `Stylesheet { id, name, vertex: IndexMap<Label, VertexStyle>, edge: IndexMap<Label, EdgeStyle>, globals }`
  serialised as JSON (`schema_version: 1`) in `stylesheets` table and for import/export.
- **Resolution:** `StyleIndex` maps label → `StyleId`; nodes store `StyleId` (u16). Property-bound sizes computed into
  a per-node `size: f32` array on the background executor with min/max over visible values.
- **Auto colour:** palette index = `fxhash(label) % 12`, with collision avoidance among labels present in the graph (linear probe), stable for the session and persisted into the stylesheet when the user edits it.
- **Captions:** template parsed once to segments (`{prop}`, literal); evaluated lazily for visible nodes and cached per (node, stylesheet revision).
- **Shapes:** circle = quad full radius; square = quad small radius; diamond/hexagon as paths (cached per size bucket); icons as monochrome sprites from Lucide SVGs rasterised by GPUI.
- **Editor UI:** right side panel listing labels present in the graph and stylesheet; colour picker (gpui-kit), shape segmented control, icon search, template input with property-key suggestions.

## Risks / Trade-offs

- [Path-based shapes cost more to draw] → LOD: far zoom draws all shapes as squares/circles.

## Verification

- Unit tests for resolution, template evaluation, size mapping, truncation, JSON round-trip.
- Headless UI tests for switching and connection defaults.
- Screenshot of air-routes with a styled stylesheet.

## As built

- **Model** (`graph_store::style`, serde): `Stylesheet { schema_version, id, name, vertex, edge, globals }`
  with `VertexStyle { color, shape, icon, caption, size }`, `EdgeStyle { color, width, caption, dashed }`,
  `Size::Fixed { px } | Size::Property { key, min, max }` and `Globals { label_density, min_label_px,
  max_caption_len, show_edge_captions }`. Colours are `"#rrggbb"`. A rule under `*` applies to every label
  without its own (the built-in Presentation sheet uses it).
- **Resolution:** `resolve(sheet, store, auto)` returns per-label `Looks` (colour as palette slot or RGB,
  shape, icon) and writes the per-element parts into the store: node radii, edge width multipliers,
  dashed flags, node and edge captions (templates rendered once per resolve, then truncated). It runs
  after every merge and on every stylesheet change, synchronously; at air-routes sizes it is a few
  milliseconds, so it does not use the background executor. Property-bound sizes scale over the visible
  elements of that label.
- **Auto colours:** FNV-1a of the label name modulo 12, linearly probed past slots other labels took, kept
  for the session in the library (`AutoPalette`), so a label keeps its colour across queries and tabs.
- **Rendering:** per-node radii drive culling, edge trimming, label anchors and hit-testing (the grid
  reaches as far as the largest node). Circles and squares are quads; diamonds and hexagons are inline-SVG
  sprites (a slightly larger copy underneath draws the selection border); icons are Lucide SVGs from the
  app's assets, drawn when nodes are at least 7 px in radius at full detail. All sprites use one 32 px
  tile per shape or icon scaled by a transform. Edges take their label's colour (translucent), width
  multiplier and dashes (6 + 2w on, 4 + w off). Edge captions are drawn at midpoints, at full detail
  only and limited to half the label budget. At far zoom every node is a default-size circle; the raster
  path uses the per-label colours.
- **Library and editor:** `ui_results::style_library::StyleLibrary` (built-ins Default and Presentation,
  create/duplicate/rename/delete/import, per-connection defaults, the session's auto colours) is observed
  by every graph view, so edits apply live. The editor (`style_editor`) is a 320 px side panel next to the
  canvas: stylesheet chips, rename, New/Duplicate/Delete/Import…/Export…, "Default for this connection",
  then a label (or "Labels & captions") with swatches for the 12 palette colours plus Auto, a hex field,
  shapes, icon name, caption template, fixed or property-bound size/width, dashed, density presets, minimum
  label size, maximum caption length and edge captions. The toolbar has a "Style: …" dropdown.
- **Storage:** migration 4 adds `stylesheets (id, name, json, updated_at)`; per-connection defaults live in
  `ui_state` under `graph.stylesheet.defaults`. `ui_workspace::styles` loads both at startup and persists
  changes in the background.
- **Deferred:** icon search with previews (the icon is typed by Lucide name), and a colour picker beyond
  the palette plus a hex field.
