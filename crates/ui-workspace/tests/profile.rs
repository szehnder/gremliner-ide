//! Profile and explain in the app: the Profile view marks the bottleneck, selecting a step highlights its
//! source, and profiling a mutation asks first. Scenario names map to
//! `openspec/changes/query-profile-explain/specs/query-profile-explain/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use gremlin_model::profile::{ProfileReport, StepMetric};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_results::{Analysis, ResultSet, ResultsView, RunStatus, ViewKind};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorPanel;
use ui_workspace::run::Runner;
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
    vcx.simulate_resize(size(px(1400.0), px(900.0)));
    Env { ws, cx: vcx }
}

fn connection(env: &mut Env, alias: &str) -> Uuid {
    let mut config = ConnectionConfig::new(
        "Modern",
        ProviderConfig::GremlinServer(GremlinServerConfig {
            alias: alias.into(),
            ..Default::default()
        }),
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

fn results_view(env: &mut Env) -> Entity<ResultsView> {
    let ws = env.ws.clone();
    env.cx.update(|_, cx| {
        let dock = ws.read(cx).dock_area().read(cx);
        dock.layout(gpui_kit::component::dock::DockPlacement::Bottom)
            .unwrap()
            .panels()
            .find_map(|id| {
                dock.panel(id).and_then(|p| {
                    p.view()
                        .downcast::<ui_workspace::panels::ResultsPanel>()
                        .ok()
                })
            })
            .unwrap()
            .read(cx)
            .view()
            .clone()
    })
}

fn step(name: &str, pct: f64) -> StepMetric {
    StepMetric {
        name: name.into(),
        count: Some(3),
        traversers: Some(3),
        dur_ms: pct / 10.0,
        pct,
        children: Vec::new(),
        annotations: Vec::new(),
    }
}

#[gpui_kit::test]
fn scenario_query_profile_explain_slowest_step_marked(cx: &mut TestAppContext) {
    let _env = setup(cx);
    let report = ProfileReport {
        total_ms: 10.0,
        steps: vec![
            step("TinkerGraphStep(vertex,[])", 20.0),
            step("VertexStep(OUT,vertex)", 70.0),
            step("PathStep", 10.0),
        ],
        optimized: None,
        sections: Vec::new(),
    };
    // The results view on its own, as the results dock hosts it.
    let (handle, view): (AnyWindowHandle, Entity<ResultsView>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| ResultsView::new())
            })
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1200.0), px(600.0)));
    vcx.update(|window, cx| {
        let result = cx.new(|cx| {
            let mut r = ResultSet::new(1, "q", "q.profile()", "Modern", 100);
            r.set_analysis(
                Analysis::Profile {
                    report,
                    ranges: vec![None; 3],
                },
                cx,
            );
            r
        });
        view.update(cx, |v, cx| v.set_result(result, window, cx));
    });
    vcx.run_until_parked();
    assert_eq!(vcx.update(|_, cx| view.read(cx).view()), ViewKind::Profile);
    let marked = vcx.update(|window, cx| {
        window.render_frame(cx);
        (
            window
                .within("profile-row-1")
                .try_find("profile-bottleneck")
                .is_some(),
            window
                .within("profile-row-0")
                .try_find("profile-bottleneck")
                .is_some(),
        )
    });
    assert_eq!(marked, (true, false), "the 70% step is the bottleneck");
}

#[gpui_kit::test]
fn scenario_query_profile_explain_step_to_source(cx: &mut TestAppContext) {
    if !gremlin_client::testenv::is_up(&gremlin_client::testenv::url()) {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "gModern");
    let text = "g.V().out('knows').has('name','josh').values('age')";
    let tab = tab(&mut env, conn, text);
    env.cx.simulate_keystrokes("ctrl-shift-enter");
    let started = Instant::now();
    let result = loop {
        env.cx.run_until_parked();
        let latest = env
            .cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().cloned());
        if let Some(r) = latest
            && !env.cx.update(|_, cx| r.read(cx).is_running())
        {
            break r;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "profile finished"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let (status, analysis) = env.cx.update(|_, cx| {
        (
            result.read(cx).status.clone(),
            result.read(cx).analysis.clone(),
        )
    });
    assert_eq!(status, RunStatus::Succeeded);
    let Some(Analysis::Profile { report, ranges }) = analysis else {
        panic!("no profile")
    };
    let has_row = report
        .steps
        .iter()
        .position(|s| s.name.starts_with("HasStep"))
        .expect("a has step row");
    assert_eq!(
        &text[ranges[has_row].clone().unwrap()],
        "has('name','josh')"
    );
    // Clicking that row highlights the call in the editor.
    env.cx.run_until_parked();
    env.cx.update(|window, cx| window.render_frame(cx));
    support::click_settled(&mut env.cx, &format!("profile-row-{has_row}"));
    env.cx.run_until_parked();
    let hints = env
        .cx
        .update(|_, cx| tab.read(cx).editor().read(cx).hint_ranges().to_vec());
    let start = text.find("has(").unwrap();
    assert_eq!(hints, vec![start..start + "has('name','josh')".len()]);
    let view = results_view(&mut env);
    assert_eq!(
        env.cx.update(|_, cx| view.read(cx).view()),
        ViewKind::Profile
    );
}

#[gpui_kit::test]
fn scenario_query_profile_explain_confirm_profiling_a_mutation(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env, "gModern");
    let _tab = tab(&mut env, conn, "g.V().drop()");
    env.cx.simulate_keystrokes("ctrl-shift-enter");
    env.cx.run_until_parked();
    assert!(
        env.cx.update(|window, cx| window.has_active_dialog(cx)),
        "asks before profiling a mutation"
    );
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_none()),
        "nothing ran"
    );
    support::click_settled(&mut env.cx, "cancel");
    env.cx.run_until_parked();
    assert!(!env.cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_none()),
        "cancelling runs nothing"
    );
}
