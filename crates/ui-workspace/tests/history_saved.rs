//! Query history and saved queries in the app. Scenario names map to
//! `openspec/changes/query-history-saved/specs/{query-history,saved-queries}/spec.md`; storage-level
//! scenarios are in `crates/storage/tests/history.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::secrets::{MemoryStore, SecretStore};
use storage::{HistoryEntry, Outcome, Storage};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::{EditorPanel, EditorTabs};
use ui_workspace::run::Runner;
use ui_workspace::saved::{SavedLibrary, TabSettings};
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};
use uuid::Uuid;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

struct Env {
    ws: Entity<Workspace>,
    cx: VisualTestContext,
    storage: Storage,
}

fn setup(cx: &mut TestAppContext) -> Env {
    cx.executor().allow_parking();
    let storage = Storage::open_in_memory().unwrap();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let db = storage.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(db),
                theme_override: None,
                secrets: Some(secrets),
            },
            cx,
        );
    });
    let (handle, ws): (AnyWindowHandle, Entity<Workspace>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(WorkspaceOptions::default(), window, cx))
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1400.0), px(1000.0)));
    Env {
        ws,
        cx: vcx,
        storage,
    }
}

fn connection(env: &mut Env, name: &str) -> Uuid {
    let config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig::default()),
    );
    let manager = env.cx.update(|_, cx| ConnectionManager::global(cx));
    let task = env
        .cx
        .update(|_, cx| manager.update(cx, |m, cx| m.save(config, PasswordChange::Clear, cx)));
    env.cx
        .foreground_executor()
        .block_test(task)
        .unwrap()
        .config
        .id
}

fn wait(env: &mut Env, what: &str, mut cond: impl FnMut(&mut Env) -> bool) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if cond(env) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn record(env: &Env, text: &str, conn: Uuid, at: i64) {
    env.storage
        .record_history_blocking(
            HistoryEntry {
                id: 0,
                connection_id: Some(conn.to_string()),
                connection_name: "Local".into(),
                text: text.into(),
                started_at: at,
                duration_ms: 3,
                outcome: Outcome::Success,
                count: 1,
                error: None,
            },
            100,
        )
        .unwrap();
}

fn new_tab(env: &mut Env, conn: Uuid, text: &str) -> Entity<EditorPanel> {
    let ws = env.ws.clone();
    let tab = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.open_query(text, Some(conn), window, cx)));
    env.cx.update(|window, cx| {
        let state = tab.read(cx).editor().read(cx).state().clone();
        state.update(cx, |s, cx| s.focus(window, cx));
    });
    env.cx.run_until_parked();
    tab
}

#[gpui_kit::test]
fn scenario_query_history_search_history(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env, "Local");
    record(&env, "g.V().out('route').count()", conn, 1);
    record(&env, "g.V().count()", conn, 2);
    record(&env, "g.E().hasLabel('route').limit(5)", conn, 3);
    let ws = env.ws.clone();
    let panel = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.show_history(window, cx)));
    // Ctrl+Shift+H focuses its search: type into it.
    env.cx.update(|window, cx| window.input("route", cx));
    wait(&mut env, "search", |env| {
        env.cx.update(|_, cx| panel.read(cx).entries().len() == 2)
    });
    let texts: Vec<String> = env.cx.update(|_, cx| {
        panel
            .read(cx)
            .entries()
            .iter()
            .map(|e| e.text.clone())
            .collect()
    });
    assert_eq!(
        texts,
        [
            "g.E().hasLabel('route').limit(5)",
            "g.V().out('route').count()"
        ]
    );
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("history-panel")
            .try_find("history-row-3")
            .is_some()
    });
    assert!(shown);
}

#[gpui_kit::test]
fn scenario_query_history_re_run_from_history(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env, "Local");
    record(&env, "g.V().limit(3)", conn, 1);
    let ws = env.ws.clone();
    let panel = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.show_history(window, cx)));
    wait(&mut env, "entries", |env| {
        env.cx.update(|_, cx| panel.read(cx).entries().len() == 1)
    });
    let tabs_before = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs().len());
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.hover("history-row-1", cx);
    });
    support::click_settled(&mut env.cx, "history-run-1");
    env.cx.run_until_parked();
    let tabs = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs());
    assert_eq!(tabs.len(), tabs_before + 1, "a new tab");
    let (text, tab_conn) = env.cx.update(|_, cx| {
        let t = tabs.last().unwrap().read(cx);
        (t.editor().read(cx).text(cx), t.connection_id())
    });
    assert_eq!(text, "g.V().limit(3)");
    assert_eq!(tab_conn, Some(conn), "bound to the entry's connection");
    let ran = env.cx.update(|_, cx| {
        Runner::global(cx)
            .read(cx)
            .latest()
            .map(|r| r.read(cx).query.clone())
    });
    assert_eq!(ran.as_deref(), Some("g.V().limit(3)"), "and it ran");
}

