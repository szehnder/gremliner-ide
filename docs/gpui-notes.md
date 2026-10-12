# GPUI / gpui-kit notes

Lessons learned while building with gpui-kit 0.7.x (`gpui-pre` 0.3.x). Add to this file whenever an API surprises you.

## Basics
- `use gpui_kit::*;` brings in GPUI. Components are under `gpui_kit::component::*` and unstyled behaviour under `gpui_kit::base::*`.
- Entry point: `gpui_kit::application().run(|cx| { gpui_kit::init(cx); gpui_kit::open_window(opts, cx, |window, cx| cx.new(|cx| View::new(window, cx))) })`.
- Headless UI tests: add `gpui-kit = { features = ["test-support"] }` under dev-dependencies and use `#[gpui_kit::test]`. See gpui-kit's `TESTING.md`.

## Painting many primitives (graph canvas spike)
- **Always paint large batches inside `window.paint_layer(bounds, |window| …)`.** Outside a layer every
  `paint_quad`/sprite is inserted into the scene's bounds tree to compute a draw order: ~1.5–2 µs each when
  primitives overlap. Inside a layer they share one order and cost ~90 ns.
- Primitives in the same layer draw in insertion order. Use separate layers for things that must stack (edges, then nodes).
- `Path` (`push_triangle` is public) is rasterised through a full-window MSAA intermediate pass, and vertices
  are copied several times. It's fine for a few shapes, slow for tens of thousands of triangles.
- `window.paint_svg(bounds, key, Some(svg_bytes), TransformationMatrix, color, cx)` draws an instanced
  monochrome sprite with an arbitrary 2×2 transform plus translation (applied in device pixels, around the
  window origin). Culling uses the *untransformed* bounds, so request the tile inside the content mask and
  move it with the transform. Good for rotated lines and arrows.
- `window.paint_image(bounds, image_bounds, …, Arc<RenderImage>, 0, false)` draws a straight-alpha **BGRA**
  image scaled to `image_bounds` and clipped to `bounds`. Every new `RenderImage` is uploaded to the atlas.
  Call `window.drop_image(old)` when replacing one.
- `window.request_animation_frame()` re-notifies the current view next frame. On Wayland no frames arrive
  while the window is hidden or occluded, and unfocused windows are throttled to 30 fps (`inactive_frame_interval`).
- GPUI logs through the `log` crate; the app installs `env_logger` (`RUST_LOG=gpui_wgpu=info` shows the
  selected GPU). `ZED_DEVICE_ID=0x<pci-device-id>` forces an adapter.
- Headless tests: `cx.add_window_view(|_, cx| View::new(cx))`, then `cx.run_until_parked()` to lay out and
  paint, then `cx.simulate_mouse_down/move/up(...)`; read state with `view.read_with(cx, …)`.

## App shell (dock, themes, palette, tests)
- **`#[gpui_kit::test]` + `use gpui_kit::*` in a test module recurses.** With `test-support`, the glob
  exports GPUI's `test` macro, which shadows the built-in `#[test]` that `#[gpui_kit::test]` expands to.
  In unit-test modules that glob-import (directly or via `use super::*`), add
  `use ::core::prelude::v1::test;`, or import Kit items explicitly.
- **Asset sources:** `gpui_kit::assets::Assets::load` returns `Err` (not `Ok(None)`) for paths it does not
  ship, so chain it **last** after any `icon_assets!` extras (`ui_workspace::assets::AppAssets`). Register
  the source with `application().with_assets(..)` or every `Icon` renders blank.
- **Dock:** build with `DockSkin::dock_area(id, Some(version), window, cx)`; add panels through
  `panel_handle(entity)` (`DockLayout::tabs().panel_view(..)`, `add_panel_view`), otherwise the skin
  cannot read titles. `DockArea::dump()/load()` round-trip `DockAreaState` (serde). Restoring needs every
  panel registered with `register_panel(cx, name, builder)` first. Dock resize maps the pointer position
  to the dock edge (no grab offset), so a drag that starts inside the 1 px handle lands within ~2 px.
