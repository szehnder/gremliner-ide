//! Inspecting and editing graph data end to end against the Docker testenv (skipped when it is down).
//! Each test seeds its own tagged copy of a small "modern" graph into the writable `gSynth` graph.
//! Scenario names map to `openspec/changes/graph-inspector-editing/specs/graph-editing/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window,
    WindowOptions, div, px, size,
};
use graph_store::NodeIx;
use gremlin_model::edits::{Cardinality, EditCommand, ElementRef};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_results::graph_view::{GraphView, SelectedElement};
use ui_results::inspector::Inspector;
use ui_results::{ResultSet, RunStatus};
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

fn graph(env: &mut Env) -> Entity<GraphView> {
    env.cx
        .update(|_, cx| Runner::global(cx).read(cx).latest_graph().cloned())
        .expect("the tab has a graph")
}

/// Run `text` and wait until it and the graph's follow-up queries are done.
fn run(env: &mut Env, tab: &Entity<EditorPanel>, text: &str) -> Entity<GraphView> {
    type_and_run(env, tab, text);
    let result = wait_done(env, Duration::from_secs(20));
    let (status, _, _) = status_of(env, &result);
    assert_eq!(status, RunStatus::Succeeded, "{text}");
    let g = graph(env);
    settle(env, &g);
    g
}

/// Wait (real time) for the graph's follow-up queries and layout, then draw a frame.
fn settle(env: &mut Env, g: &Entity<GraphView>) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        env.cx.update(|window, cx| window.render_frame(cx));
        let busy = env.cx.update(|_, cx| {
            let g = g.read(cx);
            g.pending_fetches() > 0 || g.is_layout_running(cx)
        });
        if !busy {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "fetches did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    env.cx.update(|window, cx| window.render_frame(cx));
    env.cx.run_until_parked();
    env.cx.update(|window, cx| window.render_frame(cx));
}

fn find(env: &mut Env, g: &Entity<GraphView>, caption: &str) -> NodeIx {
    let caption = caption.to_owned();
    env.cx.update(move |_, cx| {
        let s = g.read(cx).store(cx);
        (0..s.node_count())
            .map(|i| NodeIx(i as u32))
            .find(|n| s.caption(*n).as_ref() == caption)
            .expect("node present")
    })
}

/// Seeds marko (age 29) with knows→vadas, knows→josh and created→lop, all tagged; returns the tag.
fn seed(env: &mut Env) -> String {
    let tag = Uuid::new_v4().to_string();
    let script = "m = g.addV('person').property('name','marko').property('age',29).property('tag',tag).next(); \
        v = g.addV('person').property('name','vadas').property('age',27).property('tag',tag).next(); \
        j = g.addV('person').property('name','josh').property('age',32).property('tag',tag).next(); \
        l = g.addV('software').property('name','lop').property('lang','java').property('tag',tag).next(); \
        g.V(m).addE('knows').to(v).iterate(); g.V(m).addE('knows').to(j).iterate(); \
        g.V(m).addE('created').to(l).iterate(); 'ok'";
    server(script, &tag);
    let _ = env;
    tag
}

/// Runs `script` on gSynth with `tag` bound.
fn server(script: &str, tag: &str) -> Vec<gremlin_model::GValue> {
    let script = script.to_owned();
    let tag = tag.to_owned();
    RUNTIME.block_on(async move {
        let client = gremlin_client::Client::connect(
            gremlin_client::ClientConfig::new(gremlin_client::testenv::url()).alias("gSynth"),
        )
        .await
        .unwrap();
        let req =
            gremlin_client::Request::new(script).binding("tag", gremlin_model::GValue::String(tag));
        client.execute_request(req).await.unwrap().items
    })
}

fn age_on_server(tag: &str) -> i64 {
    server(
        "g.V().has('tag', tag).has('name', 'marko').values('age')",
        tag,
    )
    .first()
    .and_then(gremlin_model::GValue::as_i64)
    .unwrap()
}

