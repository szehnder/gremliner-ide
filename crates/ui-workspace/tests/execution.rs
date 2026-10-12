//! End-to-end query execution against the Docker testenv (skipped with a message when it is down).
//! Scenario names map to `openspec/changes/query-execution/specs/query-execution/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_results::{ResultSet, RunStatus, ViewKind};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorPanel;
use ui_workspace::run::Runner;
use ui_workspace::settings::AppSettings;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};
use uuid::Uuid;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

fn testenv_up() -> bool {
    let up = gremlin_client::testenv::is_up(&gremlin_client::testenv::url());
    if !up {
        eprintln!(
            "SKIPPED: testenv not reachable; run `cargo xtask testenv up && cargo xtask testenv seed`"
        );
    }
    up
}

struct Env {
    ws: Entity<Workspace>,
    cx: VisualTestContext,
}

fn setup(cx: &mut TestAppContext) -> Env {
    cx.executor().allow_parking();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(Storage::open_in_memory().unwrap()),
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
    vcx.simulate_resize(size(px(1400.0), px(900.0)));
    Env { ws, cx: vcx }
}

fn connection(env: &mut Env, name: &str, alias: &str, read_only: bool) -> Uuid {
    let mut config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig {
            alias: alias.into(),
            ..Default::default()
        }),
    );
    config.read_only = read_only;
    config.timeout_ms = 15_000;
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

fn tab(env: &mut Env, connection: Uuid) -> Entity<EditorPanel> {
    let ws = env.ws.clone();
    let tab = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.new_tab(window, cx)));
    env.cx
        .update(|_, cx| tab.update(cx, |t, cx| t.set_connection(Some(connection), cx)));
    env.cx.run_until_parked();
    tab
}

fn type_and_run(env: &mut Env, tab: &Entity<EditorPanel>, text: &str) {
    let text = text.to_owned();
    env.cx.update(|window, cx| {
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.set_text(&text, window, cx));
        editor.read(cx).state().clone().update(cx, |s, cx| {
            s.set_selected_range(0..0, cx);
            s.focus(window, cx);
        });
    });
    env.cx.simulate_keystrokes("ctrl-enter");
}

fn latest(env: &mut Env) -> Option<Entity<ResultSet>> {
    env.cx
        .update(|_, cx| Runner::global(cx).read(cx).latest().cloned())
}

/// Wait (real time) until the latest run is no longer running.
fn wait_done(env: &mut Env, timeout: Duration) -> Entity<ResultSet> {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if let Some(result) = latest(env)
            && !env.cx.update(|_, cx| result.read(cx).is_running())
        {
            return result;
        }
        assert!(
            started.elapsed() < timeout,
            "query did not finish within {timeout:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn status_of(env: &mut Env, r: &Entity<ResultSet>) -> (RunStatus, usize, bool) {
    env.cx.update(|_, cx| {
        let r = r.read(cx);
        (r.status.clone(), r.items.len(), r.truncated)
    })
}

#[gpui_kit::test]
fn scenario_query_execution_summary_after_success(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    type_and_run(&mut env, &tab, "g.V()");
    let result = wait_done(&mut env, Duration::from_secs(10));
    let (status, count, _) = status_of(&mut env, &result);
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(count, 6);
    let sent = env.cx.update(|_, cx| result.read(cx).sent.clone());
    assert_eq!(sent, "g.V().toList()", "the implicit terminal was appended");
    let elapsed = env.cx.update(|_, cx| result.read(cx).elapsed);
    assert!(elapsed.is_some_and(|e| e > Duration::ZERO));
    let summary = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("results-count").is_some()
    });
    assert!(summary, "the summary bar is shown");
}

#[gpui_kit::test]
fn scenario_query_execution_elements_open_in_graph(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    type_and_run(&mut env, &tab, "g.V().hasLabel('person')");
    wait_done(&mut env, Duration::from_secs(10));
    env.cx.run_until_parked();
    let ws = env.ws.clone();
    let results = env.cx.update(|_, cx| {
        let dock = ws.read(cx).dock_area().read(cx);
        let panel = dock
            .layout(gpui_kit::component::dock::DockPlacement::Bottom)
            .unwrap()
            .panels()
            .find_map(|id| {
                dock.panel(id).and_then(|p| {
                    p.view()
                        .downcast::<ui_workspace::panels::ResultsPanel>()
                        .ok()
                })
            })
            .unwrap();
        panel.read(cx).view().clone()
    });
    let (view, nodes) = env.cx.update(|_, cx| {
        let v = results.read(cx);
        let nodes = v.graph().map(|g| g.read(cx).store(cx).node_count());
        (v.view(), nodes)
    });
    assert_eq!(view, ViewKind::Graph);
    assert_eq!(nodes, Some(4), "four person nodes");
    env.cx
        .update(|window, cx| results.update(cx, |v, cx| v.set_view(ViewKind::Table, window, cx)));
    let rows = env.cx.update(|_, cx| {
        results.read(cx).table().and_then(|t| {
            t.read(cx)
                .table()
                .map(|s| s.read(cx).delegate().order().len())
        })
    });
    assert_eq!(rows, Some(4), "four person rows");
}

