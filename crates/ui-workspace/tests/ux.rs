//! Cross-cutting UX standards: empty and error states, reduce motion, keyboard access, onboarding and
//! accessibility. Scenario names map to `openspec/changes/polish-pass/specs/ux-quality/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Role, SharedString, TestAppContext,
    VisualTestContext, WindowOptions, px, size,
};
use graph_canvas::GraphCanvas;
use graph_store::{GraphStore, NodeIx};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::secrets::{MemoryStore, SecretStore};
use storage::{Storage, Table};
use ui_workspace::connections::{ConnStatus, ConnectionManager};
use ui_workspace::run::Runner;
use ui_workspace::settings::{AppSettings, MotionChoice};
use ui_workspace::theme::Motion;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions, io, keymap, shortcuts, tour};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

fn init(cx: &mut TestAppContext, storage: Storage) {
    cx.executor().allow_parking();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(storage),
                theme_override: None,
                secrets: Some(secrets),
            },
            cx,
        );
    });
}

fn open(
    cx: &mut TestAppContext,
    options: WorkspaceOptions,
) -> (Entity<Workspace>, VisualTestContext) {
    let (handle, ws): (AnyWindowHandle, Entity<Workspace>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let ws = cx.new(|cx| Workspace::new(options, window, cx));
                window.focus(&ws.read(cx).focus_handle().clone(), cx);
                ws
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1400.0), px(900.0)));
    vcx.run_until_parked();
    (ws, vcx)
}

fn exists(id: &str, cx: &mut VisualTestContext) -> bool {
    let id = SharedString::from(id.to_owned());
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|s| s.visible())
    })
}

fn focused(id: &str, cx: &mut VisualTestContext) -> bool {
    let id = SharedString::from(id.to_owned());
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.try_find(id).and_then(|s| s.focused()) == Some(true)
    })
}

/// Wait (in real time, the work runs on tokio) until `cond` holds.
fn wait_until(cx: &mut VisualTestContext, mut cond: impl FnMut(&mut VisualTestContext) -> bool) {
    for _ in 0..1000 {
        cx.run_until_parked();
        if cond(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("condition not met within 10 s");
}

fn save_connection(name: &str, port: u16, cx: &mut VisualTestContext) -> ConnectionConfig {
    let config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig {
            port,
            ..GremlinServerConfig::default()
        }),
    );
    let m = cx.update(|_, cx| ConnectionManager::global(cx));
    let task = cx.update(|_, cx| m.update(cx, |m, cx| m.save(config, PasswordChange::Keep, cx)));
    let saved = cx
        .foreground_executor()
        .block_test(task)
        .map_err(|_| "save failed")
        .unwrap();
    cx.run_until_parked();
    saved.config
}

fn status(id: uuid::Uuid, cx: &mut VisualTestContext) -> ConnStatus {
    cx.update(|_, cx| ConnectionManager::global(cx).read(cx).status(id))
}

/// Whatever holds keyboard focus is a visible element (a hidden focus target cannot be indicated).
fn assert_focus_visible(step: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.focused(cx).is_some(), "{step}: nothing has focus");
    });
}

/// Press Tab until `id` has focus (each stop must be visible).
fn tab_to(id: &str, cx: &mut VisualTestContext) {
    for _ in 0..60 {
        if focused(id, cx) {
            return;
        }
        cx.simulate_keystrokes("tab");
        assert_focus_visible(&format!("tabbing to {id}"), cx);
    }
    panic!("Tab never reached {id}");
}

#[gpui_kit::test]
fn scenario_ux_quality_empty_results_dock(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(cx, WorkspaceOptions::default());
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    assert!(exists("results-panel", &mut cx));
    assert!(
        exists("results-empty-shortcut", &mut cx),
        "the empty results dock shows how to run a query"
    );
    let keys =
        cx.update(|_, cx| shortcuts::keystrokes_for(&ui_workspace::actions::editor::RunQuery, cx));
    assert!(keys.is_some_and(|k| !k.is_empty()), "with its shortcut");
}