/// A graph of the seeded subgraph on a gSynth connection.
fn seeded_graph(env: &mut Env, production: bool) -> (Entity<GraphView>, String) {
    let tag = seed(env);
    let mut config = ConnectionConfig::new(
        if production { "Prod synth" } else { "Synth" },
        ProviderConfig::GremlinServer(GremlinServerConfig {
            alias: "gSynth".into(),
            ..Default::default()
        }),
    );
    config.timeout_ms = 15_000;
    if production {
        config.environment = providers::Environment::Production;
    }
    let manager = env.cx.update(|_, cx| ConnectionManager::global(cx));
    let task = env
        .cx
        .update(|_, cx| manager.update(cx, |m, cx| m.save(config, PasswordChange::Clear, cx)));
    let conn = env
        .cx
        .foreground_executor()
        .block_test(task)
        .unwrap()
        .config
        .id;
    let tab = tab(env, conn);
    let g = run(env, &tab, &format!("g.V().has('tag', '{tag}')"));
    (g, tag)
}

/// The Inspector in a window of its own, bound to `graph`.
fn inspector(env: &mut Env, graph: &Entity<GraphView>) -> (Entity<Inspector>, VisualTestContext) {
    let mut out = None;
    let graph = graph.clone();
    let (handle, _) = env
        .cx
        .update(|_, cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let i = cx.new(|cx| Inspector::new(window, cx));
                i.update(cx, |i, cx| i.set_graph(graph, cx));
                out = Some(i.clone());
                cx.new(|_| Host(i.into()))
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, &env.cx.cx);
    vcx.simulate_resize(size(px(420.0), px(900.0)));
    (out.unwrap(), vcx)
}

struct Host(gpui_kit::AnyView);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

fn select(env: &mut Env, g: &Entity<GraphView>, caption: &str) -> NodeIx {
    let n = find(env, g, caption);
    env.cx
        .update(|_, cx| g.update(cx, |g, cx| g.select_and_focus(n, cx)));
    settle(env, g);
    n
}

/// Confirm the open preview and wait for the server's answer.
fn confirm(env: &mut Env, g: &Entity<GraphView>) {
    env.cx
        .update(|_, cx| g.update(cx, |g, cx| g.confirm_proposal(cx)));
    wait_edit(env, g);
}

fn wait_edit(env: &mut Env, g: &Entity<GraphView>) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        let done = env.cx.update(|_, cx| {
            let g = g.read(cx);
            g.proposal().is_none() && g.edit_in_flight() == 0 && g.pending_fetches() == 0
        });
        if done {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "edit did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    settle(env, g);
}