- **Dialogs** live in gpui-kit's `Root` overlay, outside the app view: actions dispatched from inside a
  dialog do not bubble to the workspace. The palette closes, then focuses the workspace and dispatches
  (`window.defer`). Open dialogs focus themselves; focus an input inside one via a deferred
  `state.focus(window, cx)`. Headless tests must open windows with `gpui_kit::open_window` (wraps `Root`)
  and `VisualTestContext::from_window`, not `add_window_view`, or `open_dialog` has nowhere to render.
- **Actions route through the rendered dispatch tree.** `window.dispatch_action` before the first frame
  is lost; do startup dispatches inside `window.on_next_frame`.
- **Key hints:** `Kbd::binding_for_action` shows the *last* binding registered for an action, and only the
  first keystroke of a chord. Register the primary binding last; avoid chords for commands you advertise.
- **Theme:** load a gpui-kit `ThemeSet` JSON with `ThemeRegistry::load_themes_from_str`, assign
  `Theme::light_theme/dark_theme`, then `Theme::change(mode)`. Changing mode reloads the theme file and
  resets font sizes, so set `font_size`/`mono_font_size` afterwards via `Theme::update`. `Theme::font_size`
  is the window rem size (gpui-kit `Root` applies it), so it scales every component: that is our zoom.
  `Theme::default().apply_config(&Rc::new(config))` resolves a theme without an `App` (contrast tests).
- **Fonts:** `cx.text_system().add_fonts(vec![Cow::Borrowed(bytes)])`. The test platform's text system is
  a no-op; to test real font loading use `gpui_kit::platform::current_platform(true).text_system()`.
- **Test ids:** only elements with `.id(..).test_support()` are visible to `window.find/try_find/click`.
- **No public way to fake an OS appearance change** in tests (`TestWindow::simulate_appearance_change` is
  reachable only inside gpui). Keep the observer thin and test the handler it calls.
- **`--command <action>`** (e.g. `workspace::OpenCommandPalette`) dispatches any registered action after
  the first frame; combine with `cargo xtask screenshot` to capture a specific UI state.

## Screenshots on the dev machine (KDE Wayland)
- Windows launched by `cargo xtask screenshot` from an agent's shell receive one or two frames and then no
  more Wayland frame callbacks (even `--perf-hud`, which requests every frame, records a single interval),
  although spectacle captures them as the active window. Background results that arrive later (a
  connection finishing, a query completing) are therefore not visible in the PNG. Capture states that exist
  by the second frame (`--command` dispatches right after the first) and verify asynchronous behaviour with
  headless tests and logs (`RUST_LOG=ui_workspace=info` logs connection status changes). Interactive
  launches by the user are not affected in the same way; see `docs/benchmarks/graph-canvas-spike.md` for the
  frame-pacing notes from the render gate.

## Code editor (query-editor)
- `EditorState::new(window, cx).language("gremlin").default_value(..)` + `Editor::new(&state)`. Register a
  tree-sitter language with `component::highlighter::LanguageRegistry::singleton().register(name,
  &LanguageConfig::new(name, language, vec![], highlights, "", ""))` (gpui-kit feature `tree-sitter`). The
  grammar must use the same `tree-sitter` crate version as gpui-kit (0.26).
- **Highlight queries:** only gpui-kit's capture names are themed (`function`, `variable.special`, `type`,
  `constant`, `string`, ...; dotted names fall back to their prefix), and **the first matching pattern
  wins** for a node, so put specific patterns before general ones. Editor colours come from the theme's
  `highlight` section (`editor.background`, `syntax.*`); without one a dark theme gets a white editor.
- `set_value` does not emit `InputEvent::Change` (no events while setting); mirror programmatic changes
  yourself (`QueryEditor::set_text`). `replace_all` is one undoable edit (use it for formatting);
  `replace` replaces the selection.
- Diagnostics: `state.diagnostics_mut()` → `reset(&rope)` + `push(Diagnostic::new(start..end, msg)
  .with_severity(..))` with LSP `Position`s (`rope.offset_to_position`), then `cx.notify()`.
- Hover: set `state.lsp_mut().hover_provider = Some(Rc::new(impl HoverProvider))`; return markdown `Hover`.
- Range decorations (`create_range_decorations_collection`, `RangeDecorationStyle::Fill`) track edits on
  their own; good for the "statement that will run" highlight.
