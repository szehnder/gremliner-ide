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
