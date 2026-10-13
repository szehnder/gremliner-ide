//! Headless UI tests for editor tabs. Scenario names map to
//! `openspec/changes/query-editor/specs/query-editor/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::Instant;

use gpui_kit::component::dock::DockPlacement;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use providers::{
    ConnectionConfig, Environment, GremlinServerConfig, PasswordChange, ProviderConfig,
};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::{EditorPanel, EditorTabs};
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
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

fn open(cx: &mut TestAppContext) -> (Entity<Workspace>, VisualTestContext) {
    let (handle, ws): (AnyWindowHandle, Entity<Workspace>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(WorkspaceOptions::default(), window, cx))
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1400.0), px(900.0)));
    (ws, vcx)
}

fn setup(cx: &mut TestAppContext) -> (Storage, Entity<Workspace>, VisualTestContext) {
    let storage = Storage::open_in_memory().unwrap();
    init(cx, storage.clone());
    let (ws, vcx) = open(cx);
    (storage, ws, vcx)
}

fn new_tab(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorPanel> {
    let tab = cx.update(|window, cx| ws.update(cx, |ws, cx| ws.new_tab(window, cx)));
    cx.run_until_parked();
    tab
}

fn set_text(tab: &Entity<EditorPanel>, text: &str, cursor: usize, cx: &mut VisualTestContext) {
    let text = text.to_owned();
    cx.update(|window, cx| {
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.set_text(&text, window, cx));
        let state = editor.read(cx).state().clone();
        state.update(cx, |s, cx| {
            s.set_selected_range(cursor..cursor, cx);
            s.focus(window, cx);
        });
    });
    cx.run_until_parked();
}

fn text_of(tab: &Entity<EditorPanel>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| tab.read(cx).editor().read(cx).text(cx))
}

fn exists(id: String, cx: &mut VisualTestContext) -> bool {
    let id = gpui_kit::SharedString::from(id);
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|s| s.visible())
    })
}

fn last_run(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| {
        EditorTabs::global(cx)
            .read(cx)
            .last_run()
            .map(|r| r.text.clone())
    })
}

#[gpui_kit::test]
fn scenario_query_editor_show_an_error(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    // Set rather than typed: typing `(` auto-inserts the closing `)`.
    set_text(&tab, "g.V().has('name'", 16, &mut cx);
    let (diags, editor_diags) = cx.update(|_, cx| {
        let editor = tab.read(cx).editor().read(cx);
        let count = editor.state().read(cx).diagnostics().map_or(0, |d| d.len());
        (editor.diagnostics().to_vec(), count)
    });
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        diags[0].range.start,
        "g.V().has('name'".len(),
        "at the end of the text"
    );
    assert_eq!(editor_diags, 1, "underlined in the editor");
    assert!(
        exists("problem-0".into(), &mut cx),
        "listed in the problems panel"
    );
    assert!(!exists("problem-1".into(), &mut cx));
}

#[gpui_kit::test]
fn scenario_query_editor_jump_to_problem(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    set_text(&tab, "g.V().outt('x')\ng.V()", 0, &mut cx);
    cx.update(|window, cx| window.click("problem-0", cx));
    cx.run_until_parked();
    let cursor = cx.update(|_, cx| tab.read(cx).editor().read(cx).cursor(cx));
    assert_eq!(cursor, "g.V().".len(), "cursor at the start of `outt`");
}

#[gpui_kit::test]
fn scenario_query_editor_format_document(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    let long = "g.V().hasLabel('airport').has('country','US').out('route').has('region','US-TX').values('code').dedup()";
    set_text(&tab, long, 0, &mut cx);
    cx.simulate_keystrokes("shift-alt-f");
    let formatted = text_of(&tab, &mut cx);
    assert!(formatted.lines().count() > 3, "{formatted}");
    assert!(formatted.starts_with("g.V().\n  hasLabel('airport')."));
    cx.simulate_keystrokes("ctrl-z");
    assert_eq!(
        text_of(&tab, &mut cx),
        long,
        "one undo restores the original"
    );
}

#[gpui_kit::test]
fn toggle_comment_with_ctrl_slash(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    set_text(&tab, "g.V()\ng.E()", 7, &mut cx);
    cx.simulate_keystrokes("ctrl-/");
    assert_eq!(text_of(&tab, &mut cx), "g.V()\n// g.E()");
    // The global shortcuts dialog did not open.
    assert!(!cx.update(gpui_kit::component::WindowExt::has_active_dialog));
}

