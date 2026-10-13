//! The Graph Explorer tab: validation, open in editor, and "Search path from here". Scenario names map to
//! `openspec/changes/graph-explorer/specs/graph-explorer/spec.md`; generation and search scenarios are in
//! `crates/gremlin-syntax/tests/explorer.rs` and `crates/providers/tests/explorer.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use gremlin_syntax::explorer::Op;
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::{EditorTabs, ExplorerPanel};
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
    vcx.simulate_resize(size(px(1400.0), px(1000.0)));
    Env { ws, cx: vcx }
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

fn explorer(env: &mut Env, conn: Uuid) -> Entity<ExplorerPanel> {
    let ws = env.ws.clone();
    let panel = env.cx.update(|window, cx| {
        ws.update(cx, |ws, cx| ws.open_explorer(Some(conn), None, window, cx))
            .unwrap()
    });
    env.cx.run_until_parked();
    panel
}

#[gpui_kit::test]
fn scenario_graph_explorer_path_must_end_on_a_vertex(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let panel = explorer(&mut env, conn);
    support::click_settled(&mut env.cx, "explorer-add-step"); // an edge
    let hint = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("explorer-hint").is_some()
    });
    assert!(hint, "a hint to add a vertex step");
    assert!(
        env.cx
            .update(|_, cx| panel.read(cx).path().problem())
            .unwrap()
            .contains("must end on a vertex")
    );
    // Search is disabled: clicking it runs nothing.
    support::click_settled(&mut env.cx, "explorer-run");
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_none())
    );
    support::click_settled(&mut env.cx, "explorer-add-step"); // a vertex
    support::click_settled(&mut env.cx, "explorer-run");
    assert!(
        env.cx
            .update(|_, cx| Runner::global(cx).read(cx).latest().is_some()),
        "ending on a vertex runs"
    );
}

#[gpui_kit::test]
fn scenario_graph_explorer_open_in_editor(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let panel = explorer(&mut env, conn);
    env.cx.update(|window, cx| {
        panel.update(cx, |p, cx| {
            p.set_label(0, Some("airport".into()), cx);
            p.add_filter(0, window, cx);
            p.set_filter_key(0, 0, "city".into(), cx);
            p.set_filter_op(0, 0, Op::StartingWith, cx);
            p.set_filter_value(0, 0, "San", window, cx);
        })
    });
    env.cx.run_until_parked();
    let tabs_before = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs().len());
    support::click_settled(&mut env.cx, "explorer-open-editor");
    env.cx.run_until_parked();
    let tabs = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs());
    assert_eq!(tabs.len(), tabs_before + 1, "a new tab");
    let (text, tab_conn) = env.cx.update(|_, cx| {
        let tab = tabs.last().unwrap().read(cx);
        (tab.editor().read(cx).text(cx), tab.connection_id())
    });
    assert!(text.contains("parameters: p0 = 'San'"), "{text}");
    assert!(
        text.contains("g.V().has('airport', 'city', TextP.startingWith('San')).path().limit(200)"),
        "{text}"
    );
    assert_eq!(tab_conn, Some(conn));
}

#[gpui_kit::test]
fn scenario_graph_explorer_start_from_a_node(cx: &mut TestAppContext) {
    if !gremlin_client::testenv::is_up(&gremlin_client::testenv::url()) {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    let mut env = setup(cx);
    let conn = connection(&mut env);
    let ws = env.ws.clone();
    let tab = env.cx.update(|window, cx| {
        ws.update(cx, |ws, cx| {
            ws.open_and_run("g.V().has('code','AUS')", Some(conn), window, cx)
        })
    });
    let started = Instant::now();
    let graph = loop {
        env.cx.run_until_parked();
        let id = env.cx.update(|_, cx| tab.read(cx).id());
        let graph = env
            .cx
            .update(|_, cx| Runner::global(cx).read(cx).graph_for_tab(id).cloned());
        if let Some(g) = graph
            && env.cx.update(|_, cx| g.read(cx).store(cx).node_count()) == 1
        {
            break g;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "AUS in the graph"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    env.cx
        .update(|_, cx| graph.update(cx, |g, cx| g.search_path_from(graph_store::NodeIx(0), cx)));
    env.cx.run_until_parked();
    let explorer = env.cx.update(|_, cx| {
        let dock = ws.read(cx).dock_area().read(cx);
        dock.layout(gpui_kit::component::dock::DockPlacement::Center)
            .unwrap()
            .panels()
            .find_map(|id| dock.panel(id)?.view().downcast::<ExplorerPanel>().ok())
    });
    let explorer = explorer.expect("the explorer opened");
    let (script, bindings, conn_of) = env.cx.update(|_, cx| {
        let p = explorer.read(cx);
        let g = p.generated(cx).unwrap();
        (g.script, g.bindings, p.connection())
    });
    assert!(script.starts_with("g.V(p0)"), "{script}");
    assert_eq!(conn_of, Some(conn));
    let aus_id = RUNTIME.block_on(async {
        let c = gremlin_client::Client::connect(gremlin_client::ClientConfig::new(
            gremlin_client::testenv::url(),
        ))
        .await
        .unwrap();
        c.execute("g.V().has('code','AUS').id()", Default::default())
            .await
            .unwrap()
            .items[0]
            .clone()
    });
    assert_eq!(bindings[0].1, aus_id, "fixed to the AUS vertex by id");
    let shown = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("explorer-fixed").is_some()
    });
    assert!(shown);
}