fn wait_for_counts(env: &mut Env, g: &Entity<GraphView>) {
    let started = Instant::now();
    while env
        .cx
        .update(|_, cx| g.read(cx).proposal().is_some_and(|p| p.waiting_for_counts))
    {
        env.cx.run_until_parked();
        assert!(started.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui_kit::test]
fn scenario_graph_editing_inspect_a_vertex(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, _) = seeded_graph(&mut env, false);
    select(&mut env, &g, "marko");
    let (inspector, mut icx) = inspector(&mut env, &g);
    // Wait for the degree query.
    let started = Instant::now();
    let degree = loop {
        icx.run_until_parked();
        let d = icx.update(|_, cx| match g.update(cx, |g, cx| g.selection(cx)) {
            Some(SelectedElement::Vertex { degree, .. }) => degree,
            _ => None,
        });
        if let Some(d) = d {
            break d;
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(degree, (0, 3), "out-degree 3");
    let rows = icx.update(|_, cx| inspector.update(cx, |i, cx| i.rows(cx)));
    let row = |k: &str| rows.iter().find(|r| r.key == k).cloned().unwrap();
    assert_eq!(
        (row("name").value, row("name").type_name),
        ("marko".into(), "String".into())
    );
    assert_eq!(
        (row("age").value, row("age").type_name),
        ("29".into(), "Int32".into())
    );
    let shown = icx.update(|window, cx| {
        window.render_frame(cx);
        [
            "inspector-label",
            "inspector-id",
            "inspector-degree",
            "inspector-prop-age-0",
        ]
        .iter()
        .all(|id| window.try_find(*id).is_some())
    });
    assert!(
        shown,
        "the inspector shows label, id, degree and properties"
    );
}

#[gpui_kit::test]
fn scenario_graph_editing_navigate_to_endpoint(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, _) = seeded_graph(&mut env, false);
    let (marko, lop) = (find(&mut env, &g, "marko"), find(&mut env, &g, "lop"));
    env.cx.update(|_, cx| {
        g.update(cx, |g, cx| {
            g.canvas().update(cx, |c, _| {
                let s = c.store_mut();
                let e = (0..s.edge_count())
                    .map(|i| graph_store::EdgeIx(i as u32))
                    .find(|e| s.edge_endpoints_of(*e) == (marko, lop))
                    .unwrap();
                s.select_edge(e, false);
            });
            cx.notify();
        })
    });
    let (_inspector, mut icx) = inspector(&mut env, &g);
    icx.update(|window, cx| {
        window.render_frame(cx);
        window.click("inspector-in", cx);
    });
    std::thread::sleep(Duration::from_millis(400));
    settle(&mut env, &g);
    let (selected, centred) = env.cx.update(|_, cx| {
        let v = g.read(cx);
        let s = v.store(cx);
        let cam = v.canvas().read(cx).camera();
        let p = s.position(lop);
        (
            s.selected().collect::<Vec<_>>(),
            (cam.center[0] - p[0]).abs() < 0.5 && (cam.center[1] - p[1]).abs() < 0.5,
        )
    });
    assert_eq!(selected, vec![lop], "the in-vertex is selected");
    assert!(centred, "and centred");
}

#[gpui_kit::test]
fn scenario_graph_editing_edit_a_property(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, tag) = seeded_graph(&mut env, false);
    let marko = select(&mut env, &g, "marko");
    let (inspector, mut icx) = inspector(&mut env, &g);
    let age = icx.update(|_, cx| {
        inspector.update(cx, |i, cx| {
            i.rows(cx).into_iter().find(|r| r.key == "age").unwrap()
        })
    });
    icx.update(|window, cx| inspector.update(cx, |i, cx| i.start_edit(&age, window, cx)));
    let input = icx.update(|_, cx| inspector.read(cx).edit_value_input().clone());
    icx.update(|window, cx| input.update(cx, |s, cx| s.set_value("30", window, cx)));
    icx.update(|_, cx| inspector.update(cx, |i, cx| i.save_edit(cx)));
    let preview = env
        .cx
        .update(|_, cx| g.read(cx).proposal().unwrap().statement.clone());
    assert_eq!(
        preview.script,
        "g.V(elementId).property(single, propKey, propValue)"
    );
    assert!(
        preview
            .bindings
            .iter()
            .any(|(k, v)| k == "propValue" && *v == gremlin_model::GValue::Int32(30)),
        "the value is a typed binding"
    );
    assert_eq!(age_on_server(&tag), 29, "nothing runs before confirming");
    confirm(&mut env, &g);
    assert_eq!(age_on_server(&tag), 30);
    let rows = icx.update(|_, cx| inspector.update(cx, |i, cx| i.rows(cx)));
    assert_eq!(rows.iter().find(|r| r.key == "age").unwrap().value, "30");
    let local = env.cx.update(|_, cx| {
        g.read(cx)
            .store(cx)
            .node_prop(marko, "age")
            .map(|v| v.to_string())
    });
    assert_eq!(
        local.as_deref(),
        Some("30"),
        "the graph shows the new value"
    );
}

#[gpui_kit::test]
fn scenario_graph_editing_undo_property_change(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, tag) = seeded_graph(&mut env, false);
    select(&mut env, &g, "marko");
    let id = env.cx.update(|_, cx| {
        let v = g.read(cx);
        let n = v.store(cx).selected().next().unwrap();
        v.id_value(v.store(cx).node_ext(n)).unwrap()
    });
    env.cx.update(|_, cx| {
        g.update(cx, |g, cx| {
            g.propose(
                EditCommand::SetProperty {
                    element: ElementRef::vertex(id),
                    key: "age".into(),
                    value: gremlin_model::GValue::Int32(30),
                    cardinality: Cardinality::Single,
                },
                cx,
            )
        })
    });
    confirm(&mut env, &g);
    assert_eq!(age_on_server(&tag), 30);
    assert!(env.cx.update(|_, cx| g.read(cx).can_undo()));
    env.cx.update(|_, cx| g.update(cx, |g, cx| g.undo(cx)));
    let preview = env.cx.update(|_, cx| {
        g.read(cx)
            .proposal()
            .map(|p| (p.command.describe(), p.statement.bindings.clone()))
    });
    let (describe, bindings) = preview.expect("undo shows a preview");
    assert!(describe.starts_with("Set age = 29"), "{describe}");
    assert!(bindings.contains(&("propValue".into(), gremlin_model::GValue::Int32(29))));
    confirm(&mut env, &g);
    assert_eq!(age_on_server(&tag), 29);
    assert!(env.cx.update(|_, cx| g.read(cx).can_redo()));
}

