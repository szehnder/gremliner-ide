# Graph canvas spike: benchmark gate report

Change: `openspec/changes/graph-canvas-spike` · Decisions: [ADR-0006](../adr/0006-graph-canvas-on-gpui-primitives.md),
[ADR-0032](../adr/0032-raster-level-of-detail.md), [ADR-0031](../adr/0031-x86-64-v2-baseline.md) · Date: 2026-10-11

## Result: gate passed

| Graph | Target (p95 frame interval) | Default GPU (AMD Radeon Vega, integrated) | NVIDIA GTX 1660 Ti (PRIME offload) |
|---|---|---|---|
| 10k nodes / 20k edges | ≤ 8.3 ms (120 fps) | **8.26 ms**, 137.9 fps avg ✅ | 8.80 ms, 137.2 fps avg (marginal) |
| 50k nodes / 100k edges | ≤ 16.7 ms (60 fps) | **13.10 ms**, 128.1 fps avg ✅ | 12.84 ms, 132.4 fps avg ✅ |

Frame interval = time between successive canvas paints, i.e. what the user sees; the display is 144 Hz,
so 6.94 ms is the floor and any frame over budget shows up as 13.9 ms. p50 was 6.96–7.05 ms in every run
(full refresh rate).

## Reference machine
Laptop, 16 logical CPUs (AMD Ryzen, Renoir), 30 GB RAM, AMD Radeon Vega (integrated, drives the display)
+ NVIDIA GTX 1660 Ti Mobile, 1920×1080 @ 143.88 Hz, Manjaro Linux 6.6, KDE Plasma (Wayland), GPUI via
gpui-kit 0.7.1 (`gpui-pre` 0.3.8, wgpu/Vulkan). Window 1600×1000 logical px, scale factor 1.

## Method
`gremliner --render-bench N,E --bench-seconds 8` generates a seeded synthetic graph (8 labels, clustered
layout, 80% local / 20% long-range edges, i.e. a dense hairball, the worst case for fill), skips 30 warm-up
frames, then drives the camera for 8 s: pan sweep at fit zoom, zoom in to 8× and out, orbit at 3×. Reported
by the ignored tests in `crates/gremliner/tests/render_gate.rs`. `gremliner-cli render-bench` measures the CPU
side headlessly. The window must be visible and focused: Wayland stops frame callbacks for hidden windows.
`ZED_DEVICE_ID=0x2191` selects the NVIDIA GPU; `GREMLINER_NO_RASTER=1` disables the raster level of detail.

## How we got there (10k/20k p50/p95 frame interval, NVIDIA unless noted)

| Step | 10k/20k | 50k/100k | Finding |
|---|---|---|---|
| Naive: quads per node, one `Path` of triangles per edge bucket, minimap quads | 8 fps (AMD) / 24 fps | — | `paint_quad` cost ~1.5–2 µs each: GPUI inserts every primitive into its overlap (bounds) tree to compute draw order |
| Batch each kind inside `window.paint_layer` | 27.5 / 34.7 ms | — | Layer = one draw order for the batch, which skips the tree. Overlays went from 10 ms to 0.5 ms |
| Edges as transformed monochrome sprites instead of path triangles | 14.3 / 21.9 ms | 41.9 / 69.9 ms | Paths go through a full-window MSAA pass plus several vertex copies; sprites are instanced. Still ~200 ns per primitive in total |
| Raster level of detail (ADR-0032) | 7.05 / 14.6 ms | 6.98 / 12.7 ms | Frame cost decoupled from graph size; spikes on refresh frames |
| Dedicated half-size raster pool, edges-only raster at full zoom, label font-size quantised | 7.01 / 9.18 ms | 6.99 / 13.95 ms | The rasteriser was starving the UI thread |
| Refresh throttle 120 ms while moving, `x86-64-v2` | **6.97 / 8.80 ms** (AMD: 6.96 / **8.26**) | 6.99 / 12.84 ms (AMD: 7.05 / **13.10**) | Fewer ~12 MB texture uploads |

Headless CPU numbers (`gremliner-cli render-bench`, 16 threads): scene build (cull + LOD) 0.3 ms at 10k,
1.7 ms at 50k; full rasterisation of a 1600×1000 frame 13.7 ms at 10k, 53 ms at 50k (hairball, ~40–75 M pixel
blends; ~3.2 ns per blend single-threaded with x86-64-v2).

## Known limits and follow-ups
- While zooming fast in a dense view, the cached image is briefly scaled (slightly soft) until the next
  refresh, at most ~120 ms plus raster time.
- Arrowheads on non-highlighted edges are omitted in dense views at full zoom.
- PRIME offload (NVIDIA rendering, AMD display) adds a copy per frame; the default integrated GPU was better.
- A future custom-pipeline hook in GPUI (or a patched `gpui-pre-wgpu`) would remove the per-primitive
  overhead and the raster refresh latency; tracked by ADR-0007's "revisit" condition.
