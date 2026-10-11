# 0006. Graph canvas built on GPUI paint primitives with LOD and culling

- Status: accepted — gate passed with the raster level of detail in [0032](0032-raster-level-of-detail.md) (see `docs/benchmarks/graph-canvas-spike.md`)
- Date: 2026-10-11

## Context
The graph view must feel like G.V(): smooth pan/zoom/drag at tens of thousands of elements. Upstream GPUI does not
expose custom GPU pipelines portably; it does batch-render `Quad`s (instanced, with corner radii and borders),
sprites, text, and tessellated `Path`s very efficiently.

## Decision
`graph-canvas` is a custom GPUI `Element` that paints:
- **Nodes** as quads with full corner radius (circles), border for selection/hover, batched per style.
- **Edges** as polygons built into **one `Path` per style bucket** (thin quads per segment, arrowheads as triangles),
  rebuilt only when camera or positions change.
- **Labels** via GPUI text with a shaped-line cache keyed by (text, size); drawn only above a minimum on-screen size.

Performance techniques: uniform-grid spatial index for culling and O(1) hit-testing; level-of-detail tiers
(far: no labels/arrows, hairline edges; very far: density-only edges); no per-frame allocation (reused buffers).

**Gate:** on the reference Linux machine, 10k nodes / 20k edges ≥ 120 fps pan/zoom; 50k / 100k ≥ 60 fps;
paint CPU < 4 ms at 10k. Measured by `gremliner-cli render-bench` and the in-app `--perf-hud`.

## Alternatives considered
- **CPU raster to an image each frame** — readback/upload of a 4K image per frame is too slow.
- **Custom wgpu pipeline** — not available upstream; see ADR-0007.

## Consequences
- Rendering is portable across all GPUI backends with no unsafe code.
- Fancy effects (glow, curved edges at scale) are limited; curves only at near zoom.

## Revisit when
The gate fails (→ ADR-0007), or GPUI gains a public custom-render hook.
