//! The prompt bar end to end: a scripted OpenAI-compatible model (mock server) and the Docker testenv
//! for dry runs. Scenario names map to `openspec/changes/nl-to-gremlin/specs/nl-to-gremlin/spec.md`.

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
use ui_assistant::PromptBar;
use ui_workspace::ai::LlmProfiles;
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

const TEXAS: &str = "g.V().has('airport','region','US-TX').values('code')";
const TEXAS_RUNWAYS: &str =
    "g.V().has('airport','region','US-TX').has('runways',gt(3)).values('code')";

fn answer(query: &str) -> String {
    openai_stream(
        &serde_json::json!({"query": query, "explanation": "Airport codes.", "intent": "read", "expected_shape": "table"})
            .to_string(),
    )
}

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

fn bar(env: &mut Env, tab: &Entity<EditorPanel>) -> Entity<PromptBar> {
    env.cx
        .update(|_, cx| tab.read(cx).prompt_bar().cloned())
        .expect("prompt bar open")
}

/// Ctrl+K in the editor, type, Enter; wait for the card to finish.
fn ask(env: &mut Env, tab: &Entity<EditorPanel>, prompt: &str) -> Entity<PromptBar> {
    env.cx.update(|window, cx| {
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.focus(window, cx));
    });
    env.cx.simulate_keystrokes("ctrl-k");
    env.cx.run_until_parked();
    let bar = bar(env, tab);
    env.cx.simulate_input(prompt);
    env.cx.simulate_keystrokes("enter");
    env.cx.run_until_parked();
    bar
}

fn wait_done(env: &mut Env, bar: &Entity<PromptBar>) {
    let b = bar.clone();
    wait_until(env, "the pipeline", move |env| {
        env.cx.update(|_, cx| {
            b.read(cx)
                .card
                .as_ref()
                .is_some_and(|c| c.outcome.is_some() || c.error.is_some())
        })
    });
}

#[gpui_kit::test]
fn scenario_nl_to_gremlin_no_llm_configured(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    env.cx
        .update(|window, cx| tab.update(cx, |t, cx| t.open_prompt_bar(window, cx)));
    env.cx.run_until_parked();
    let shows = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::SharedString::from("prompt-no-llm"))
            .is_some()
    });
    assert!(shows, "explains that a model is needed");
    support::click_settled(&mut env.cx, "prompt-open-settings");
    env.cx.run_until_parked();
    let opened = env.cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(gpui_kit::SharedString::from("settings-panel"))
            .is_some()
    });
    assert!(opened, "AI settings opened");
}

#[gpui_kit::test]
fn scenario_nl_to_gremlin_open_and_submit(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = RUNTIME.block_on(MockServer::start(vec![Route::sse(
        "/v1/chat/completions",
        &answer(TEXAS),
    )]));
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    let bar = ask(&mut env, &tab, "airports in Texas");
    // The card appears right away with progress.
    let (prompt, progressing) = env.cx.update(|_, cx| {
        let card = bar.read(cx).card.as_ref().expect("card");
        // A fast mock model may already have answered; either way the card shows where it is.
        (
            card.prompt.clone(),
            card.stage.is_some() || card.outcome.is_some() || card.candidate.is_some(),
        )
    });
    assert_eq!(prompt, "airports in Texas");
    assert!(progressing, "shows pipeline progress");
    let disclosure = env
        .cx
        .update(|_, cx| bar.read(cx).disclosure().cloned())
        .unwrap();
    // The scripted server is on localhost: a local model, so samples are shared by default (ADR-0021).
    assert_eq!(
        disclosure.text(),
        "Scripted \u{b7} sends schema + sample values"
    );
    wait_done(&mut env, &bar);
    let outcome = env
        .cx
        .update(|_, cx| bar.read(cx).card.as_ref().unwrap().outcome.clone())
        .unwrap();
    assert!(outcome.verified, "{outcome:?}");
    assert_eq!(outcome.answer.query, TEXAS);
    assert!(matches!(outcome.badges.dry_run, assistant::DryRun::Passed { rows, .. } if rows > 0));
    let rendered = env.cx.update(|window, cx| {
        window.render_frame(cx);
        ["prompt-badges", "prompt-query", "prompt-preview"]
            .iter()
            .all(|id| window.try_find(gpui_kit::SharedString::from(*id)).is_some())
    });
    assert!(rendered, "badges, query and preview are shown");
}

