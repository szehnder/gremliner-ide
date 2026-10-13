//! Query parameters in the app: detection, bindings, the missing-value prompt, saved defaults and
//! report tabs. Scenario names map to
//! `openspec/changes/query-parameters/specs/query-parameters/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use gremlin_model::GValue;
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_results::{ResultSet, RunStatus};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorPanel;
use ui_workspace::run::Runner;
use ui_workspace::saved::{ParamDef, SavedLibrary, TabSettings};
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

fn testenv_up() -> bool {
    let up = gremlin_client::testenv::is_up(&gremlin_client::testenv::url());
    if !up {
        eprintln!("SKIPPED: testenv not reachable");
    }
    up
}

fn connection(env: &mut Env) -> Uuid {
    let config = ConnectionConfig::new(
        "Air routes",
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

fn tab(env: &mut Env, conn: Uuid, text: &str) -> Entity<EditorPanel> {
    let ws = env.ws.clone();
    let tab = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.open_query(text, Some(conn), window, cx)));
    env.cx.update(|window, cx| {
        let state = tab.read(cx).editor().read(cx).state().clone();
        state.update(cx, |s, cx| {
            s.set_selected_range(0..0, cx);
            s.focus(window, cx)
        });
    });
    env.cx.run_until_parked();
    tab
}

fn wait_done(env: &mut Env) -> Entity<ResultSet> {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        let latest = env
            .cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().cloned());
        if let Some(r) = latest
            && !env.cx.update(|_, cx| r.read(cx).is_running())
        {
            return r;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "run finished");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn scenario_query_parameters_parameters_listed(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let tab = tab(
        &mut env,
        conn,
        "g.V().has('airport','code',code).out().limit(n)",
    );
    assert_eq!(
        env.cx
            .update(|_, cx| tab.read(cx).parameter_names().to_vec()),
        ["code", "n"]
    );
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        let w = window.within("params-panel");
        (
            w.try_find("param-row-code").is_some(),
            w.try_find("param-row-n").is_some(),
        )
    });
    assert_eq!(shown, (true, true));
    // Detection follows edits.
    env.cx
        .update(|window, cx| window.input(".has('runways', r)", cx));
    env.cx.run_until_parked();
    let names = env
        .cx
        .update(|_, cx| tab.read(cx).parameter_names().to_vec());
    assert!(names.contains(&"r".to_owned()), "{names:?}");
}

#[gpui_kit::test]
fn scenario_query_parameters_query_text_unchanged(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let text = "g.V().has('airport','code',code).values('city')";
    let tab = tab(&mut env, conn, text);
    env.cx.update(|window, cx| {
        tab.update(cx, |t, cx| t.set_param_value("code", "\"AUS\"", window, cx))
    });
    env.cx.simulate_keystrokes("ctrl-enter");
    let result = wait_done(&mut env);
    let (status, items, sent) = env.cx.update(|_, cx| {
        let r = result.read(cx);
        (r.status.clone(), r.items.clone(), r.sent.clone())
    });
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(items, vec![GValue::String("Austin".into())]);
    assert!(sent.starts_with(text), "the original text was sent: {sent}");
    assert!(!sent.contains("AUS"), "the value went as a binding: {sent}");
}

#[gpui_kit::test]
fn scenario_query_parameters_prompt_for_missing(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let tab = tab(&mut env, conn, "g.V().hasLabel('airport').limit(n)");
    env.cx.simulate_keystrokes("ctrl-enter");
    env.cx.run_until_parked();
    let prompted = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("param-prompt")
            .try_find("param-prompt-n")
            .is_some()
    });
    assert!(prompted, "asks for n");
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_none()),
        "nothing ran yet"
    );
    env.cx.update(|window, cx| window.input("2", cx));
    support::click_settled(&mut env.cx, "param-prompt-ok");
    env.cx.run_until_parked();
    let value = env
        .cx
        .update(|_, cx| tab.read(cx).settings().param("n").map(|p| p.value.clone()));
    assert_eq!(value.as_deref(), Some("2"));
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_some()),
        "ran once the value was given"
    );
    if testenv_up() {
        let result = wait_done(&mut env);
        assert_eq!(env.cx.update(|_, cx| result.read(cx).items.len()), 2);
    }
}

#[gpui_kit::test]
fn scenario_query_parameters_default_restored(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let tab = tab(&mut env, conn, "g.V().limit(n)");
    env.cx.update(|_, cx| {
        tab.update(cx, |t, cx| {
            let mut s = t.settings().clone();
            s.parameters = vec![ParamDef {
                name: "n".into(),
                default: "10".into(),
                label: "How many".into(),
                ..Default::default()
            }];
            t.set_settings(s, cx);
            t.save_as("First n", None, false, cx);
        })
    });
    let id = env
        .cx
        .update(|_, cx| tab.read(cx).saved_query_id().unwrap().to_owned());
    let ws = env.ws.clone();
    env.cx.update(|window, cx| {
        let dock = ws.read(cx).dock_area().clone();
        dock.update(cx, |d, cx| d.remove_panel(tab.clone(), window, cx));
    });
    let reopened = env.cx.update(|window, cx| {
        let row = SavedLibrary::global(cx)
            .read(cx)
            .query(&id)
            .cloned()
            .unwrap();
        ws.update(cx, |ws, cx| ws.open_saved(&row, window, cx))
    });
    env.cx.run_until_parked();
    let def = env
        .cx
        .update(|_, cx| reopened.read(cx).settings().param("n").cloned())
        .unwrap();
    assert_eq!(
        (def.default.as_str(), def.label.as_str()),
        ("10", "How many")
    );
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("params-panel")
            .try_find("param-default-n")
            .is_some()
    });
    assert!(shown, "the panel shows n with its default");
}

#[gpui_kit::test]
fn scenario_query_parameters_run_report(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let settings = TabSettings {
        parameters: vec![ParamDef {
            name: "code".into(),
            label: "Airport code".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let row = env.cx.update(|_, cx| {
        SavedLibrary::global(cx).update(cx, |l, cx| {
            let id = l.save_new(
                "Airport city",
                None,
                Some(conn),
                "g.V().has('airport','code',code).values('city')",
                &settings,
                cx,
            );
            l.query(&id).cloned().unwrap()
        })
    });
    let ws = env.ws.clone();
    let (a, b) = env.cx.update(|window, cx| {
        ws.update(cx, |ws, cx| {
            (
                ws.open_report(&row, window, cx),
                ws.open_report(&row, window, cx),
            )
        })
    });
    assert_ne!(a, b, "several reports of one query");
    env.cx.run_until_parked();
    assert_eq!(env.cx.update(|_, cx| b.read(cx).labels()), ["Airport code"]);
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("report-form")
            .try_find("report-field-code")
            .is_some()
    });
    assert!(shown, "an Airport code field");
    if !testenv_up() {
        return;
    }
    env.cx.update(|window, cx| {
        b.update(cx, |r, cx| {
            r.set_field("code", "\"AUS\"", window, cx);
            r.run(window, cx);
        })
    });
    wait_done(&mut env);
    env.cx.run_until_parked();
    let items = env.cx.update(|_, cx| {
        b.read(cx)
            .results()
            .read(cx)
            .result()
            .map(|r| r.read(cx).items.clone())
    });
    assert_eq!(
        items,
        Some(vec![GValue::String("Austin".into())]),
        "results below the form"
    );
}