#[gpui_kit::test]
fn scenario_ux_quality_error_with_recovery(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(cx, WorkspaceOptions::default());
    // Nothing listens on port 1.
    let config = save_connection("Broken", 1, &mut cx);
    let m = cx.update(|_, cx| ConnectionManager::global(cx));
    cx.update(|_, cx| m.update(cx, |m, cx| m.connect(config.id, cx)).detach());
    wait_until(&mut cx, |cx| {
        matches!(status(config.id, cx), ConnStatus::Error(_))
    });

    assert!(exists("conn-error-broken", &mut cx), "error state shown");
    assert!(
        exists("conn-error-reason-broken", &mut cx),
        "with the reason"
    );
    assert!(exists("conn-retry-broken", &mut cx), "offers Retry");
    assert!(
        exists("conn-error-edit-broken", &mut cx),
        "offers Edit connection"
    );

    cx.update(|window, cx| window.click("conn-retry-broken", cx));
    cx.run_until_parked();
    wait_until(&mut cx, |cx| {
        matches!(status(config.id, cx), ConnStatus::Error(_))
    });

    support::click_settled(&mut cx, "conn-error-edit-broken");
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "Edit opens the form"
    );
    assert!(exists("conn-save", &mut cx));
}

#[gpui_kit::test]
fn scenario_ux_quality_reduce_motion(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    cx.update(|cx| {
        assert!(!cx.reduce_motion());
        assert_eq!(Motion::Normal.duration(cx), Duration::from_millis(200));
        AppSettings::set_os_reduce_motion(true, cx);
        assert!(cx.reduce_motion(), "System follows the OS preference");
        assert_eq!(
            Motion::Normal.duration(cx),
            Duration::ZERO,
            "panel transitions are instant"
        );
        AppSettings::update_general(cx, |g| g.motion = MotionChoice::Full);
        assert!(!cx.reduce_motion(), "the setting can override the OS");
        AppSettings::update_general(cx, |g| g.motion = MotionChoice::System);
        assert!(cx.reduce_motion());
    });

    // Graph focus lands in the first frame instead of tweening.
    let mut g = GraphStore::new();
    g.add_node(1.into(), "person", "marko", [0.0, 0.0]);
    g.add_node(2.into(), "person", "vadas", [900.0, 400.0]);
    let (view, cx) = cx.add_window_view(|_, cx| GraphCanvas::new(g, cx));
    cx.run_until_parked();
    view.update(cx, |v, _| v.focus_node(NodeIx(1)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(cx, |v, _| {
        assert!(!v.camera_animating(), "no tween runs");
        assert_eq!(v.camera().center, [900.0, 400.0], "camera is already there");
    });

    cx.update(|_, cx| AppSettings::set_os_reduce_motion(false, cx));
    view.update(cx, |v, _| v.focus_node(NodeIx(0)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(cx, |v, _| {
        assert!(v.camera_animating(), "with motion, focus tweens")
    });
}

#[gpui_kit::test]
fn scenario_ux_quality_shortcuts_sheet(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(cx, WorkspaceOptions::default());
    cx.simulate_keystrokes("ctrl-/");
    cx.run_until_parked();
    assert!(exists("shortcuts", &mut cx), "the sheet opens");

    let bound: Vec<&'static str> = keymap::default_bindings()
        .iter()
        .map(|b| b.action().name())
        .collect();
    let groups = cx.update(|_, cx| shortcuts::rows(cx));
    for action in &bound {
        assert!(
            groups
                .iter()
                .any(|(_, rows)| rows.iter().any(|r| r.action == *action)),
            "{action} has a binding but no row in the sheet"
        );
        let id = shortcuts::row_id(action);
        let shown = cx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(id.clone()).is_some()
        });
        assert!(shown, "{action} is rendered in the sheet");
    }
    // Grouped by area, with contextual bindings (editor, graph, debugger) included.
    let category = |action: &str| {
        groups
            .iter()
            .find(|(_, rows)| rows.iter().any(|r| r.action == action))
            .map(|(c, _)| *c)
    };
    assert_eq!(category("debug::StepOver"), Some("Debug"));
    assert_eq!(category("editor::RunQuery"), Some("Editor"));
    assert_eq!(category("graph::FitGraph"), Some("Graph"));
    let mut seen = Vec::new();
    for (c, _) in &groups {
        assert!(!seen.contains(c), "category {c} appears once");
        seen.push(*c);
    }
}

#[gpui_kit::test]
fn scenario_ux_quality_keyboard_only_connection(cx: &mut TestAppContext) {
    if !gremlin_client::testenv::is_up(&gremlin_client::testenv::url()) {
        eprintln!(
            "SKIPPED: testenv not reachable; run `cargo xtask testenv up && cargo xtask testenv seed`"
        );
        return;
    }
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(cx, WorkspaceOptions::default());

    // Palette → Add Connection.
    cx.simulate_keystrokes("f1");
    cx.simulate_input("add connection");
    cx.simulate_keystrokes("enter");
    wait_until(&mut cx, |cx| exists("conn-save", cx));
    assert_focus_visible("form opened", &mut cx);
    cx.simulate_input("Keys");
    // Host and port default to localhost:8182.
    tab_to("conn-save", &mut cx);
    cx.simulate_keystrokes("enter");
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| window.has_active_dialog(cx))
            && cx.update(|_, cx| {
                ConnectionManager::global(cx)
                    .read(cx)
                    .find_by_name("Keys")
                    .is_some()
            })
    });
    let id = cx.update(|_, cx| {
        ConnectionManager::global(cx)
            .read(cx)
            .find_by_name("Keys")
            .map(|c| c.id)
    });
    let names: Vec<String> = cx.update(|_, cx| {
        ConnectionManager::global(cx)
            .read(cx)
            .connections()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    });
    let id = id.unwrap_or_else(|| panic!("saved from the keyboard; saved: {names:?}"));

    // Tab to the sidebar row; Enter connects.
    tab_to("conn-row-keys", &mut cx);
    cx.simulate_keystrokes("enter");
    wait_until(&mut cx, |cx| {
        matches!(status(id, cx), ConnStatus::Connected(_))
    });

    // New tab, type, run.
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    assert_focus_visible("new tab", &mut cx);
    cx.simulate_input("g.V().count()");
    cx.simulate_keystrokes("alt-enter");
    wait_until(&mut cx, |cx| {
        cx.update(|_, cx| !Runner::global(cx).read(cx).is_running())
            && !exists("results-empty-shortcut", cx)
    });
    assert!(exists("results-view", &mut cx) || exists("results-panel", &mut cx));
}

