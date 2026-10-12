//! The Graph view end to end against the Docker testenv (skipped with a message when it is down):
//! follow-up queries for missing endpoints and edges between vertices, and expanding neighbours.
//! Scenario names map to `openspec/changes/graph-view/specs/graph-view/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, TestAppContext,
    VisualTestContext, WindowOptions, px, size,
};
use graph_store::{NodeIx, flags};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_results::graph_view::{ExpandDirection, GraphView};
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

fn captions(env: &mut Env, g: &Entity<GraphView>) -> Vec<String> {
    env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        let mut c: Vec<String> = (0..s.node_count())
            .map(|i| s.caption(NodeIx(i as u32)).to_string())
            .collect();
        c.sort();
        c
    })
}

fn placeholders(env: &mut Env, g: &Entity<GraphView>) -> usize {
    env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        (0..s.node_count())
            .filter(|i| s.has_flag(NodeIx(*i as u32), flags::PLACEHOLDER))
            .count()
    })
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

#[gpui_kit::test]
fn scenario_graph_view_paths_become_a_graph(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Air routes", "g", false);
    let tab = tab(&mut env, conn);
    let g = run(
        &mut env,
        &tab,
        "g.V().has('code','AUS').outE('route').inV().path().limit(10)",
    );
    let (nodes, edges) = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        (s.node_count(), s.edge_count())
    });
    assert_eq!(
        (nodes, edges),
        (11, 10),
        "AUS plus ten destinations, no duplicate AUS"
    );
    assert_eq!(
        captions(&mut env, &g)
            .iter()
            .filter(|c| *c == "AUS")
            .count(),
        1
    );
    let aus = find(&mut env, &g, "AUS");
    let directed_from_aus = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        (0..s.edge_count()).all(|e| s.edge_endpoints_of(graph_store::EdgeIx(e as u32)).0 == aus)
    });
    assert!(
        directed_from_aus,
        "edges point from AUS to the destinations"
    );
}

#[gpui_kit::test]
fn scenario_graph_view_edges_between_results(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.V().hasLabel('person')");
    let edges: Vec<(String, String, String)> = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        let mut v: Vec<_> = (0..s.edge_count())
            .map(|e| {
                let e = graph_store::EdgeIx(e as u32);
                let (a, b) = s.edge_endpoints_of(e);
                (
                    s.caption(a).to_string(),
                    s.edge_label_name(s.edge_label(e)).to_owned(),
                    s.caption(b).to_string(),
                )
            })
            .collect();
        v.sort();
        v
    });
    let t = |a: &str, l: &str, b: &str| (a.to_owned(), l.to_owned(), b.to_owned());
    assert_eq!(
        edges,
        vec![t("marko", "knows", "josh"), t("marko", "knows", "vadas")]
    );
    assert_eq!(
        captions(&mut env, &g).len(),
        4,
        "no software vertices pulled in"
    );

    // With the option off, a vertex-only result stays edgeless.
    env.cx
        .update(|_, cx| g.update(cx, |g, cx| g.set_edges_between(false, cx)));
    let g = run(&mut env, &tab, "g.V().hasLabel('person').limit(3)");
    assert_eq!(env.cx.update(|_, cx| g.read(cx).store(cx).edge_count()), 0);
}

#[gpui_kit::test]
fn scenario_graph_view_edge_only_result(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.E().hasLabel('created')");
    assert_eq!(env.cx.update(|_, cx| g.read(cx).store(cx).edge_count()), 4);
    assert_eq!(
        captions(&mut env, &g),
        ["josh", "lop", "marko", "peter", "ripple"],
        "endpoints were fetched and captioned"
    );
    assert_eq!(placeholders(&mut env, &g), 0);
}

#[gpui_kit::test]
fn scenario_graph_view_double_click_expand(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Air routes", "g", false);
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.V().has('code','AUS')");
    let aus = find(&mut env, &g, "AUS");
    let at = env
        .cx
        .update(|_, cx| g.read(cx).canvas().read(cx).node_window_position(aus));
    env.cx.simulate_event(gpui_kit::MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    env.cx.simulate_event(gpui_kit::MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    settle(&mut env, &g);
    let (nodes, edges, more) = env.cx.update(|_, cx| {
        let v = g.read(cx);
        let s = v.store(cx);
        (s.node_count(), s.edge_count(), v.more_text())
    });
    assert_eq!(edges, 100, "capped at 100 edges");
    assert!(nodes > 1 && nodes <= 101, "{nodes} nodes");
    assert_eq!(more.as_deref(), Some("100 of 198 neighbours of AUS shown"));
    assert_eq!(
        placeholders(&mut env, &g),
        0,
        "neighbour details were fetched"
    );
    // New neighbours sit near AUS.
    let near = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        let a = s.position(aus);
        (0..s.node_count()).all(|i| {
            let p = s.position(NodeIx(i as u32));
            ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt() < 400.0
        })
    });
    assert!(near, "neighbours are placed around AUS");

    // Load more fetches the next page.
    env.cx.update(|_, cx| g.update(cx, |g, cx| g.load_more(cx)));
    settle(&mut env, &g);
    let edges = env.cx.update(|_, cx| g.read(cx).store(cx).edge_count());
    assert_eq!(edges, 198, "the second page brings the rest");
    assert_eq!(env.cx.update(|_, cx| g.read(cx).more_text()), None);
}

