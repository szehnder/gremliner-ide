//! Headless UI tests for the workspace shell: real views in a test window, driven by keystrokes and
//! pointer events. Scenario names map to `openspec/changes/app-shell/specs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::LazyLock;

use gpui_kit::component::dock::DockPlacement;
use gpui_kit::component::{ActiveTheme as _, ThemeMode, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowAppearance, WindowOptions, point, px, size,
};
use storage::{Storage, Table};
use ui_workspace::io::{self, Activity};
use ui_workspace::persist::DOCK_KEY;
use ui_workspace::settings::{AppSettings, GENERAL_KEY, GeneralSettings};
use ui_workspace::theme::{DEFAULT_UI_FONT_SIZE, ThemeChoice, Tokens};
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
});

const WINDOW: [f32; 2] = [1400.0, 900.0];

fn init_app(cx: &mut TestAppContext, storage: Option<Storage>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage,
                theme_override: None,
                secrets: Some(std::sync::Arc::new(storage::secrets::MemoryStore::default())),
            },
            cx,
        );
    });
}

/// Open a workspace window wrapped in gpui-kit's Root (dialogs and notifications need it).
fn open(cx: &mut TestAppContext) -> (Entity<Workspace>, VisualTestContext) {
    let (handle, workspace): (AnyWindowHandle, Entity<Workspace>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let workspace =
                    cx.new(|cx| Workspace::new(WorkspaceOptions::default(), window, cx));
                window.focus(&workspace.read(cx).focus_handle().clone(), cx);
                workspace
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(WINDOW[0]), px(WINDOW[1])));
    vcx.run_until_parked();
    (workspace, vcx)
}

fn dock_state(
    ws: &Entity<Workspace>,
    placement: DockPlacement,
    cx: &mut VisualTestContext,
) -> (bool, Option<Pixels>) {
    cx.update(|_, cx| {
        let dock = ws.read(cx).dock_area().read(cx);
        (dock.is_dock_open(placement), dock.dock_size(placement))
    })
}

fn bounds_of(id: &'static str, cx: &mut VisualTestContext) -> gpui_kit::Bounds<Pixels> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(id).bounds()
    })
}

fn exists(id: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|s| s.visible())
    })
}