#[gpui_kit::test]
fn scenario_query_editor_run_second_statement(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    let text = "g.V().count()\n\ng.E().count()";
    set_text(&tab, text, text.len() - 3, &mut cx);
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(last_run(&mut cx).as_deref(), Some("g.E().count()"));
    assert_eq!(
        text_of(&tab, &mut cx),
        text,
        "Ctrl+Enter does not insert a newline"
    );
    // The statement that runs is highlighted.
    let highlighted = cx.update(|_, cx| tab.read(cx).editor().read(cx).highlighted_statement());
    assert_eq!(highlighted, Some(15..text.len()));
    // Alt+Enter runs too.
    set_text(&tab, text, 2, &mut cx);
    cx.simulate_keystrokes("alt-enter");
    assert_eq!(last_run(&mut cx).as_deref(), Some("g.V().count()"));
}

#[gpui_kit::test]
fn scenario_query_editor_run_selection(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    let text = "g.V().limit(3).values('name')";
    set_text(&tab, text, 0, &mut cx);
    cx.update(|_, cx| {
        let state = tab.read(cx).editor().read(cx).state().clone();
        state.update(cx, |s, cx| s.set_selected_range(0..14, cx));
    });
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(last_run(&mut cx).as_deref(), Some("g.V().limit(3)"));
}

fn save_connection(name: &str, env: Environment, cx: &mut VisualTestContext) -> ConnectionConfig {
    let mut config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig::default()),
    );
    config.environment = env;
    let manager = cx.update(|_, cx| ConnectionManager::global(cx));
    let task =
        cx.update(|_, cx| manager.update(cx, |m, cx| m.save(config, PasswordChange::Clear, cx)));
    cx.foreground_executor().block_test(task).unwrap().config
}

#[gpui_kit::test]
fn scenario_query_editor_new_tab_defaults(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let local = save_connection("Local", Environment::Local, &mut cx);
    let first = new_tab(&ws, &mut cx);
    cx.update(|_, cx| first.update(cx, |t, cx| t.set_connection(Some(local.id), cx)));
    cx.simulate_keystrokes("ctrl-t");
    let tabs = cx.update(|_, cx| EditorTabs::global(cx).read(cx).tabs());
    assert_eq!(tabs.len(), 2);
    assert_eq!(
        cx.update(|_, cx| tabs[1].read(cx).connection_id()),
        Some(local.id)
    );
}

#[gpui_kit::test]
fn scenario_query_editor_split_editor(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let left = new_tab(&ws, &mut cx);
    let right = new_tab(&ws, &mut cx);
    let group = |tab: &Entity<EditorPanel>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let id = gpui_kit::component::dock::PanelId::from(tab.entity_id());
            ws.read(cx)
                .dock_area()
                .read(cx)
                .layout(DockPlacement::Center)
                .and_then(|t| t.find_panel_node(id))
        })
    };
    assert_eq!(
        group(&left, &mut cx),
        group(&right, &mut cx),
        "one column before"
    );
    cx.dispatch_action(ui_workspace::actions::SplitRight);
    cx.run_until_parked();
    assert_ne!(
        group(&left, &mut cx),
        group(&right, &mut cx),
        "two columns after"
    );
    let x = |tab: &Entity<EditorPanel>, cx: &mut VisualTestContext| {
        let id = gpui_kit::SharedString::from(format!(
            "editor-tab-{}",
            cx.update(|_, cx| tab.read(cx).id())
        ));
        cx.update(move |window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().origin.x
        })
    };
    assert!(
        x(&right, &mut cx) > x(&left, &mut cx),
        "the tab moved to the right column"
    );
}

#[gpui_kit::test]
fn dirty_marker_tracks_unsaved_changes(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    let id = cx.update(|_, cx| tab.read(cx).id());
    assert!(!exists(format!("tab-dirty-{id}"), &mut cx));
    cx.update(|window, cx| window.input("g.V()", cx));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| tab.read(cx).is_dirty()));
    // Ctrl+S asks for a name (the tab title by default), then saves into the library.
    cx.simulate_keystrokes("ctrl-s");
    cx.run_until_parked();
    support::click_settled(&mut cx, "save-query-ok");
    assert!(!cx.update(|_, cx| tab.read(cx).is_dirty()));
}

