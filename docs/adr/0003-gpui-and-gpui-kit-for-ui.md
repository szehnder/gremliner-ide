# 0003. GPUI + gpui-kit for the user interface

- Status: accepted (conditional on the ADR-0006 benchmark gate)
- Date: 2026-10-11

## Context
We need: a GPU-rendered retained/immediate hybrid UI; an IDE-grade code editor; dockable/resizable panels with
serialisable layout; virtualised tables and lists for 100k+ rows; theming; accessibility; and a headless UI test
harness so agents can verify behaviour.

GPUI is the framework behind the Zed editor. As of Feb 2026 its Linux renderer is wgpu (previously Blade, ~20% lower
CPU reported), macOS uses Metal, Windows DirectX. `gpui-kit` 0.7 (Longbridge) re-exports a pinned GPUI
(`gpui-pre` 0.3.x from crates.io), `gpui-base` and `gpui-component`: 75+ components including Dock, a code
editor (tree-sitter highlighting, LSP-style diagnostics/completion/hover, stable at 200k lines), virtual Table and
List, themes, AccessKit, and `#[gpui_kit::test]` headless UI tests. It powers the shipping Longbridge Pro app.

## Decision
Build the UI on GPUI via the single `gpui-kit` dependency (pinned exact minor). UI crates import `gpui_kit::*`;
no crate depends on `gpui` directly.

## Alternatives considered
- **Tauri v2 + SolidJS + Sigma.js/cosmos.gl** — quickest route to web polish and GPU graph layouts, but WebView memory
  overhead and WebKitGTK WebGL performance on Linux undermine the "peak performance" goal.
- **Iced** — first-class custom wgpu shaders (best graph-render headroom), but no IDE-grade editor, docking, or
  virtual table; we would build those ourselves.
- **egui** — excellent for tooling and custom painting (rerun), but immediate-mode look and text editing fall short of
  Compass-level polish.
- **Slint** — polished and fast, but markup-driven model and licensing are a poorer fit for an IDE with custom canvases.

## Consequences
- Upstream GPUI has no portable custom-shader hook; the graph canvas must be built from GPUI primitives (ADR-0006),
  with a fallback (ADR-0007).
- GPUI APIs churn; pinning `gpui-kit` exactly and upgrading deliberately (one OpenSpec change per upgrade) contains it.
- Less public training data on GPUI than on web stacks; mitigated by vendoring examples in `docs/gpui-notes.md` as we learn.

## Revisit when
The graph-canvas benchmark gate fails and the fallback is also unacceptable, or gpui-kit is abandoned.
