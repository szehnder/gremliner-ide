# 0032. Raster level of detail for dense graph views

- Status: accepted
- Date: 2026-10-11
- Refines: [0006](0006-graph-canvas-on-gpui-primitives.md)

## Context
The graph-canvas spike measured GPUI's per-primitive cost at roughly 90 ns in our paint plus a similar amount
inside GPUI afterwards (~200 ns total), even when batched in one `paint_layer`. A 50k-node/100k-edge view is
150k primitives ≈ 30 ms of CPU per frame, so pure vector drawing cannot meet the ADR-0006 targets
regardless of GPU. (Without layers it was worse: ~1.5–2 µs per primitive, from GPUI's overlap tree.)

## Decision
For views with more than 6,000 elements, the canvas draws the bulk of the graph as a cached raster image:
- A parallel CPU rasteriser (`graph_store::view::raster`, rayon, 16-row bands, Xiaolin Wu hairlines,
  single-channel transmittance for edges, anti-aliased discs) renders a region 20% larger than the
  viewport on each side, on a dedicated pool with half the cores.
- Every frame paints that image transformed to the current camera (pan = translate, zoom = scale) — one
  primitive. A new raster is requested when zoom drifts > 4%, the view leaves the margin, or the graph
  changes; refreshes are throttled to one per 120 ms while the camera moves, so detail catches up within
  ~120 ms and is crisp at rest.
- Zoomed out (below full detail): nodes and edges are rasterised; selected/hovered nodes and edges touching
  the selection are drawn as vectors on top.
- Zoomed in (full detail): only edges are rasterised; nodes, labels and highlighted edges (with arrowheads)
  are vectors. Arrowheads on non-highlighted edges are omitted in dense views.
- Small views (≤ 6,000 elements) stay fully vector.

## Alternatives considered
- **Vector only** — measured 22 fps at 50k/100k on a GTX 1660 Ti; cannot meet budget.
- **Custom wgpu renderer (ADR-0007)** — the best long-term performance, but the WGPUI fork is not
  compatible with gpui-kit and upstream GPUI offers no custom-pipeline hook; deferred.
- **Rasterise every frame** — 14 ms (10k) to 53 ms (50k) of CPU per frame; too slow for 120 fps.

## Consequences
- Frame time no longer scales with graph size: measured p95 8.26 ms (10k/20k) and 13.1 ms (50k/100k) on the
  reference laptop's default (integrated) GPU; see `docs/benchmarks/graph-canvas-spike.md`.
- While zooming quickly, the image is briefly scaled (slightly soft) until the refresh lands.
- Hit-testing, selection and editing are unaffected (they use the spatial grid, not pixels).
- The rasteriser is pure Rust and unit-tested, including a banded-vs-single-pass equivalence test.

## Revisit when
GPUI exposes custom GPU pipelines or a cheap per-primitive path, or the raster refresh latency becomes a UX
complaint.
