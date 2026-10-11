# Proposal

## Why

Every feature lives inside the workspace shell. Building it first proves out GPUI/gpui-kit (ADR-0003) on all
platforms and sets the visual standard (ADR-0023) that every later surface inherits.

## What Changes

- Main window with a gpui-kit Dock layout: left sidebar (Connections), centre editor tab area, bottom results dock,
  right panel area (Assistant / Inspector), status bar. Panels resize, collapse and reorder by drag. The layout persists across restarts.
- Theme tokens (light/dark/system), embedded fonts (Inter, JetBrains Mono), Lucide icons.
- Command palette (Ctrl/Cmd+Shift+P) listing every registered action with its keybinding, with fuzzy search.
- Settings window (Ctrl/Cmd+,) with General (theme, font size, telemetry notice) as the first page; other changes add pages.
- Polished empty state (welcome screen: "Add a connection", recent connections, keyboard tips).
- tokio runtime bridge (ADR-0005) and `--perf-hud` frame-time overlay.
- App flags `--open-connection`, `--run`, `--exit-after`, `--perf-hud` (no-ops until their features exist) and the `cargo xtask screenshot` implementation.

Out of scope: connection forms, editor content and results (later changes).

## Capabilities

### New Capabilities
- `app-shell`: window, dock layout, panels, status bar, persistence of layout and window state, startup flags.
- `theming`: light/dark/system themes, tokens, fonts, accessibility contrast.
- `command-palette`: discoverable, searchable list of all actions with keybindings.
- `settings`: settings window and persistence of user preferences.

### Modified Capabilities
- None.

## Impact

- Crates: `gremliner`, `ui-workspace`, `storage` (settings table: introduce SQLite + migrations, ADR-0022).
- ADRs: 0003, 0005, 0022, 0023, 0024.
