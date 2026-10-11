# Design

## Context

`crates/gremliner/src/main.rs` currently opens a gpui-kit hello window. gpui-kit provides `Dock`/`DockArea` with
serialisable state, `Root`, theme support, and a `List`/`Input` for palette building. See proposal.md.

## Goals / Non-Goals

**Goals:**
- A shell later changes plug panels into via a small registration API (`PanelRegistry`).
- Establish the token system (ADR-0023) and the I/O bridge (ADR-0005).

**Non-Goals:**
- Multi-window workspaces (single main window in v1).

## Decisions

- **Panel registration:** `ui-workspace` exposes `trait WorkspacePanel` (id, title, icon, default dock position)
  wrapping gpui-kit `Panel`. Features register panels at startup; the dock layout is serialised as gpui-kit
  `DockAreaState` JSON in SQLite `ui_state` (ADR-0022).
- **Actions & keymap:** every user command is a GPUI action declared with `actions!`; keybindings in a single
  `keymap.rs` table per platform. The palette reads the action registry plus a `CommandMeta` (title, category,
  `when` predicate) map. Fuzzy matching via `nucleo-matcher` (new small dep, noted in ADR-0023 follow-up? no:
  it is a leaf utility, no ADR needed).
- **Theme tokens:** `ui-workspace::theme::Tokens` struct with semantic roles; built from gpui-kit `Theme` plus our
  overrides; light and dark JSON files in `assets/themes/`. A unit test computes WCAG contrast for all text×surface pairs.
- **Fonts:** embed Inter and JetBrains Mono via `AssetSource` and register with the text system at startup.
- **Bridge:** `ui_workspace::io::spawn_io(cx, future) -> Task<Result<T>>` using a global `tokio::runtime::Runtime` created in `main`.
- **Storage bootstrap:** `storage` crate gets SQLite open/migrate, `settings` and `ui_state` tables; runs on a dedicated thread with a channel API.
- **Perf HUD:** an overlay element sampling frame durations (ring buffer of 240), showing p50/p95/max.
- **Screenshot:** `cargo xtask screenshot [-- app-args]` runs the release binary with `--exit-after 4`, waits 3 s, and
  captures with `grim` (Wayland, using window geometry from `hyprctl`/`swaymsg` when available, else full output) or
  `import -window <id>` (X11 via `xdotool search --name Gremliner`). Saves to `target/screenshots/<timestamp>.png`.

## Risks / Trade-offs

- [gpui-kit dock API changes on upgrade] → isolate in `ui-workspace::dock` module; upgrade is its own change.
- [Screenshot tooling differs per compositor] → document the supported paths; failure message lists the tools to install.

## Verification

- `cargo test -p ui-workspace` (headless `#[gpui_kit::test]` for dock toggle, palette filtering, settings persistence).
- `cargo xtask screenshot` produces a PNG of the welcome screen in light and dark (`--theme dark`).
- `cargo run -p gremliner --release -- --perf-hud --exit-after 3` exits 0.

## As built (deviations and additions)

- **Fonts** are embedded with `include_bytes!` and registered via `text_system().add_fonts` rather than through
  the `AssetSource` (simpler, same result). Inter 4.1 (Regular/Medium/SemiBold/Bold) and JetBrains Mono 2.304
  (Regular/Bold/Italic), OFL licences beside the files in `assets/fonts/`.
- **Themes** are a gpui-kit `ThemeSet` (`assets/themes/gremliner.json`, "Gremliner Light/Dark"); `Tokens`
  carries sizes only (rem-based spacing on a 4 px grid at the default 14 px, so zoom scales everything).
  Colours are read through `cx.theme()`. The contrast test resolves colours with gpui-kit's own loader.
- **Settings view** is a closable tab in the editor area (gpui-kit `Settings` component), not a separate
  window. Reset asks for confirmation in an alert dialog.
- **Assets:** `AppAssets` = `icon_assets!` extras chained before gpui-kit's default icon set.
- **Startup reads** (settings, layout, window geometry, recent commands) are blocking reads before the first
  frame (< 1 ms); every write goes through the storage thread. Quit flushes the storage queue.
- **Screenshot:** KDE uses `spectacle -b -n -a`; grim/X11 paths as designed. Each run uses a fresh data dir.
- **`--command <action>`** dispatches a registered action after the first frame (screenshots, agents).
  `--data-dir`/`GREMLINER_DATA_DIR` and `--no-persist` select or disable the database.
- **Palette** ranks with nucleo-matcher (`palette::rank`, pure and unit-tested); empty-query order is recent
  first, then registration order. Keyboard Shortcuts uses `Ctrl+/` (chords display only their first key).
- **Test gaps:** the System-theme scenario drives `AppSettings::set_appearance` (what the window observer
  calls) because gpui exposes no public way to fake an OS appearance change. The slow-task scenario uses a
  gated task instead of a literal 5 s sleep. `scenario_app_shell_exit_after_timeout` launches the real
  binary and runs only with `GREMLINER_GUI_TESTS=1` (it opens a window).