#[gpui_kit::test]
fn scenario_saved_queries_save_and_reopen(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env, "Local");
    let tab = new_tab(&mut env, conn, "g.V().has('airport','region','US-TX')");
    env.cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            t.set_settings(
                TabSettings {
                    timeout_ms: Some(10_000),
                    ..Default::default()
                },
                cx,
            )
        })
    });
    env.cx.simulate_keystrokes("ctrl-s");
    env.cx.run_until_parked();
    let form = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("save-query-form").is_some()
    });
    assert!(form, "asks for a name");
    env.cx.simulate_keystrokes("ctrl-a");
    env.cx
        .update(|window, cx| window.input("Texas airports", cx));
    support::click_settled(&mut env.cx, "save-query-ok");
    env.cx.run_until_parked();
    let id = env
        .cx
        .update(|_, cx| tab.read(cx).saved_query_id().map(str::to_owned))
        .unwrap();
    // Persisted with its settings.
    wait(&mut env, "persisted", |env| {
        env.storage
            .list_saved_blocking()
            .unwrap()
            .iter()
            .any(|q| q.id == id)
    });
    let row = env
        .storage
        .list_saved_blocking()
        .unwrap()
        .into_iter()
        .find(|q| q.id == id)
        .unwrap();
    assert_eq!(row.name, "Texas airports");
    assert!(row.settings.contains("10000"), "{}", row.settings);
    // Close the tab, then reopen from the Saved sidebar.
    let ws = env.ws.clone();
    env.cx.update(|window, cx| {
        ws.update(cx, |ws, cx| ws.show_saved(window, cx));
    });
    // Close the tab, then reopen the query: a fresh tab, as after a restart.
    env.cx.update(|window, cx| {
        let dock = ws.read(cx).dock_area().clone();
        dock.update(cx, |d, cx| d.remove_panel(tab.clone(), window, cx));
    });
    env.cx.run_until_parked();
    let reopened = env.cx.update(|window, cx| {
        let row = SavedLibrary::global(cx)
            .read(cx)
            .query(&id)
            .cloned()
            .unwrap();
        ws.update(cx, |ws, cx| ws.open_saved(&row, window, cx))
    });
    assert_ne!(reopened, tab, "a new tab");
    env.cx.run_until_parked();
    let (text, timeout, title) = env.cx.update(|_, cx| {
        let t = reopened.read(cx);
        (
            t.editor().read(cx).text(cx),
            t.settings().timeout_ms,
            t.title().to_string(),
        )
    });
    assert_eq!(text, "g.V().has('airport','region','US-TX')");
    assert_eq!(timeout, Some(10_000), "with its 10-second timeout");
    assert_eq!(title, "Texas airports");
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("saved-panel")
            .try_find(gpui_kit::SharedString::from(format!("saved-q-{id}")))
            .is_some()
    });
    assert!(shown, "listed in the Saved sidebar");
}

#[gpui_kit::test]
fn scenario_saved_queries_move_into_folder(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let (folder, query) = env.cx.update(|_, cx| {
        SavedLibrary::global(cx).update(cx, |l, cx| {
            let folder = l.create_folder("Airports", None, cx);
            let query = l.save_new(
                "Texas airports",
                None,
                None,
                "g.V()",
                &TabSettings::default(),
                cx,
            );
            (folder, query)
        })
    });
    let ws = env.ws.clone();
    env.cx.update(|window, cx| {
        ws.update(cx, |ws, cx| ws.show_saved(window, cx));
    });
    env.cx.run_until_parked();
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.drag_to(
            gpui_kit::SharedString::from(format!("saved-q-{query}")),
            gpui_kit::SharedString::from(format!("saved-folder-{folder}")),
            cx,
        );
    });
    env.cx.run_until_parked();
    let in_folder = env.cx.update(|_, cx| {
        SavedLibrary::global(cx)
            .read(cx)
            .query(&query)
            .unwrap()
            .folder_id
            .clone()
    });
    assert_eq!(
        in_folder.as_deref(),
        Some(folder.as_str()),
        "appears under Airports"
    );
    wait(&mut env, "persisted", |env| {
        env.storage
            .list_saved_blocking()
            .unwrap()
            .iter()
            .any(|q| q.id == query && q.folder_id.as_deref() == Some(folder.as_str()))
    });
}

#[gpui_kit::test]
fn scenario_saved_queries_dirty_indicator(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env, "Local");
    let id = env.cx.update(|_, cx| {
        SavedLibrary::global(cx).update(cx, |l, cx| {
            l.save_new(
                "Routes",
                None,
                Some(conn),
                "g.E()",
                &TabSettings::default(),
                cx,
            )
        })
    });
    let ws = env.ws.clone();
    let tab = env.cx.update(|window, cx| {
        let row = SavedLibrary::global(cx)
            .read(cx)
            .query(&id)
            .cloned()
            .unwrap();
        ws.update(cx, |ws, cx| ws.open_saved(&row, window, cx))
    });
    env.cx.run_until_parked();
    let tab_id = env.cx.update(|_, cx| tab.read(cx).id());
    assert!(!env.cx.update(|_, cx| tab.read(cx).is_dirty()));
    env.cx.update(|window, cx| {
        let state = tab.read(cx).editor().read(cx).state().clone();
        state.update(cx, |s, cx| s.focus(window, cx));
        window.input(".limit(3)", cx);
    });
    env.cx.run_until_parked();
    let marked = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::SharedString::from(format!("tab-dirty-{tab_id}")))
            .is_some()
    });
    assert!(env.cx.update(|_, cx| tab.read(cx).is_dirty()));
    assert!(marked, "the tab shows an unsaved indicator");
    env.cx.simulate_keystrokes("ctrl-s");
    env.cx.run_until_parked();
    assert!(!env.cx.update(|_, cx| tab.read(cx).is_dirty()), "saved");
    let text = env.cx.update(|_, cx| {
        SavedLibrary::global(cx)
            .read(cx)
            .query(&id)
            .unwrap()
            .text
            .clone()
    });
    assert!(
        text.contains(".limit(3)"),
        "Ctrl+S updated the saved query: {text}"
    );
    let dialog = env
        .cx
        .update(|window, _| window.try_find("save-query-form").is_some());
    assert!(!dialog, "no dialog for a tab linked to a saved query");
}
