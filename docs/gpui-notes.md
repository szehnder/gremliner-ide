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
