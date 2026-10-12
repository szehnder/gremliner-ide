//! Schema discovery in the app against the Docker testenv (skipped with a message when it is down): the
//! cache on reconnect, the sidebar schema browser and the schema graph. Scenario names map to
//! `openspec/changes/schema-discovery/specs/schema-discovery/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicBool, Ordering};
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
use ui_results::{ResultSet, RunStatus};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorTabs;
use ui_workspace::run::Runner;
use ui_workspace::schema::SchemaRegistry;
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
    storage: Storage,
    cx: VisualTestContext,
}

fn setup(cx: &mut TestAppContext) -> Env {
    cx.executor().allow_parking();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let storage = Storage::open_in_memory().unwrap();
    let shell_storage = storage.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(shell_storage),
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
    Env {
        ws,
        storage,
        cx: vcx,
    }
}

fn connection(env: &mut Env, name: &str, alias: &str) -> Uuid {
    let config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig {
            alias: alias.into(),
            ..Default::default()
        }),
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

fn connect(env: &mut Env, id: Uuid) {
    let manager = env.cx.update(|_, cx| ConnectionManager::global(cx));
    let task = env
        .cx
        .update(|_, cx| manager.update(cx, |m, cx| m.connect(id, cx)));
    env.cx.foreground_executor().block_test(task);
    env.cx.run_until_parked();
}

fn registry(env: &mut Env) -> Entity<SchemaRegistry> {
    env.cx.update(|_, cx| SchemaRegistry::global(cx))
}

/// Wait (real time) until `done` holds.
fn wait_until(env: &mut Env, what: &str, mut done: impl FnMut(&mut Env) -> bool) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if done(env) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait until discovery for `id` has finished.
fn wait_discovered(env: &mut Env, id: Uuid) -> Arc<schema::Schema> {
    let reg = registry(env);
    wait_until(env, "schema discovery", |env| {
        env.cx.update(|_, cx| {
            reg.read(cx)
                .entry(id)
                .is_some_and(|e| !e.refreshing && e.schema.as_ref().is_some_and(|s| !s.in_progress))
        })
    });
    env.cx
        .update(|_, cx| reg.read(cx).schema(id))
        .expect("schema")
}

#[gpui_kit::test]
fn scenario_schema_discovery_cached_schema_on_reconnect(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let id = connection(&mut env, "Modern", "gModern");
    connect(&mut env, id);
    let first = wait_discovered(&mut env, id);
    assert!(first.vertex_label("person").is_some());

    // The finished schema was cached.
    let storage = env.storage.clone();
    let cached = futures::executor::block_on(storage.load_schema(&id.to_string()))
        .unwrap()
        .expect("cached");
    assert_eq!(cached.discovered_at_ms, first.discovered_at_ms);

    // Disconnect, and make the cache distinguishable from what discovery will find.
    let manager = env.cx.update(|_, cx| ConnectionManager::global(cx));
    env.cx
        .update(|_, cx| manager.update(cx, |m, cx| m.disconnect(id, cx)));
    let mut stale = schema::Schema::from_json(&cached.json).unwrap();
    stale
        .vertex_labels
        .push(schema::LabelInfo::new("cachedonly"));
    stale.discovered_at_ms = 1_000;
    futures::executor::block_on(storage.save_schema(storage::CachedSchema {
        connection_id: id.to_string(),
        discovered_at_ms: 1_000,
        json: stale.to_json(),
    }))
    .unwrap();
    // A fresh registry state, as after an app restart.
    let reg = registry(&mut env);
    let saw_cache = Arc::new(AtomicBool::new(false));
    let flag = saw_cache.clone();
    env.cx.update(|_, cx| {
        cx.observe(&reg, move |reg, cx| {
            let entry = reg.read(cx).entry(id);
            if entry.is_some_and(|e| {
                e.from_cache
                    && e.schema
                        .as_ref()
                        .is_some_and(|s| s.vertex_label("cachedonly").is_some())
            }) {
                flag.store(true, Ordering::SeqCst);
            }
        })
        .detach();
    });
    env.cx.update(|_, cx| reg.update(cx, |r, _| r.forget(id)));

    connect(&mut env, id);
    let refreshed = wait_discovered(&mut env, id);
    assert!(
        saw_cache.load(Ordering::SeqCst),
        "the cached schema was shown before discovery finished"
    );
    assert!(
        refreshed.vertex_label("cachedonly").is_none(),
        "refresh replaced it"
    );
    assert!(refreshed.discovered_at_ms > 1_000);
    let entry = env
        .cx
        .update(|_, cx| reg.read(cx).entry(id).cloned())
        .unwrap();
    assert!(!entry.from_cache);
}

