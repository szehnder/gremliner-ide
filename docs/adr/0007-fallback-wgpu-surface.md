# 0007. Fallback: custom wgpu surface for the graph canvas

- Status: proposed — not activated: the ADR-0006 gate passed with ADR-0032. Note: the WGPUI fork is not compatible with gpui-kit, so activating this would also mean patching GPUI's wgpu renderer.
- Date: 2026-10-11

## Context
If GPUI primitives cannot reach the ADR-0006 targets, we need a path to direct GPU rendering.

## Decision
Keep `graph-canvas` behind a `GraphRenderer` trait (scene in → draw). If the gate fails, implement a renderer on
the WGPUI fork's `WgpuSurface` element (embeds a raw wgpu render pass in the UI tree) or an equivalent upstream
contribution, with instanced node/edge shaders. The trait means the rest of the app is unaffected.

## Alternatives considered
- **Switch the whole app to Iced** — throws away the IDE components we chose GPUI for.

## Consequences
- Small up-front cost: the trait boundary in the spike.
- A fork dependency, if activated, must be pinned and tracked by its own ADR.

## Revisit when
Gate results are in.
