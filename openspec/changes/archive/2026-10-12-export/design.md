# Design

## Context

The canvas renders through the `GraphRenderer` trait (ADR-0006). GPUI has no offscreen-render-to-image API, so PNG
export uses a separate CPU rasteriser on the same scene description.

## Goals / Non-Goals

**Goals:**
- Exports look like the screen (same styles, same layout).

**Non-Goals:**
- Animated exports.

## Decisions

- **SVG first:** `graph-canvas::export::svg(scene, styles) -> String` writes the scene description to SVG directly.
- **PNG:** render that SVG with `resvg`/`tiny-skia` (pure Rust, new dependency → note in ADR-0006 consequences via a new short ADR `0031-resvg-for-image-export`) at the chosen scale.
- **GraphML/GraphSON writers:** in `gremlin-model`; GraphML uses `key` declarations per property with inferred `attr.type`.
- **Clipboard:** GPUI clipboard for text; image via platform clipboard support in GPUI if available, else write a temp PNG and copy its path (documented limitation).

## Risks / Trade-offs

- [Large graphs produce huge SVGs] → warn above 50k elements; PNG rendering streams via tiny-skia.

## Verification

- Unit tests for SVG structure, GraphML round trip (testenv load), JSON Lines.
- PNG export snapshot (dimension and non-empty pixel checks).
