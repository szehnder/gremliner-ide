# Design

## Context

ADR-0006 picks GPUI primitives; ADR-0007 is the fallback. GPUI's `Window` paint API offers `paint_quad` (instanced,
corner radii, border), `paint_path` (tessellated, per-colour), and text shaping (`ShapedLine`). Custom elements
implement `Element` (`request_layout`, `prepaint`, `paint`). See proposal.md.

## Goals / Non-Goals

**Goals:**
- Hit the frame targets with zero per-frame heap allocation in the steady state.
- Produce numbers on the reference machine and record them.

**Non-Goals:**
- Visual styling rules and layouts (later changes); the spike uses palette-by-label colouring.

## Decisions

- **Scene model:** `graph-canvas` keeps `RenderCache { node_quads_by_style, edge_paths_by_style, label_runs }`, rebuilt
  when `(store.revision, camera, viewport, lod_tier)` changes. Pan-only changes reuse geometry when GPUI allows a
  transform (if no transform API exists, rebuild; this is the main thing the benchmark measures).
- **Edges:** each segment becomes a thin quad (2 triangles) appended into one `Path` per (colour, width) bucket;
  arrowheads are triangles in the same path. Below the LOD threshold, edge width is fixed at 1 device pixel and arrows are skipped.
- **Culling:** uniform grid over world space (cell = 2× median edge length), rebuilt on revision change; a node is
  drawn if its cell intersects the viewport; an edge is drawn if either endpoint cell is visible or the segment intersects the viewport (Liang–Barsky).
- **Hit-testing:** query the grid cells around the cursor; nodes by distance ≤ radius (topmost = highest draw index),
  edges by point-segment distance ≤ 4 px.
- **Labels:** shaped-line cache keyed by `(text, font_size_bucket)`; LRU 4k entries.
- **Camera:** f32 world → screen affine; zoom about the cursor; inertia with exponential decay; `fit_to_view` from the visible bounding box.
- **Benchmark:** `--render-bench N,E` opens a window, generates a seeded graph, runs a 10 s scripted camera path
  (pan sweep, zoom in/out 8×), records frame times via `request_animation_frame` timestamps and GPUI's frame
  callback, then prints p50/p95/p99/max and exits. `gremliner-cli render-bench` only measures CPU scene building (headless).
- **GraphRenderer trait:** `fn paint(&mut self, scene: &CanvasScene, bounds, window, cx)` keeps a swap point for ADR-0007.

## Outcome (implementation notes)

What the spike actually built, kept here so `/opsx:archive` preserves the reasoning (details in
`docs/benchmarks/graph-canvas-spike.md`):
- The pure geometry (camera, culling grid, hit-testing, LOD, scene building, minimap transform, rasteriser)
  lives in `graph-store::view` so the headless CLI can use it without GPUI (layering rule, ADR-0004).
  `graph-canvas` is the GPUI adapter.
- Every batch is painted inside `window.paint_layer`; without it GPUI's overlap tree costs ~1.5–2 µs per
  primitive.
- Edges and arrowheads are monochrome sprites with a transformation matrix (stripe and triangle tiles), not
  path triangles.
- Dense views (> 6,000 elements) use the raster level of detail (ADR-0032). x86-64-v2 baseline (ADR-0031).

## Risks / Trade-offs

- [GPUI path tessellation too slow for 100k edges] → fallback order: (1) cache path geometry in screen space and only
  rebuild on zoom changes; (2) draw edges as very thin rotated sprites if available; (3) ADR-0007.
- [Frame timing on Wayland is vsync-capped at 60 Hz] → measure CPU frame time (paint duration) as well as presented frames; the gate uses paint CPU time when the display caps.

## Verification

- `cargo run -p gremliner --release -- --render-bench 10000,20000` and `--render-bench 50000,100000` print stats meeting the spec.
- `cargo test -p graph-canvas` covers hit-testing, culling, LOD and camera math as pure functions.
- Results recorded in `docs/benchmarks/graph-canvas-spike.md`.