#[gpui_kit::test]
fn scenario_graph_editing_draw_an_edge(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, tag) = seeded_graph(&mut env, false);
    let before = env.cx.update(|_, cx| g.read(cx).store(cx).edge_count());
    let (marko, lop) = (find(&mut env, &g, "marko"), find(&mut env, &g, "lop"));
    let (a, b) = env.cx.update(|_, cx| {
        let c = g.read(cx).canvas().read(cx);
        (c.node_window_position(marko), c.node_window_position(lop))
    });
    let alt = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    env.cx.simulate_mouse_down(a, MouseButton::Left, alt);
    env.cx.simulate_mouse_move(b, Some(MouseButton::Left), alt);
    env.cx.simulate_mouse_up(b, MouseButton::Left, alt);
    let label = env.cx.update(|_, cx| g.read(cx).link_label_input().clone());
    env.cx
        .update(|window, cx| label.update(cx, |s, cx| s.set_value("created", window, cx)));
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("edit-link-next", cx);
    });
    env.cx.run_until_parked();
    let script = env
        .cx
        .update(|_, cx| g.read(cx).proposal().map(|p| p.statement.script.clone()));
    assert_eq!(
        script.as_deref(),
        Some("g.V(fromId).addE(edgeLabel).to(__.V(toId))")
    );
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("edit-run", cx);
    });
    wait_edit(&mut env, &g);
    let created = server(
        "g.V().has('tag', tag).has('name','marko').outE('created').where(inV().has('name','lop')).count()",
        &tag,
    );
    assert_eq!(
        created.first().and_then(gremlin_model::GValue::as_i64),
        Some(2),
        "a second created edge"
    );
    assert_eq!(
        env.cx.update(|_, cx| g.read(cx).store(cx).edge_count()),
        before + 1,
        "and it is drawn"
    );
}

#[gpui_kit::test]
fn scenario_graph_editing_delete_a_vertex(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, tag) = seeded_graph(&mut env, false);
    select(&mut env, &g, "marko");
    env.cx
        .update(|_, cx| g.update(cx, |g, cx| g.delete_selection(cx)));
    wait_for_counts(&mut env, &g);
    let counts = env
        .cx
        .update(|_, cx| g.read(cx).proposal().and_then(|p| p.counts));
    assert_eq!(counts, Some((1, 3)), "1 vertex and 3 edges");
    let text = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("edit-counts").is_some()
    });
    assert!(text, "the confirmation states the counts");
    confirm(&mut env, &g);
    let left = server("g.V().has('tag', tag).values('name')", &tag);
    assert_eq!(left.len(), 3, "marko is gone");
    let (nodes, edges) = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        (s.node_count(), s.edge_count())
    });
    assert_eq!(
        (nodes, edges),
        (3, 0),
        "and so are its node and edges in the graph"
    );
}

#[gpui_kit::test]
fn scenario_graph_editing_production_confirmation(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let (g, tag) = seeded_graph(&mut env, true);
    select(&mut env, &g, "marko");
    let id = env.cx.update(|_, cx| {
        let v = g.read(cx);
        let n = v.store(cx).selected().next().unwrap();
        v.id_value(v.store(cx).node_ext(n)).unwrap()
    });
    env.cx.update(|_, cx| {
        g.update(cx, |g, cx| {
            g.propose(
                EditCommand::SetProperty {
                    element: ElementRef::vertex(id),
                    key: "age".into(),
                    value: gremlin_model::GValue::Int32(31),
                    cardinality: Cardinality::Single,
                },
                cx,
            );
            g.confirm_proposal(cx);
        })
    });
    env.cx.run_until_parked();
    assert!(
        env.cx.update(|window, cx| window.has_active_dialog(cx)),
        "a second confirmation naming the connection"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(age_on_server(&tag), 29, "not run yet");
    support::click_settled(&mut env.cx, "ok");
    wait_edit(&mut env, &g);
    assert_eq!(age_on_server(&tag), 31);
}
