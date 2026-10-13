# 0038. Graph image export through SVG and resvg

- Status: accepted
- Date: 2026-10-12
- Amends: [0006](0006-graph-canvas-on-gpui-primitives.md) (adds an export path beside the on-screen renderer)

## Context
Graphs need to be exported as PNG and SVG (export). GPUI cannot render a view offscreen to an image, and
the canvas draws with GPU primitives that have no file output.

## Decision
- `graph_store::export::svg` writes the scene to SVG directly from the store (positions, sizes, captions,
  dashes) and the resolved looks (shapes and colours, with the theme palette passed in). It writes one
  `class="node"` shape per visible node, edge lines with arrowhead markers, and a `<text>` per caption.
- PNG is that SVG rendered by `resvg`/`tiny-skia` at the chosen scale, transparent or on the theme
  background. resvg 0.46 is built with the features GPUI already uses (`text`, `system-fonts`), so it
  adds no new crates to the build. Captions use the system fonts.
- GraphML, GraphSON, JSON Lines and "Copy as JSON" are writers in `gremlin-model` over the graph's
  elements, rebuilt from the store with their original ids.
- Images go to the clipboard through GPUI's `ClipboardItem::new_image` (PNG).

## Alternatives considered
- **A second CPU renderer in the canvas crate**: it would duplicate drawing code that SVG already
  describes, and would need its own text shaping.
- **Screenshotting the window**: there is no portable API, and it is limited to the visible area at
  screen resolution.

## Consequences
- Exports match the screen's layout and styles. Effects drawn only by the GPU path (the hover glow,
  level-of-detail rasters) are not exported.
- Very large graphs make large SVGs; PNG rendering runs on the background executor.

## Revisit when
GPUI gains offscreen rendering of a view to an image.