#[gpui_kit::test]
fn scenario_query_execution_progressive_rendering(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    env.cx
        .update(|_, cx| AppSettings::update_general(cx, |g| g.result_limit = 100_000));
    let conn = connection(&mut env, "Air", "g", false);
    let tab = tab(&mut env, conn);
    let seen = Arc::new(Mutex::new(Vec::<(bool, usize)>::new()));
    type_and_run(&mut env, &tab, "g.E()");
    let result = latest(&mut env).expect("a run started");
    let record = seen.clone();
    let _sub = env.cx.update(|_, cx| {
        cx.observe(&result, move |r, cx| {
            let r = r.read(cx);
            record.lock().unwrap().push((r.is_running(), r.items.len()));
        })
    });
    let result = wait_done(&mut env, Duration::from_secs(60));
    let (status, total, _) = status_of(&mut env, &result);
    assert_eq!(status, RunStatus::Succeeded);
    assert!(total > 50_000, "air-routes has ~57k edges, got {total}");
    let seen = seen.lock().unwrap();
    let partial = seen
        .iter()
        .filter(|(running, n)| *running && *n > 0 && *n < total)
        .count();
    assert!(
        partial >= 2,
        "rows appeared before completion and the count grew: {seen:?}"
    );
}

#[gpui_kit::test]
fn scenario_query_execution_cancel_a_long_query(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    env.cx
        .update(|_, cx| AppSettings::update_general(cx, |g| g.result_limit = 1_000_000));
    let conn = connection(&mut env, "Air", "g", false);
    let tab = tab(&mut env, conn);
    // A long but bounded stream: Gremlin Server 3.7 keeps evaluating a script after the client stops
    // reading, so an unbounded traversal would load the shared test server for other tests.
    type_and_run(&mut env, &tab, "g.V().both().both().limit(300000)");
    let result = latest(&mut env).unwrap();
    // Wait for the first rows.
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if env.cx.update(|_, cx| !result.read(cx).items.is_empty()) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "no rows arrived"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let pressed = Instant::now();
    env.cx.simulate_keystrokes("escape");
    let result = wait_done(&mut env, Duration::from_secs(5));
    let took = pressed.elapsed();
    let (status, count, _) = status_of(&mut env, &result);
    assert_eq!(status, RunStatus::Cancelled);
    assert!(count > 0, "partial results stay visible");
    assert!(took < Duration::from_millis(200), "stopped after {took:?}");
}

#[gpui_kit::test]
fn scenario_query_execution_truncated_result(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    env.cx
        .update(|_, cx| AppSettings::update_general(cx, |g| g.result_limit = 100));
    let conn = connection(&mut env, "Air", "g", false);
    let tab = tab(&mut env, conn);
    type_and_run(&mut env, &tab, "g.V()");
    let result = wait_done(&mut env, Duration::from_secs(20));
    let (status, count, truncated) = status_of(&mut env, &result);
    assert_eq!(
        (status, count, truncated),
        (RunStatus::Succeeded, 100, true)
    );
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("results-truncated").is_some()
    });
    assert!(shown, "the summary indicates truncation");
}

#[gpui_kit::test]
fn scenario_query_execution_server_error_view(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    type_and_run(&mut env, &tab, "g.V()");
    wait_done(&mut env, Duration::from_secs(10));
    type_and_run(&mut env, &tab, "g.V().nosuchstep()");
    let result = wait_done(&mut env, Duration::from_secs(10));
    let (status, count, _) = status_of(&mut env, &result);
    let RunStatus::Failed(error) = status else {
        panic!("expected an error, got {status:?}");
    };
    assert!(error.message.contains("nosuchstep"), "{}", error.message);
    assert_eq!(error.code, Some(597));
    assert_eq!(count, 0, "no stale results from the previous run");
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("results-error").is_some()
    });
    assert!(shown, "the error view is shown");
}

#[gpui_kit::test]
fn read_only_connection_asks_before_running_a_mutation(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern RO", "gModern", true);
    let tab = tab(&mut env, conn);
    // A mutation that changes nothing: drop() of zero vertices.
    type_and_run(&mut env, &tab, "g.V().limit(0).drop()");
    env.cx.run_until_parked();
    assert!(
        env.cx.update(|window, cx| window.has_active_dialog(cx)),
        "blocked with an explanation"
    );
    assert!(latest(&mut env).is_none(), "nothing was sent");
    env.cx.update(|window, cx| window.click("ok", cx));
    let result = wait_done(&mut env, Duration::from_secs(10));
    let (status, _, _) = status_of(&mut env, &result);
    assert_eq!(status, RunStatus::Succeeded, "Run anyway ran it once");
}