#[gpui_kit::test]
fn scenario_graph_view_expand_by_label(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Air routes", "g", false);
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.V().has('code','AUS')");
    let aus = find(&mut env, &g, "AUS");
    env.cx.update(|_, cx| {
        g.update(cx, |g, cx| {
            g.expand(aus, ExpandDirection::Out, Some("route".into()), cx)
        })
    });
    settle(&mut env, &g);
    let (edges, all_out_routes) = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        let all = (0..s.edge_count()).all(|e| {
            let e = graph_store::EdgeIx(e as u32);
            s.edge_endpoints_of(e).0 == aus && s.edge_label_name(s.edge_label(e)) == "route"
        });
        (s.edge_count(), all)
    });
    assert_eq!(edges, 98, "AUS has 98 outgoing routes");
    assert!(all_out_routes, "only outgoing route edges were added");
    assert_eq!(env.cx.update(|_, cx| g.read(cx).more_text()), None);
}

#[gpui_kit::test]
fn layout_choice_persists_per_tab(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Modern", "gModern", false);
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.V()");
    let grid = ui_results::graph_view::LayoutSettings {
        kind: graph_layout::LayoutKind::Grid,
        ..Default::default()
    };
    env.cx
        .update(|_, cx| g.update(cx, |g, cx| g.set_layout_settings(grid, true, cx)));
    settle(&mut env, &g);
    let tab_id = env.cx.update(|_, cx| tab.read(cx).id());
    let saved = (0..200).find_map(|_| {
        env.cx.run_until_parked();
        let v = env.cx.update(|_, cx| {
            ui_workspace::settings::AppSettings::storage(cx)
                .unwrap()
                .get_json_blocking::<serde_json::Value>(
                    storage::Table::UiState,
                    &ui_workspace::run::layout_key(tab_id),
                )
                .unwrap()
        });
        if v.is_none() {
            std::thread::sleep(Duration::from_millis(10));
        }
        v
    });
    let saved = saved.expect("layout saved");
    assert_eq!(
        ui_results::graph_view::LayoutSettings::from_json(&saved),
        grid
    );
    // Grid: six nodes on a 3-column grid (two rows).
    let rows = env.cx.update(|_, cx| {
        let s = g.read(cx).store(cx);
        let mut ys: Vec<i64> = s.positions().iter().map(|p| p[1].round() as i64).collect();
        ys.sort_unstable();
        ys.dedup();
        ys.len()
    });
    assert_eq!(rows, 2);
}

#[gpui_kit::test]
fn scenario_graph_stylesheets_connection_default(cx: &mut TestAppContext) {
    use ui_results::style_library::StyleLibrary;
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env, "Air routes", "g", false);
    // A stylesheet "Air", made the connection's default.
    let air = env.cx.update(|_, cx| {
        let library = StyleLibrary::global(cx);
        library.update(cx, |l, cx| {
            let id = l.create("Air", cx);
            l.set_connection_default(&conn.to_string(), Some(&id), cx);
            id
        })
    });
    let tab = tab(&mut env, conn);
    let g = run(&mut env, &tab, "g.V().has('code','AUS')");
    assert_eq!(
        env.cx.update(|_, cx| g.read(cx).stylesheet_id().to_owned()),
        air
    );
    // The stylesheet and the default were saved.
    let saved = (0..200).find_map(|_| {
        env.cx.run_until_parked();
        let storage = env.cx.update(|_, cx| {
            ui_workspace::settings::AppSettings::storage(cx)
                .unwrap()
                .clone()
        });
        let rows = futures::executor::block_on(storage.list_stylesheets()).unwrap();
        let defaults = storage
            .get_json_blocking::<std::collections::HashMap<String, String>>(
                storage::Table::UiState,
                ui_workspace::styles::DEFAULTS_KEY,
            )
            .unwrap();
        match (rows.iter().find(|r| r.id == air), defaults) {
            (Some(row), Some(d)) => Some((row.name.clone(), d)),
            _ => {
                std::thread::sleep(Duration::from_millis(10));
                None
            }
        }
    });
    let (name, defaults) = saved.expect("persisted");
    assert_eq!(name, "Air");
    assert_eq!(defaults.get(&conn.to_string()), Some(&air));
}
