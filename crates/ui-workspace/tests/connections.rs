//! Headless UI tests for connection management. Scenario names map to
//! `openspec/changes/connections/specs/connections/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use providers::{
    ConnectionConfig, ConnectionStore, Environment, GremlinServerConfig, PasswordChange,
    ProviderConfig, ProviderKind,
};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::connections::form::{self, ConnectionForm};
use ui_workspace::connections::{ConnStatus, ConnectionManager};
use ui_workspace::io;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

struct Env {
    storage: Storage,
    secrets: Arc<MemoryStore>,
}

fn setup(cx: &mut TestAppContext) -> (Env, Entity<Workspace>, VisualTestContext) {
    cx.executor().allow_parking();
    let storage = Storage::open_in_memory().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let shared: Arc<dyn SecretStore> = secrets.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(storage.clone()),
                theme_override: None,
                secrets: Some(shared),
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
    (Env { storage, secrets }, ws, vcx)
}

fn manager(cx: &mut VisualTestContext) -> Entity<ConnectionManager> {
    cx.update(|_, cx| ConnectionManager::global(cx))
}

fn names(cx: &mut VisualTestContext) -> Vec<String> {
    let m = manager(cx);
    cx.update(|_, cx| {
        m.read(cx)
            .connections()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    })
}

fn exists(id: &str, cx: &mut VisualTestContext) -> bool {
    let id = gpui_kit::SharedString::from(id.to_owned());
    cx.update(move |window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some_and(|s| s.visible())
    })
}

fn open_form(
    existing: Option<ConnectionConfig>,
    cx: &mut VisualTestContext,
) -> Entity<ConnectionForm> {
    let form = cx.update(|window, cx| form::open(existing, window, cx));
    cx.run_until_parked();
    form
}

fn set(
    form: &Entity<ConnectionForm>,
    field: fn(&ConnectionForm) -> &Entity<gpui_kit::component::input::InputState>,
    value: &str,
    cx: &mut VisualTestContext,
) {
    let value = value.to_owned();
    cx.update(|window, cx| {
        let state = field(form.read(cx)).clone();
        state.update(cx, |s, cx| s.set_value(value, window, cx));
    });
}

/// Wait (in real time, the work runs on tokio) until `cond` holds.
async fn wait_until(
    cx: &mut VisualTestContext,
    mut cond: impl FnMut(&mut VisualTestContext) -> bool,
) {
    for _ in 0..500 {
        cx.run_until_parked();
        if cond(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("condition not met within 5 s");
}

fn save_direct(
    name: &str,
    port: u16,
    env: Environment,
    cx: &mut VisualTestContext,
) -> ConnectionConfig {
    let mut config = ConnectionConfig::new(
        name,
        ProviderConfig::GremlinServer(GremlinServerConfig {
            port,
            ..Default::default()
        }),
    );
    config.environment = env;
    let m = manager(cx);
    let task = cx.update(|_, cx| m.update(cx, |m, cx| m.save(config, PasswordChange::Clear, cx)));
    let saved = cx.foreground_executor().block_test(task).unwrap();
    saved.config
}

#[gpui_kit::test]
async fn scenario_connections_create_a_connection(cx: &mut TestAppContext) {
    let (env, _ws, mut cx) = setup(cx);
    let form = open_form(None, &mut cx);
    set(&form, |f| &f.name, "Local", &mut cx);
    set(&form, |f| &f.host, "localhost", &mut cx);
    set(&form, |f| &f.port, "8182", &mut cx);
    cx.update(|window, cx| window.click("conn-save", cx));
    wait_until(&mut cx, |cx| {
        !cx.update(|window, cx| window.has_active_dialog(cx))
    })
    .await;
    assert_eq!(names(&mut cx), ["Local"]);
    assert!(exists("conn-row-local", &mut cx), "appears in the sidebar");
    // Persisted: a fresh store over the same database (a restart) lists it.
    let store = ConnectionStore::new(env.storage.clone(), env.secrets.clone());
    let saved = store.list().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].endpoint_label(), "localhost:8182");
}

#[gpui_kit::test]
async fn scenario_connections_duplicate_name_rejected(cx: &mut TestAppContext) {
    let (_env, _ws, mut cx) = setup(cx);
    save_direct("Local", 8182, Environment::Local, &mut cx);
    let form = open_form(None, &mut cx);
    set(&form, |f| &f.name, "local", &mut cx);
    cx.update(|window, cx| window.click("conn-save", cx));
    // The store answers from the I/O runtime (a real thread): wait for it.
    let f = form.clone();
    wait_until(&mut cx, move |cx| {
        cx.update(|_, cx| f.read(cx).errors.name.is_some())
    })
    .await;
    let error = cx.update(|_, cx| form.read(cx).errors.name.clone());
    assert_eq!(
        error.as_deref(),
        Some("A connection named \"local\" already exists.")
    );
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "the form stays open"
    );
    assert_eq!(names(&mut cx), ["Local"], "nothing saved");
}