#[gpui_kit::test]
fn scenario_nl_to_gremlin_insert_into_editor(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = RUNTIME.block_on(MockServer::start(vec![Route::sse(
        "/v1/chat/completions",
        &answer(TEXAS),
    )]));
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    env.cx.update(|window, cx| {
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.set_text("g.V().count()", window, cx));
        editor
            .read(cx)
            .state()
            .clone()
            .update(cx, |s, cx| s.set_selected_range(13..13, cx));
    });
    let bar = ask(&mut env, &tab, "airports in Texas");
    wait_done(&mut env, &bar);
    support::click_settled(&mut env.cx, "prompt-insert");
    let text = env
        .cx
        .update(|_, cx| tab.read(cx).editor().read(cx).text(cx));
    assert_eq!(
        text,
        format!("g.V().count()\n{TEXAS}"),
        "a new statement after the cursor's line"
    );
    assert!(
        env.cx.update(|_, cx| tab.read(cx).prompt_bar().is_none()),
        "the card closes"
    );
}

#[gpui_kit::test]
fn scenario_nl_to_gremlin_run_from_card(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = RUNTIME.block_on(MockServer::start(vec![Route::sse(
        "/v1/chat/completions",
        &answer(TEXAS),
    )]));
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    let bar = ask(&mut env, &tab, "airports in Texas");
    wait_done(&mut env, &bar);
    support::click_settled(&mut env.cx, "prompt-run");
    env.cx.run_until_parked();
    wait_until(&mut env, "the run", |env| {
        env.cx.update(|_, cx| {
            Runner::global(cx)
                .read(cx)
                .latest()
                .is_some_and(|r| !r.read(cx).is_running())
        })
    });
    let (query, sent, rows) = env.cx.update(|_, cx| {
        let r = Runner::global(cx).read(cx).latest().unwrap().read(cx);
        (r.query.clone(), r.sent.clone(), r.items.len())
    });
    assert_eq!(query, TEXAS);
    assert!(
        !sent.contains("limit(25)"),
        "runs without the dry-run limit: {sent}"
    );
    assert!(rows > 25, "all Texas airports: {rows}");
}

#[gpui_kit::test]
fn scenario_nl_to_gremlin_refine_a_query(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = RUNTIME.block_on(MockServer::start(vec![Route::sse_sequence(
        "/v1/chat/completions",
        vec![answer(TEXAS), answer(TEXAS_RUNWAYS)],
    )]));
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    let bar = ask(&mut env, &tab, "airports in Texas");
    wait_done(&mut env, &bar);
    env.cx.update(|window, cx| {
        let refine = bar.read(cx).refine.clone();
        refine.update(cx, |i, cx| {
            i.set_value("only those with more than 3 runways", window, cx)
        });
        bar.update(cx, |b, cx| b.submit_refine(window, cx));
    });
    let b = bar.clone();
    wait_until(&mut env, "the refinement", move |env| {
        env.cx.update(|_, cx| {
            b.read(cx)
                .card
                .as_ref()
                .and_then(|c| c.outcome.as_ref())
                .is_some_and(|o| o.answer.query == TEXAS_RUNWAYS)
        })
    });
    let outcome = env
        .cx
        .update(|_, cx| bar.read(cx).card.as_ref().unwrap().outcome.clone())
        .unwrap();
    assert!(outcome.answer.query.contains("US-TX") && outcome.answer.query.contains("runways"));
    // The refinement request carried the first prompt and answer.
    let second: serde_json::Value = serde_json::from_str(&llm.requests()[1].body).unwrap();
    let messages = second["messages"].as_array().unwrap();
    let texts: Vec<String> = messages
        .iter()
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(texts.iter().any(|t| t == "airports in Texas"), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("US-TX")),
        "earlier answer in context"
    );
    assert_eq!(texts.last().unwrap(), "only those with more than 3 runways");
}

/// The app's `--ask` (screenshots, agents): a scripted prompt reaches the pipeline like a typed one.
#[gpui_kit::test]
fn scripted_ask_opens_the_bar_and_submits(cx: &mut TestAppContext) {
    if !testenv_up() {
        return;
    }
    let mut env = setup(cx);
    let llm = RUNTIME.block_on(MockServer::start(vec![Route::sse(
        "/v1/chat/completions",
        &answer(TEXAS),
    )]));
    profile(&mut env, &format!("{}/v1", llm.base));
    let id = connection(&mut env);
    let tab = tab(&mut env, id);
    env.cx
        .update(|window, cx| tab.update(cx, |t, cx| t.ask("airports in Texas", window, cx)));
    env.cx.run_until_parked();
    let bar = bar(&mut env, &tab);
    wait_done(&mut env, &bar);
    let (prompt, verified) = env.cx.update(|_, cx| {
        let card = bar.read(cx).card.as_ref().expect("card");
        (
            card.prompt.clone(),
            card.outcome.as_ref().is_some_and(|o| o.verified),
        )
    });
    assert_eq!(prompt, "airports in Texas");
    assert!(verified);
}