fn drag(cx: &mut VisualTestContext, from: (f32, f32), to: (f32, f32)) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let p = |(x, y): (f32, f32)| point(px(x), px(y));
    cx.simulate_mouse_down(p(from), MouseButton::Left, Modifiers::none());
    // Past the drag threshold first, then to the target.
    cx.simulate_mouse_move(
        p((from.0 + 6.0, from.1)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(p(to), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(p(to), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn scenario_app_shell_first_launch(cx: &mut TestAppContext) {
    init_app(cx, Some(Storage::open_in_memory().unwrap()));
    let (_ws, mut cx) = open(cx);
    assert!(exists("welcome", &mut cx), "welcome view is shown");
    assert!(
        exists("welcome-add-connection", &mut cx),
        "primary Add connection action"
    );
    assert!(exists("connections-panel", &mut cx));
    assert!(exists("status-bar", &mut cx));
}

#[gpui_kit::test]
fn scenario_app_shell_toggle_the_results_dock(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (ws, mut cx) = open(cx);
    let before = bounds_of("welcome", &mut cx);
    let (open_before, size_before) = dock_state(&ws, DockPlacement::Bottom, &mut cx);
    assert!(open_before);

    cx.simulate_keystrokes("ctrl-j");
    let (open_after, _) = dock_state(&ws, DockPlacement::Bottom, &mut cx);
    assert!(!open_after, "results dock collapses");
    let grown = bounds_of("welcome", &mut cx);
    assert!(
        grown.size.height > before.size.height + px(100.0),
        "editor area grows: {:?} -> {:?}",
        before.size.height,
        grown.size.height
    );

    cx.simulate_keystrokes("ctrl-j");
    let restored = dock_state(&ws, DockPlacement::Bottom, &mut cx);
    assert_eq!(restored, (true, size_before), "previous size is restored");
    assert_eq!(
        bounds_of("welcome", &mut cx).size.height,
        before.size.height
    );
}

#[gpui_kit::test]
fn scenario_app_shell_resize_the_sidebar(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (ws, mut cx) = open(cx);
    let (_, width) = dock_state(&ws, DockPlacement::Left, &mut cx);
    let width = f32::from(width.unwrap());
    assert_eq!(width, f32::from(Tokens::SIDEBAR_WIDTH));
    let y = WINDOW[1] / 2.0;
    // The handle is the last logical pixel column of the dock.
    let handle_x = width - 1.5;
    drag(&mut cx, (handle_x, y), (handle_x + 100.0, y));
    // gpui-kit maps the pointer position to the dock edge, so the result is exact up to where inside the
    // 1 px handle the drag started.
    let (_, after) = dock_state(&ws, DockPlacement::Left, &mut cx);
    let after = f32::from(after.unwrap());
    assert!(
        (after - (width + 100.0)).abs() <= 2.0,
        "sidebar follows the drag: {after}"
    );

    // Dragging far past the maximum clamps.
    let handle_x = after - 1.5;
    drag(&mut cx, (handle_x, y), (handle_x + 600.0, y));
    let (_, clamped) = dock_state(&ws, DockPlacement::Left, &mut cx);
    assert_eq!(clamped, Some(Tokens::SIDEBAR_MAX_WIDTH));
}

#[gpui_kit::test]
fn scenario_app_shell_restart_restores_layout(cx: &mut TestAppContext) {
    let storage = Storage::open_in_memory().unwrap();
    init_app(cx, Some(storage.clone()));
    let (ws, mut vcx) = open(cx);
    vcx.simulate_keystrokes("ctrl-alt-b");
    vcx.update(|window, cx| {
        ws.update(cx, |ws, cx| {
            ws.dock_area().update(cx, |dock, cx| {
                dock.set_dock_size(DockPlacement::Left, px(310.0), window, cx);
                dock.set_dock_size(DockPlacement::Bottom, px(240.0), window, cx);
            });
            // What closing the window does.
            ws.save_now(window, cx);
        });
    });
    vcx.run_until_parked();
    storage.flush_blocking().unwrap();
    let mut saved = |p| dock_state(&ws, p, &mut vcx);
    let expected = [
        (DockPlacement::Left, saved(DockPlacement::Left)),
        (DockPlacement::Bottom, saved(DockPlacement::Bottom)),
        (DockPlacement::Right, saved(DockPlacement::Right)),
    ];
    assert!(!expected[2].1.0, "right panel collapsed before restart");

    // "Relaunch": a fresh workspace reading the same database.
    let (ws2, mut vcx2) = open(cx);
    for (placement, state) in expected {
        assert_eq!(
            dock_state(&ws2, placement, &mut vcx2),
            state,
            "{placement:?}"
        );
    }
    assert!(exists("welcome", &mut vcx2));
}

#[gpui_kit::test]
fn scenario_app_shell_corrupt_layout_falls_back_to_default(cx: &mut TestAppContext) {
    let storage = Storage::open_in_memory().unwrap();
    futures::executor::block_on(storage.set(
        Table::UiState,
        DOCK_KEY,
        "{\"center\": 17, garbage".into(),
    ))
    .unwrap();
    init_app(cx, Some(storage));
    let (ws, mut cx) = open(cx);
    assert!(exists("welcome", &mut cx));
    assert_eq!(
        dock_state(&ws, DockPlacement::Left, &mut cx),
        (true, Some(Tokens::SIDEBAR_WIDTH))
    );
    assert_eq!(
        dock_state(&ws, DockPlacement::Bottom, &mut cx),
        (true, Some(Tokens::RESULTS_HEIGHT))
    );
    assert_eq!(
        dock_state(&ws, DockPlacement::Right, &mut cx),
        (true, Some(Tokens::RIGHT_PANEL_WIDTH))
    );
    // No error dialog.
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
async fn scenario_app_shell_slow_background_task(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_app(cx, None);
    let (ws, mut vcx) = open(cx);
    // Stands in for a 5 s task: it finishes only when released, so the assertions below run while it is
    // in flight regardless of machine speed.
    let (release, wait) = tokio::sync::oneshot::channel::<()>();
    let task = vcx.update(|_, cx| {
        io::spawn_io_tracked("Loading schema", cx, async move {
            let _ = wait.await;
        })
    });
    vcx.run_until_parked();
    assert!(
        exists("status-activity", &mut vcx),
        "activity indicator while busy"
    );
    // The UI keeps handling input.
    vcx.simulate_keystrokes("ctrl-b");
    assert!(
        !dock_state(&ws, DockPlacement::Left, &mut vcx).0,
        "input handled while busy"
    );
    release.send(()).unwrap();
    task.await.unwrap();
    vcx.run_until_parked();
    assert!(!vcx.update(|_, cx| Activity::global(cx).read(cx).is_busy()));
    assert!(
        !exists("status-activity", &mut vcx),
        "indicator clears when done"
    );
}

#[gpui_kit::test]
fn scenario_theming_switch_to_dark(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (_ws, mut cx) = open(cx);
    cx.update(|_, cx| AppSettings::update_general(cx, |g| g.theme = ThemeChoice::Light));
    let light_bg = cx.update(|_, cx| cx.theme().background);
    cx.update(|_, cx| AppSettings::update_general(cx, |g| g.theme = ThemeChoice::Dark));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        assert_eq!(cx.theme().theme_name(), ui_workspace::theme::DARK_THEME);
        assert_ne!(cx.theme().background, light_bg);
    });
    // The workspace re-rendered without a restart.
    assert!(exists("welcome", &mut cx));
}

#[gpui_kit::test]
fn scenario_theming_follow_system_appearance(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (_ws, mut cx) = open(cx);
    cx.update(|_, cx| {
        AppSettings::update_general(cx, |g| g.theme = ThemeChoice::System);
        // What the window's appearance observer forwards when the OS switches.
        AppSettings::set_appearance(WindowAppearance::Light, cx);
        assert_eq!(cx.theme().mode, ThemeMode::Light);
        AppSettings::set_appearance(WindowAppearance::Dark, cx);
        assert_eq!(cx.theme().mode, ThemeMode::Dark);
        // An explicit choice ignores the OS.
        AppSettings::update_general(cx, |g| g.theme = ThemeChoice::Light);
        AppSettings::set_appearance(WindowAppearance::Dark, cx);
        assert_eq!(cx.theme().mode, ThemeMode::Light);
    });
}

#[gpui_kit::test]
fn scenario_theming_zoom_in(cx: &mut TestAppContext) {
    let storage = Storage::open_in_memory().unwrap();
    init_app(cx, Some(storage.clone()));
    let (_ws, mut cx) = open(cx);
    cx.simulate_keystrokes("ctrl-= ctrl-=");
    cx.update(|_, cx| {
        assert_eq!(
            AppSettings::general(cx).ui_font_size,
            DEFAULT_UI_FONT_SIZE + 2.0
        );
        assert_eq!(cx.theme().font_size, px(DEFAULT_UI_FONT_SIZE + 2.0));
    });
    assert!(exists("status-zoom", &mut cx));
    cx.run_until_parked();
    storage.flush_blocking().unwrap();
    assert_eq!(
        GeneralSettings::load(Some(&storage)).ui_font_size,
        DEFAULT_UI_FONT_SIZE + 2.0
    );
    cx.simulate_keystrokes("ctrl-0");
    cx.update(|_, cx| assert_eq!(AppSettings::general(cx).ui_font_size, DEFAULT_UI_FONT_SIZE));
}

#[gpui_kit::test]
fn scenario_settings_open_settings(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (_ws, mut cx) = open(cx);
    assert!(!exists("settings-panel", &mut cx));
    cx.simulate_keystrokes("ctrl-,");
    assert!(exists("settings-panel", &mut cx), "settings view opens");
    assert!(
        exists("settings-reset-general", &mut cx),
        "on the General page"
    );
    // Opening again focuses the existing tab instead of adding another.
    cx.simulate_keystrokes("ctrl-,");
    let count = cx.update(|window, cx| {
        window.render_frame(cx);
        gpui_kit::base::test_support::snapshots(window)
            .iter()
            .filter(|s| {
                s.path()
                    .last()
                    .is_some_and(|id| id.to_string() == "settings-panel")
            })
            .count()
    });
    assert_eq!(count, 1);
}

#[gpui_kit::test]
fn scenario_settings_persist_a_change(cx: &mut TestAppContext) {
    let storage = Storage::open_in_memory().unwrap();
    init_app(cx, Some(storage.clone()));
    let (_ws, mut cx) = open(cx);
    cx.update(|_, cx| AppSettings::update_general(cx, |g| g.editor_font_size = 15.0));
    cx.update(|_, cx| assert_eq!(cx.theme().mono_font_size, px(15.0)));
    cx.run_until_parked();
    storage.flush_blocking().unwrap();
    // A restart reads the settings row back.
    let stored: GeneralSettings = storage
        .get_json_blocking(Table::Settings, GENERAL_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(stored.editor_font_size, 15.0);
    assert_eq!(GeneralSettings::load(Some(&storage)).editor_font_size, 15.0);
}

#[gpui_kit::test]
fn scenario_settings_reset_general_settings(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (_ws, mut cx) = open(cx);
    cx.update(|_, cx| {
        AppSettings::update_general(cx, |g| {
            g.theme = ThemeChoice::Dark;
            g.ui_font_size = 18.0;
            g.editor_font_size = 16.0;
        });
    });
    cx.simulate_keystrokes("ctrl-,");
    cx.update(|window, cx| window.click("settings-reset-general", cx));
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "asks for confirmation"
    );
    // Nothing changes before confirming.
    cx.update(|_, cx| assert_eq!(AppSettings::general(cx).ui_font_size, 18.0));
    support::click_settled(&mut cx, "ok");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(*AppSettings::general(cx), GeneralSettings::default()));
}

#[gpui_kit::test]
fn scenario_command_palette_run_an_action(cx: &mut TestAppContext) {
    init_app(cx, None);
    let (ws, mut cx) = open(cx);
    cx.simulate_keystrokes("ctrl-shift-p");
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "palette opens"
    );
    cx.simulate_input("tgres");
    cx.simulate_keystrokes("down up enter");
    cx.run_until_parked();
    assert!(
        !cx.update(|window, cx| window.has_active_dialog(cx)),
        "palette closes"
    );
    assert!(
        !dock_state(&ws, DockPlacement::Bottom, &mut cx).0,
        "the action ran"
    );

    // Reopened with an empty query, the command just run is first.
    cx.simulate_keystrokes("ctrl-shift-p");
    let first = cx.update(|_, cx| {
        let recent = cx.global::<ui_workspace::palette::RecentCommands>();
        recent.0.front().cloned()
    });
    assert_eq!(first.as_deref(), Some("workspace::ToggleResultsDock"));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}