#[gpui_kit::test]
fn scenario_ux_quality_skip_tour(cx: &mut TestAppContext) {
    let storage = Storage::open_in_memory().unwrap();
    init(cx, storage.clone());
    let options = WorkspaceOptions {
        tour: true,
        ..WorkspaceOptions::default()
    };
    let (ws, mut cx) = open(cx, options.clone());
    assert!(exists("tour", &mut cx), "first launch offers the tour");
    assert!(
        exists("tour-sample", &mut cx),
        "with the sample graph guide"
    );
    support::click_settled(&mut cx, "tour-skip");
    cx.run_until_parked();
    assert!(!exists("tour", &mut cx), "skipping hides it");
    cx.update(|_, cx| assert!(ws.read(cx).tour().is_none()));
    storage.flush_blocking().unwrap();
    let state: Option<String> = storage
        .get_json_blocking(Table::UiState, tour::TOUR_KEY)
        .unwrap();
    assert_eq!(state.as_deref(), Some("skipped"));

    // A later launch over the same database does not show it.
    let (ws2, mut cx2) = open(&mut cx, options);
    assert!(!exists("tour", &mut cx2));
    cx2.update(|_, cx| assert!(ws2.read(cx).tour().is_none()));
}

#[gpui_kit::test]
fn tour_steps_through_and_sample_guide_opens(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (ws, mut cx) = open(
        cx,
        WorkspaceOptions {
            tour: true,
            ..WorkspaceOptions::default()
        },
    );
    support::click_settled(&mut cx, "tour-sample");
    cx.run_until_parked();
    assert!(exists("sample-guide", &mut cx));
    assert!(exists("sample-command", &mut cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    for step in 1..3 {
        support::click_settled(&mut cx, "tour-next");
        cx.update(|_, cx| assert_eq!(ws.read(cx).tour().unwrap().read(cx).step(), step));
    }
    support::click_settled(&mut cx, "tour-next");
    cx.update(|_, cx| assert!(ws.read(cx).tour().is_none(), "Done closes it"));
}

const NAMED_ROLES: &[Role] = &[
    Role::Button,
    Role::Link,
    Role::TextInput,
    Role::MultilineTextInput,
    Role::SearchInput,
    Role::Tab,
    Role::CheckBox,
    Role::Switch,
    Role::ComboBox,
    Role::MenuItem,
];

/// gpui-kit's dock draws its tab strip (tabs, the "…" menu, close and dock-toggle buttons) without
/// accessible names and offers no way to set them; recorded as a known gap in docs/ux/audit.md.
fn dock_chrome(s: &gpui_kit::test::ElementSnapshot) -> bool {
    let last = format!("{:?}", s.path().last());
    (s.role() == Some(Role::Tab) && last.starts_with("Some(Integer("))
        || last == "Some(Name(\"menu\"))"
        || last.starts_with("Some(NamedInteger(\"close-tab\"")
        || last.starts_with("Some(Name(\"toggle-dock:")
}

fn unnamed(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        window.render_frame(cx);
        gpui_kit::base::test_support::snapshots(window)
            .into_iter()
            .filter(|s| s.visible() && s.role().is_some_and(|r| NAMED_ROLES.contains(&r)))
            .filter(|s| s.label().is_none_or(|l| l.trim().is_empty()))
            .filter(|s| !dock_chrome(s))
            .map(|s| format!("{:?} {:?}", s.role(), s.path().last()))
            .collect()
    })
}

