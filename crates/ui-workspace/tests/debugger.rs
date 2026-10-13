//! The debugger in the app: starting, refusing mutations, stepping with keys, step strip counts,
//! simulation and inspection on air-routes (skipped without the testenv). Scenario names map to
//! `openspec/changes/query-debugger/specs/query-debugger/spec.md`; the query-level scenarios are in
//! `crates/debugger/tests`.

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
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorPanel;
use ui_workspace::run::Runner;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};
use uuid::Uuid;

const AUS: &str = "g.V().has('code','AUS').out('route').has('country','US').values('code')";

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
    vcx.simulate_resize(size(px(1400.0), px(1000.0)));
    Env { ws, cx: vcx }
}

fn connection(env: &mut Env) -> Uuid {
    let mut config = ConnectionConfig::new(
        "Air routes",
        ProviderConfig::GremlinServer(GremlinServerConfig::default()),
    );
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

fn tab(env: &mut Env, connection: Uuid, text: &str) -> Entity<EditorPanel> {
    let ws = env.ws.clone();
    let tab = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.new_tab(window, cx)));
    let text = text.to_owned();
    env.cx.update(|window, cx| {
        tab.update(cx, |t, cx| t.set_connection(Some(connection), cx));
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.set_text(&text, window, cx));
        editor.read(cx).state().clone().update(cx, |s, cx| {
            s.set_selected_range(0..0, cx);
            s.focus(window, cx);
        });
    });
    env.cx.run_until_parked();
    tab
}

fn testenv_up() -> bool {
    let up = gremlin_client::testenv::is_up(&gremlin_client::testenv::url());
    if !up {
        eprintln!("SKIPPED: testenv not reachable");
    }
    up
}

/// Wait in real time for `cond`.
fn wait(env: &mut Env, what: &str, mut cond: impl FnMut(&mut Env) -> bool) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if cond(env) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn current_text(env: &mut Env, tab: &Entity<EditorPanel>) -> String {
    env.cx.update(|_, cx| {
        let view = tab.read(cx).debug_view().unwrap().read(cx);
        let s = view.session();
        s.call_text(s.current()).to_owned()
    })
}

#[gpui_kit::test]
fn debug_session_in_the_editor(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let tab = tab(&mut env, conn, AUS);
    env.cx.simulate_keystrokes("ctrl-f5");
    env.cx.run_until_parked();
    // Started at the first step: editor locked, V() highlighted.
    let (read_only, step) = env.cx.update(|_, cx| {
        let e = tab.read(cx).editor().read(cx);
        (e.is_read_only(), e.debug_step().cloned())
    });
    assert!(read_only);
    assert_eq!(step, Some(2..5), "V() is highlighted");
    // F10 twice reaches out('route'); its prefix runs into the results dock.
    env.cx.simulate_keystrokes("f10 f10");
    env.cx.run_until_parked();
    assert_eq!(current_text(&mut env, &tab), "out('route')");
    wait(&mut env, "results at out('route')", |env| {
        env.cx.update(|_, cx| {
            Runner::global(cx).read(cx).latest().is_some_and(|r| {
                let r = r.read(cx);
                !r.is_running() && r.sent.contains(".out('route').limit(")
            })
        })
    });
    // Counts arrive for every step: one airport after has('code','AUS').
    wait(&mut env, "counts", |env| {
        env.cx
            .update(|_, cx| tab.read(cx).debug_view().unwrap().read(cx).counts().len() == 5)
    });
    let has_count = env
        .cx
        .update(|_, cx| tab.read(cx).debug_view().unwrap().read(cx).counts()[&1].clone());
    assert_eq!(has_count, Ok(1));
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("debug-steps")
            .try_find("debug-step-2")
            .is_some()
    });
    assert!(shown, "the step strip shows the steps");
    // Simulation: LHR is listed and marked as failing; hide failed removes it.
    support::click_settled(&mut env.cx, "debug-simulate");
    wait(&mut env, "simulation", |env| {
        env.cx.update(|_, cx| {
            tab.read(cx)
                .debug_view()
                .unwrap()
                .read(cx)
                .simulation()
                .is_some_and(|s| !s.loading)
        })
    });
    let (lhr, visible_before) = env.cx.update(|_, cx| {
        let sim = tab
            .read(cx)
            .debug_view()
            .unwrap()
            .read(cx)
            .simulation()
            .unwrap()
            .clone();
        let lhr = sim
            .items
            .iter()
            .position(|i| i.label.starts_with("LHR"))
            .expect("LHR is a destination");
        assert_eq!(sim.items[lhr].survives, Some(false));
        assert!(sim.items.iter().any(|i| i.survives == Some(true)));
        (lhr, sim.visible().len())
    });
    env.cx.update(|_, cx| {
        let view = tab.read(cx).debug_view().unwrap().clone();
        view.update(cx, |v, cx| v.set_hide_failed(true, cx));
    });
    let visible_after = env.cx.update(|_, cx| {
        let view = tab.read(cx).debug_view().unwrap().read(cx);
        let sim = view.simulation().unwrap();
        assert!(sim.visible().iter().all(|(_, i)| i.survives == Some(true)));
        sim.visible().len()
    });
    assert!(visible_after < visible_before, "hide failed removes them");
    // Inspect LHR: filtered out at has('country','US').
    env.cx.update(|_, cx| {
        let view = tab.read(cx).debug_view().unwrap().clone();
        view.update(cx, |v, cx| v.inspect(lhr, cx));
    });
    wait(&mut env, "inspection", |env| {
        env.cx.update(|_, cx| {
            tab.read(cx)
                .debug_view()
                .unwrap()
                .read(cx)
                .inspection_summary()
                .is_some()
        })
    });
    let summary = env.cx.update(|_, cx| {
        tab.read(cx)
            .debug_view()
            .unwrap()
            .read(cx)
            .inspection_summary()
            .unwrap()
    });
    assert!(
        summary.ends_with("is filtered out at `has('country','US')`."),
        "{summary}"
    );
    // F9 steps back; Shift+F5 stops and unlocks the editor.
    env.cx.simulate_keystrokes("f9");
    env.cx.run_until_parked();
    assert_eq!(current_text(&mut env, &tab), "has('code','AUS')");
    env.cx.simulate_keystrokes("shift-f5");
    env.cx.run_until_parked();
    let (read_only, step, debugging) = env.cx.update(|_, cx| {
        let t = tab.read(cx);
        let e = t.editor().read(cx);
        (
            e.is_read_only(),
            e.debug_step().cloned(),
            t.debug_view().is_some(),
        )
    });
    assert!(!read_only && step.is_none() && !debugging);
}

#[gpui_kit::test]
fn mutating_queries_are_not_debugged(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let tab = tab(&mut env, conn, "g.V().drop()");
    env.cx.simulate_keystrokes("ctrl-f5");
    env.cx.run_until_parked();
    let (debugging, read_only) = env.cx.update(|_, cx| {
        let t = tab.read(cx);
        (t.debug_view().is_some(), t.editor().read(cx).is_read_only())
    });
    assert!(!debugging && !read_only, "the session did not start");
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_none()),
        "nothing ran"
    );
}