fn paste_into_host(form: &Entity<ConnectionForm>, text: &str, cx: &mut VisualTestContext) {
    set(form, |f| &f.host, "", cx);
    cx.update(|window, cx| {
        let host = form.read(cx).host.clone();
        host.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.run_until_parked();
    let text = text.to_owned();
    cx.update(move |window, cx| window.input(&text, cx));
    cx.run_until_parked();
}

fn value(
    form: &Entity<ConnectionForm>,
    field: fn(&ConnectionForm) -> &Entity<gpui_kit::component::input::InputState>,
    cx: &mut VisualTestContext,
) -> String {
    cx.update(|_, cx| field(form.read(cx)).read(cx).value().to_string())
}

#[gpui_kit::test]
fn scenario_connections_paste_a_websocket_url(cx: &mut TestAppContext) {
    let (_env, _ws, mut cx) = setup(cx);
    let form = open_form(None, &mut cx);
    paste_into_host(&form, "wss://db.example.com:8182/gremlin", &mut cx);
    assert_eq!(value(&form, |f| &f.host, &mut cx), "db.example.com");
    assert_eq!(value(&form, |f| &f.port, &mut cx), "8182");
    assert_eq!(value(&form, |f| &f.path, &mut cx), "/gremlin");
    assert!(cx.update(|_, cx| form.read(cx).tls), "TLS enabled");
    assert_eq!(
        cx.update(|_, cx| form.read(cx).kind),
        ProviderKind::GremlinServer
    );
}

#[gpui_kit::test]
fn scenario_connections_paste_a_neptune_endpoint(cx: &mut TestAppContext) {
    let (_env, _ws, mut cx) = setup(cx);
    let form = open_form(None, &mut cx);
    paste_into_host(
        &form,
        "my-graph.cluster-c1a2b3c4d5e6.eu-west-2.neptune.amazonaws.com:8182",
        &mut cx,
    );
    assert_eq!(
        cx.update(|_, cx| form.read(cx).kind),
        ProviderKind::NeptuneAws
    );
    assert_eq!(value(&form, |f| &f.region, &mut cx), "eu-west-2");
    assert!(value(&form, |f| &f.host, &mut cx).ends_with(".neptune.amazonaws.com"));
}

#[gpui_kit::test]
async fn scenario_connections_status_indicator(cx: &mut TestAppContext) {
    let (_env, _ws, mut cx) = setup(cx);
    if !gremlin_client::testenv::is_up(&gremlin_client::testenv::url()) {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    let config = save_direct("Local", 8182, Environment::Local, &mut cx);
    let m = manager(&mut cx);
    assert!(matches!(
        cx.update(|_, cx| m.read(cx).status(config.id)),
        ConnStatus::Disconnected
    ));
    cx.update(|window, cx| window.click("conn-row-local", cx));
    assert!(
        matches!(
            cx.update(|_, cx| m.read(cx).status(config.id)),
            ConnStatus::Connecting
        ),
        "connecting immediately"
    );
    wait_until(&mut cx, |cx| {
        matches!(
            cx.update(|_, cx| m.read(cx).status(config.id)),
            ConnStatus::Connected(_)
        )
    })
    .await;
    assert!(exists("status-connection", &mut cx));
}

#[gpui_kit::test]
async fn scenario_connections_delete_requires_confirmation(cx: &mut TestAppContext) {
    let (env, _ws, mut cx) = setup(cx);
    let config = save_direct("Local", 8182, Environment::Local, &mut cx);
    env.secrets
        .set(&config.password_account(), "secret-to-remove")
        .unwrap();
    cx.update(|window, cx| {
        window.hover("conn-row-local", cx);
        window.click("conn-delete-local", cx);
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| window.has_active_dialog(cx)),
        "asks first"
    );
    assert_eq!(names(&mut cx), ["Local"], "nothing deleted yet");
    cx.update(|window, cx| window.click("ok", cx));
    wait_until(&mut cx, |cx| names(cx).is_empty()).await;
    assert_eq!(
        env.secrets.get(&config.password_account()).unwrap(),
        None,
        "secret removed"
    );
    assert!(env.storage.list_connections_blocking().unwrap().is_empty());
}

#[gpui_kit::test]
fn scenario_connections_production_badge(cx: &mut TestAppContext) {
    let (_env, _ws, mut cx) = setup(cx);
    let config = save_direct("Prod", 8182, Environment::Production, &mut cx);
    let m = manager(&mut cx);
    cx.update(|_, cx| m.update(cx, |m, cx| m.set_active(Some(config.id), cx)));
    cx.run_until_parked();
    // The status bar names the active connection with a production badge (danger colour by construction
    // in `environment_badge`); editor tabs gain the same badge with the query-editor change.
    let in_status_bar = cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("status-connection")
            .try_find("env-badge-production")
            .is_some()
    });
    assert!(in_status_bar);
}
