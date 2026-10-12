//! The assistant panel end to end: a scripted OpenAI-compatible model (mock server) and the Docker
//! testenv. Scenario names map to `openspec/changes/assistant-panel/specs/assistant-panel/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use llm::testing::{MockServer, Route, openai_stream};
use providers::{ConnectionConfig, GremlinServerConfig, PasswordChange, ProviderConfig};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_assistant::{ChatView, ChipKind};
use ui_workspace::ai::LlmProfiles;
use ui_workspace::connections::ConnectionManager;
use ui_workspace::io;
use ui_workspace::panels::EditorPanel;
use ui_workspace::panels::{AssistantPanel, EditorTabs};
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
        eprintln!("SKIPPED: testenv not reachable");
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

fn connection(env: &mut Env) -> Uuid {
    let config = ConnectionConfig::new(
        "Air",
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

fn profile(env: &mut Env, base: &str) {
    let mut p = llm::preset("custom").unwrap().profile("Scripted");
    p.base_url = base.to_owned();
    p.model = "scripted".into();
    let task = env.cx.update(|_, cx| {
        LlmProfiles::global(cx).update(cx, |s, cx| s.save(p, llm::KeyChange::Keep, cx))
    });
    env.cx.foreground_executor().block_test(task).unwrap();
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

fn connect(env: &mut Env, id: Uuid) {
    let manager = env.cx.update(|_, cx| ConnectionManager::global(cx));
    let task = env
        .cx
        .update(|_, cx| manager.update(cx, |m, cx| m.connect(id, cx)));
    env.cx.foreground_executor().block_test(task);
    wait_until(env, "schema discovery", |env| {
        env.cx.update(|_, cx| {
            SchemaRegistry::global(cx)
                .read(cx)
                .entry(id)
                .is_some_and(|e| !e.refreshing && e.schema.is_some())
        })
    });
}

fn panel(env: &mut Env) -> Entity<AssistantPanel> {
    let ws = env.ws.clone();
    let p = env
        .cx
        .update(|window, cx| ws.update(cx, |ws, cx| ws.toggle_assistant(window, cx)))
        .unwrap();
    env.cx.run_until_parked();
    p
}

fn chat(
    env: &mut Env,
    panel: &Entity<AssistantPanel>,
    tab: &Entity<EditorPanel>,
) -> Entity<ChatView> {
    let (p, t) = (panel.clone(), tab.clone());
    env.cx
        .update(|window, cx| p.update(cx, |p, cx| p.view_for(&t, window, cx)))
}

fn set_text(env: &mut Env, tab: &Entity<EditorPanel>, text: &str) {
    let text = text.to_owned();
    env.cx.update(|window, cx| {
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.set_text(&text, window, cx));
        editor
            .read(cx)
            .state()
            .clone()
            .update(cx, |s, cx| s.set_selected_range(0..0, cx));
    });
    env.cx.run_until_parked();
}

fn wait_answer(env: &mut Env, view: &Entity<ChatView>) {
    let v = view.clone();
    wait_until(env, "the answer", move |env| {
        env.cx
            .update(|_, cx| !v.read(cx).busy() && !v.read(cx).entries.is_empty())
    });
}

fn llm_server(replies: Vec<&str>) -> MockServer {
    RUNTIME.block_on(MockServer::start(vec![Route::sse_sequence(
        "/v1/chat/completions",
        replies.into_iter().map(openai_stream).collect(),
    )]))
}

#[gpui_kit::test]
fn scenario_assistant_panel_separate_conversations(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = llm_server(vec!["Answer for A.", "Answer for B."]);
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let a = tab(&mut env, id);
    let b = tab(&mut env, id);
    let p = panel(&mut env);
    let view_a = chat(&mut env, &p, &a);
    env.cx.update(|_, cx| {
        view_a.update(cx, |v, cx| {
            v.send(assistant::chat::Mode::Chat, "question from A".into(), cx)
        })
    });
    wait_answer(&mut env, &view_a);
    // Switch to tab B.
    let b_id = env.cx.update(|_, cx| b.read(cx).id());
    env.cx
        .update(|_, cx| EditorTabs::global(cx).update(cx, |t, cx| t.set_active(b_id, cx)));
    env.cx.run_until_parked();
    let view_b = chat(&mut env, &p, &b);
    assert!(view_a.entity_id() != view_b.entity_id());
    assert!(
        env.cx.update(|_, cx| view_b.read(cx).entries.is_empty()),
        "B starts empty"
    );
    env.cx.update(|_, cx| {
        view_b.update(cx, |v, cx| {
            v.send(assistant::chat::Mode::Chat, "question from B".into(), cx)
        })
    });
    wait_answer(&mut env, &view_b);
    // B's request did not carry A's conversation.
    let second: serde_json::Value = serde_json::from_str(&llm.requests()[1].body).unwrap();
    let texts: Vec<String> = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"].to_string())
        .collect();
    assert!(
        !texts.iter().any(|t| t.contains("question from A")),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t.contains("question from B")));
    let a_entries = env.cx.update(|_, cx| view_a.read(cx).entries.len());
    assert_eq!(a_entries, 2, "A keeps its own transcript");
}

#[gpui_kit::test]
fn scenario_assistant_panel_explain_with_step_mapping(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = llm_server(vec![
        r#"{"items": [{"steps": [1], "text": "All vertices."}, {"steps": [2], "text": "The one with code AUS."}, {"steps": [3], "text": "Its outgoing routes."}, {"steps": [4], "text": "Their codes."}]}"#,
    ]);
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let t = tab(&mut env, id);
    let query = "g.V().has('code','AUS').out('route').values('code')";
    set_text(&mut env, &t, query);
    let p = panel(&mut env);
    let view = chat(&mut env, &p, &t);
    support::click_settled(&mut env.cx, "assistant-explain");
    wait_answer(&mut env, &view);
    let items = env.cx.update(|_, cx| match view.read(cx).entries.last() {
        Some(ui_assistant::chat_view::Entry::Assistant(a)) => a.explain.len(),
        _ => 0,
    });
    assert_eq!(items, 4, "an item per step");
    env.cx.update(|window, cx| {
        window.render_frame(cx);
        window.hover("explain-item-2", cx);
    });
    env.cx.run_until_parked();
    let ranges = env
        .cx
        .update(|_, cx| t.read(cx).editor().read(cx).hint_ranges().to_vec());
    assert_eq!(ranges.len(), 1);
    assert_eq!(&query[ranges[0].clone()], "out('route')");
}

#[gpui_kit::test]
fn scenario_assistant_panel_remove_context(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = llm_server(vec!["With schema.", "Without schema."]);
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    connect(&mut env, id);
    let t = tab(&mut env, id);
    let p = panel(&mut env);
    let view = chat(&mut env, &p, &t);
    env.cx.run_until_parked();
    assert!(
        env.cx
            .update(|_, cx| view.read(cx).included().contains(&ChipKind::Schema)),
        "schema chip shown"
    );
    env.cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.send(assistant::chat::Mode::Chat, "first".into(), cx)
        })
    });
    wait_answer(&mut env, &view);
    support::click_settled(&mut env.cx, "chip-schema");
    assert!(
        !env.cx
            .update(|_, cx| view.read(cx).included().contains(&ChipKind::Schema))
    );
    env.cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.send(assistant::chat::Mode::Chat, "second".into(), cx)
        })
    });
    wait_answer(&mut env, &view);
    let system = |i: usize| -> String {
        let body: serde_json::Value = serde_json::from_str(&llm.requests()[i].body).unwrap();
        body["messages"][0]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    assert!(
        system(0).contains("Schema of the connected graph"),
        "{}",
        system(0)
    );
    assert!(
        !system(1).contains("Schema of the connected graph"),
        "{}",
        system(1)
    );
    assert!(!system(1).contains("airport -route-> airport"));
}

#[gpui_kit::test]
fn scenario_assistant_panel_insert_from_answer(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = llm_server(vec![
        "Use this:\n```gremlin\ng.V().has('airport','code','AUS').out('route').count()\n```\nIt counts the routes.",
    ]);
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    connect(&mut env, id);
    let t = tab(&mut env, id);
    set_text(&mut env, &t, "g.V().count()");
    let p = panel(&mut env);
    let view = chat(&mut env, &p, &t);
    env.cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.send(
                assistant::chat::Mode::Chat,
                "how many routes from Austin?".into(),
                cx,
            )
        })
    });
    wait_answer(&mut env, &view);
    let block = env.cx.update(|_, cx| match view.read(cx).entries.last() {
        Some(ui_assistant::chat_view::Entry::Assistant(a)) => a.blocks.first().cloned(),
        _ => None,
    });
    let block = block.expect("a verified code block");
    assert!(block.verified(), "{block:?}");
    support::click_settled(&mut env.cx, "assistant-insert-100");
    let text = env.cx.update(|_, cx| t.read(cx).editor().read(cx).text(cx));
    assert_eq!(
        text,
        "g.V().count()\ng.V().has('airport','code','AUS').out('route').count()"
    );
}