#[gpui_kit::test]
fn scenario_query_editor_restore_tabs(cx: &mut TestAppContext) {
    let (storage, ws, mut vcx) = setup(cx);
    let local = save_connection("Local", Environment::Local, &mut vcx);
    let prod = save_connection("Prod", Environment::Production, &mut vcx);
    let specs = [
        ("g.V().count()", 3, Some(local.id)),
        ("g.E().limit(5)", 7, Some(prod.id)),
        ("g.V().hasLabel('x')", 0, None),
    ];
    for (text, cursor, conn) in specs {
        let tab = new_tab(&ws, &mut vcx);
        set_text(&tab, text, cursor, &mut vcx);
        vcx.update(|_, cx| tab.update(cx, |t, cx| t.set_connection(conn, cx)));
    }
    vcx.update(|window, cx| ws.update(cx, |ws, cx| ws.save_now(window, cx)));
    vcx.run_until_parked();
    storage.flush_blocking().unwrap();

    // "Relaunch": a new workspace over the same database restores the tabs from the saved layout.
    let (ws2, mut vcx2) = open(cx);
    let restored: Vec<(String, usize, Option<uuid::Uuid>)> = vcx2.update(|_, cx| {
        let dock = ws2.read(cx).dock_area().read(cx);
        dock.layout(DockPlacement::Center)
            .unwrap()
            .panels()
            .filter_map(|id| {
                dock.panel(id)
                    .and_then(|p| p.view().downcast::<EditorPanel>().ok())
            })
            .map(|tab| {
                let tab = tab.read(cx);
                let editor = tab.editor().read(cx);
                (editor.text(cx), editor.cursor(cx), tab.connection_id())
            })
            .collect()
    });
    let expected: Vec<_> = specs
        .iter()
        .map(|(t, c, conn)| ((*t).to_owned(), *c, *conn))
        .collect();
    assert_eq!(restored, expected);
}

#[gpui_kit::test]
fn production_badge_on_editor_tabs(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let prod = save_connection("Prod", Environment::Production, &mut cx);
    let tab = new_tab(&ws, &mut cx);
    cx.update(|_, cx| tab.update(cx, |t, cx| t.set_connection(Some(prod.id), cx)));
    cx.run_until_parked();
    let id = cx.update(|_, cx| tab.read(cx).id());
    let scope = gpui_kit::SharedString::from(format!("tab-title-{id}"));
    let found = cx.update(move |window, cx| {
        window.render_frame(cx);
        window
            .within(scope)
            .try_find("env-badge-production")
            .is_some()
    });
    assert!(found, "the tab shows the production badge");
}

#[gpui_kit::test]
fn scenario_query_editor_large_file_typing(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let tab = new_tab(&ws, &mut cx);
    let text: String = (0..2500)
        .map(|i| format!("g.V().hasLabel('airport').\n  has('code','A{i}').\n  out('route').\n  values('city')\n"))
        .collect();
    assert!(text.lines().count() >= 10_000);
    set_text(&tab, &text, text.len() / 2, &mut cx);
    let mut times = Vec::new();
    for _ in 0..40 {
        let started = Instant::now();
        // One keystroke, its effects, one frame. (`window.input` also draws a frame before and after
        // each character, which would count three frames per key.)
        cx.update(|window, cx| {
            let mut key = gpui_kit::Keystroke::parse("x").unwrap();
            key.key_char = Some("x".into());
            window.dispatch_keystroke(key, cx);
        });
        cx.update(|window, cx| window.render_frame(cx));
        times.push(started.elapsed());
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    // Spec: < 16 ms keystroke-to-frame. Test builds are partly unoptimised and the test platform lays out
    // without a GPU, so this checks the budget with 2x headroom in debug builds.
    let budget_ms = if cfg!(debug_assertions) { 32 } else { 16 };
    eprintln!("10k-line keystroke p95: {p95:?}");
    println!(
        "measure editor_typing_10k_lines={:.3} ms",
        p95.as_secs_f64() * 1000.0
    );
    assert!(p95.as_millis() < budget_ms, "p95 {p95:?}");
}

#[gpui_kit::test]
fn new_tab_numbers_continue_after_restored_tabs(cx: &mut TestAppContext) {
    let (_s, ws, mut cx) = setup(cx);
    let restored = cx.update(|window, cx| {
        EditorPanel::from_row(
            storage::TabRow {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Query 4".into(),
                content: String::new(),
                cursor: 0,
                connection_id: None,
                saved_query_id: None,
                settings: None,
            },
            window,
            cx,
        )
    });
    let _ = restored;
    let tab = new_tab(&ws, &mut cx);
    assert_eq!(cx.update(|_, cx| tab.read(cx).title_text()), "Query 5");
}