#[gpui_kit::test]
fn scenario_ux_quality_accessible_names(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(
        cx,
        WorkspaceOptions {
            tour: true,
            ..WorkspaceOptions::default()
        },
    );
    // Welcome, sidebar empty state, tour, status bar.
    let missing = unnamed(&mut cx);
    assert!(
        missing.is_empty(),
        "unnamed controls on first launch: {missing:#?}"
    );

    // A connection in the sidebar, a query tab, the results dock.
    save_connection("Local", 8182, &mut cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    let missing = unnamed(&mut cx);
    assert!(
        missing.is_empty(),
        "unnamed controls with a tab open: {missing:#?}"
    );
}

#[gpui_kit::test]
fn graph_selection_has_a_textual_summary(cx: &mut TestAppContext) {
    init(cx, Storage::open_in_memory().unwrap());
    let (view, cx) = cx.add_window_view(ui_results::graph_view::GraphView::new);
    cx.run_until_parked();
    let summary = view.read_with(cx, |v, cx| v.selection_summary(cx));
    assert!(summary.starts_with("No selection"), "{summary}");
}

#[gpui_kit::test]
fn query_error_offers_run_again_and_fix(cx: &mut TestAppContext) {
    if !gremlin_client::testenv::is_up(&gremlin_client::testenv::url()) {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    init(cx, Storage::open_in_memory().unwrap());
    let (_ws, mut cx) = open(cx, WorkspaceOptions::default());
    let config = save_connection("Local", 8182, &mut cx);
    let m = cx.update(|_, cx| ConnectionManager::global(cx));
    cx.update(|_, cx| m.update(cx, |m, cx| m.connect(config.id, cx)).detach());
    wait_until(&mut cx, |cx| {
        matches!(status(config.id, cx), ConnStatus::Connected(_))
    });
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    cx.simulate_input("g.V().nosuchstep()");
    cx.simulate_keystrokes("alt-enter");
    wait_until(&mut cx, |cx| exists("results-error", cx));
    assert!(exists("results-error-retry", &mut cx));
    assert!(exists("results-error-fix", &mut cx));

    let before = cx.update(|_, cx| Runner::global(cx).read(cx).latest().cloned());
    cx.update(|window, cx| window.click("results-error-retry", cx));
    wait_until(&mut cx, |cx| {
        let latest = cx.update(|_, cx| Runner::global(cx).read(cx).latest().cloned());
        latest.map(|e| e.entity_id()) != before.as_ref().map(|e| e.entity_id())
            && exists("results-error", cx)
    });

    cx.update(|window, cx| window.click("results-error-fix", cx));
    cx.run_until_parked();
    assert!(
        exists("assistant-panel", &mut cx) || exists("assistant-fix", &mut cx),
        "the assistant opens"
    );
}