#[gpui_kit::test]
fn scenario_schema_discovery_open_sample_query_from_schema(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let id = connection(&mut env, "Air", "g");
    connect(&mut env, id);
    let schema = wait_discovered(&mut env, id);
    assert!(schema.vertex_label("airport").is_some());

    // The tree shows the label with its count; opening it shows typed properties.
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("schema-label-air-v-airport", cx);
    });
    env.cx.run_until_parked();
    let shows_code = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::SharedString::from(
                "schema-prop-air-v-airport-code",
            ))
            .is_some()
    });
    assert!(shows_code, "airport properties are listed");

    let tabs_before = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs().len());
    env.cx.update(|window, cx| {
        window.hover("schema-label-air-v-airport", cx);
        window.click("schema-sample-air-v-airport", cx);
    });
    let started = Instant::now();
    let result: Entity<ResultSet> = loop {
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
            started.elapsed() < Duration::from_secs(20),
            "sample query ran"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let (query, status, count) = env.cx.update(|_, cx| {
        let r = result.read(cx);
        (r.query.clone(), r.status.clone(), r.items.len())
    });
    assert_eq!(query, "g.V().hasLabel('airport').limit(25)");
    assert_eq!(status, RunStatus::Succeeded);
    assert_eq!(count, 25);
    let tabs = env
        .cx
        .update(|_, cx| EditorTabs::global(cx).read(cx).tabs());
    assert_eq!(tabs.len(), tabs_before + 1, "a new tab opened");
    let (text, bound) = env.cx.update(|_, cx| {
        let tab = tabs.last().unwrap().read(cx);
        (tab.editor().read(cx).text(cx), tab.connection_id())
    });
    assert_eq!(text, "g.V().hasLabel('airport').limit(25)");
    assert_eq!(bound, Some(id));
}

#[gpui_kit::test]
fn scenario_schema_discovery_schema_graph_of_air_routes(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let id = connection(&mut env, "Air", "g");
    connect(&mut env, id);
    wait_discovered(&mut env, id);
    let ws = env.ws.clone();
    let panel = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.open_schema_graph(Some(id), window, cx)))
        .expect("panel");
    env.cx.run_until_parked();
    let graph = env.cx.update(|_, cx| panel.read(cx).graph().clone());
    let (vertices, edges) = env.cx.update(|_, cx| {
        let g = graph.read(cx).store(cx);
        let vertices: Vec<String> = (0..g.node_count())
            .map(|i| g.caption(graph_store::NodeIx(i as u32)).to_string())
            .collect();
        let edges: Vec<String> = g
            .edge_labels()
            .iter()
            .map(|l| g.edge_label_name(*l).to_owned())
            .collect();
        (vertices, edges)
    });
    for label in ["airport", "country", "continent"] {
        assert!(
            vertices.iter().any(|v| v == label),
            "{label} in {vertices:?}"
        );
    }
    assert!(edges.iter().any(|e| e == "route"), "{edges:?}");
    assert!(edges.iter().any(|e| e == "contains"), "{edges:?}");
    assert!(env.cx.update(|_, cx| graph.read(cx).is_offline()));
    assert_eq!(
        env.cx.update(|_, cx| graph.read(cx).pending_fetches()),
        0,
        "the schema graph never queries the server"
    );

    // Opening it again focuses the same tab.
    let again = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.open_schema_graph(Some(id), window, cx)))
        .expect("panel");
    assert_eq!(again.entity_id(), panel.entity_id());
}

#[gpui_kit::test]
fn unknown_labels_in_results_refresh_the_schema(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let id = connection(&mut env, "Modern", "gModern");
    connect(&mut env, id);
    wait_discovered(&mut env, id);
    let reg = registry(&mut env);
    // Known labels and a recent refresh: nothing happens.
    env.cx.update(|_, cx| {
        reg.update(cx, |r, cx| {
            r.note_result_labels(id, ["person", "unknown"], cx)
        });
    });
    assert!(
        !env.cx
            .update(|_, cx| reg.read(cx).entry(id).unwrap().refreshing)
    );
    env.cx.update(|_, cx| {
        reg.update(cx, |r, cx| {
            r.reset_auto_refresh(id);
            r.note_result_labels(id, ["person", "software"], cx);
        });
    });
    assert!(
        !env.cx
            .update(|_, cx| reg.read(cx).entry(id).unwrap().refreshing)
    );
    env.cx.update(|_, cx| {
        reg.update(cx, |r, cx| {
            r.note_result_labels(id, ["person", "unknown"], cx)
        });
    });
    assert!(
        env.cx
            .update(|_, cx| reg.read(cx).entry(id).unwrap().refreshing),
        "an unknown label starts a refresh"
    );
    wait_discovered(&mut env, id);
}