- The editor binds `secondary-enter` (inserts a newline and emits `PressEnter`). To make Ctrl+Enter run a
  query instead, bind your action in a more specific context: `Some("QueryEditor > Input")`.
- Typing `(` auto-inserts `)`: tests that need exact text should set it rather than type it.
- Multi-tab bars render `Panel::tab_name()` instead of `title()` when it is `Some`; leave it `None` to show
  rich titles (badges). `DockSkin::set_close_button_visible(true)` enables tab close buttons (panels opt out
  with `closable() -> false`).
- In headless tests `background_spawn` work runs on the test dispatcher's thread, so latency measurements
  include "background" work; real builds are faster.

## Query execution and results
- From async contexts (`cx.spawn`, `cx.spawn_in`), `entity.read_with(cx, ..)` and `entity.update(cx, ..)`
  return the value directly in this GPUI version (no `Result`); `WeakEntity::update` still returns a
  `Result`. `Task<T>` returned by an entity method can be awaited directly.
- Bridging a tokio stream to the UI: run the stream on the I/O runtime (`spawn_io`), send messages over a
  `futures::channel::mpsc` channel, and drain it in a `cx.spawn` task that updates the entity. Stop with a
  `oneshot` the I/O loop `tokio::select!`s on.
- **Don't rebuild views per batch.** A results view that clones all items and rebuilds its table model on
  every batch went quadratic (609 rebuilds, 115 s for 57k edges). Read items in place (move them out of
  the entity and back: `mem::take`), and throttle rebuilds adaptively: interval = max(100 ms, 4 × last
  rebuild cost).
- Key contexts can carry state: `KeyContext::new_with_defaults()` + `add("QueryRunning")` on the root while
  a query runs, and bind Esc in `"QueryRunning > Input"` (`>` means any ancestor), so Esc cancels only then.
- gpui-kit `TableDelegate::render_th` can replace the header cell entirely; we render our own sort headers
  (ascending → descending → off) because the built-in cycle starts descending. `uniform_list` (gpui)
  virtualises fixed-height rows: the JSON tree and console use it.
- Test-dispatcher timers use a fake clock: a `background_executor().timer()` never fires in a test unless the
  clock is advanced, so throttled work must also run on the final update.
- Gremlin Server 3.7 keeps evaluating a script after the client stops reading (no protocol-level cancel
  for sessionless `eval`); Cancel stops delivery, and the server stops at its evaluation timeout.

## Graph view
- gpui-kit `ContextMenuExt::context_menu` builds its menu in a deferred call on the next frame, so a child's
  own right-button `on_mouse_down` runs first: the canvas records the node under the pointer there, and the
  menu builder reads it. `PopupMenu::action_context(focus)` routes `.menu(label, action)` items to that
  focus handle (and shows their key hints); closures go in `PopupMenuItem::new(..).on_click(..)`.
- Bind keys like Delete to a context set on a wrapper around the canvas only (`"GraphCanvas"`), not on the
  whole view, or they fire while typing in the view's search box.
- `window.is_action_available(&action, cx)` tells whether the focused element's dispatch path handles an
  action; the palette uses it to offer graph commands only while the graph is focused.
- `VisualTestContext::simulate_event(MouseDownEvent { click_count: 2, .. })` simulates a double click
  (`simulate_mouse_down` always sends `click_count: 1`).
- Camera tweens in the canvas use real `Instant`s, not the test clock: tests sleep for the tween and then
  `render_frame`.
- Gremlin Server returns `path()` (and `select()`) vertices without properties; the graph marks such
  vertices as placeholders and fetches them with `g.V(ids)` so captions appear.
- Toolbar dropdowns: `use gpui_kit::component::menu::DropdownMenu as _;` then
  `Button::new(..).dropdown_menu(|menu, window, cx| menu.item(PopupMenuItem::new("x").checked(on).on_click(..)))`.
- A background thread (the layout worker) cannot wake GPUI directly; the canvas polls its latest-wins frame
  slot in `paint` and keeps calling `window.request_animation_frame()` while a layout runs.
