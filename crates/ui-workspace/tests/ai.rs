//! The AI settings: the profile dialog (list models, type a model, test, save) and per-connection
//! profiles, against a scripted LLM server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use llm::testing::{MockServer, Route};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::ai::LlmProfiles;
use ui_workspace::ai::form::{Models, ProfileForm, TestUi};
use ui_workspace::ai::page;
use ui_workspace::io;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};
use uuid::Uuid;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../llm/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

struct Env {
    cx: VisualTestContext,
    storage: Storage,
    secrets: Arc<MemoryStore>,
}

fn setup(cx: &mut TestAppContext) -> Env {
    cx.executor().allow_parking();
    let storage = Storage::open_in_memory().unwrap();
    let secrets = Arc::new(MemoryStore::default());
    let shared: Arc<dyn SecretStore> = secrets.clone();
    let shell_storage = storage.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(shell_storage),
                theme_override: None,
                secrets: Some(shared),
            },
            cx,
        );
    });
    let (handle, _ws): (AnyWindowHandle, Entity<Workspace>) = cx
        .update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Workspace::new(WorkspaceOptions::default(), window, cx))
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(handle, cx);
    vcx.simulate_resize(size(px(1400.0), px(900.0)));
    Env {
        cx: vcx,
        storage,
        secrets,
    }
}

fn server(routes: Vec<Route>) -> MockServer {
    RUNTIME.block_on(MockServer::start(routes))
}

fn wait_until(env: &mut Env, what: &str, mut done: impl FnMut(&mut Env) -> bool) {
    let started = Instant::now();
    loop {
        env.cx.run_until_parked();
        if done(env) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn open_preset(env: &mut Env, preset: &str) -> Entity<ProfileForm> {
    let id = preset.to_owned();
    let form = env
        .cx
        .update(|window, cx| page::add_from_preset(&id, window, cx))
        .unwrap();
    env.cx.run_until_parked();
    form
}

fn set(
    env: &mut Env,
    form: &Entity<ProfileForm>,
    field: fn(&ProfileForm) -> &Entity<gpui_kit::component::input::InputState>,
    value: &str,
) {
    let value = value.to_owned();
    env.cx.update(|window, cx| {
        let input = field(form.read(cx)).clone();
        input.update(cx, |i, cx| i.set_value(value, window, cx));
    });
}

fn test_state(env: &mut Env, form: &Entity<ProfileForm>) -> TestUi {
    env.cx.update(|_, cx| form.read(cx).test.clone())
}

#[gpui_kit::test]
fn profile_dialog_streams_a_test_and_saves(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let llm = server(vec![Route::sse(
        "/v1/chat/completions",
        &fixture("openai-text.sse"),
    )]);
    let form = open_preset(&mut env, "ollama");
    set(
        &mut env,
        &form,
        |f| &f.base_url,
        &format!("{}/v1", llm.base),
    );
    set(&mut env, &form, |f| &f.model, "qwen2.5-coder");
    support::click_settled(&mut env.cx, "ai-test");
    wait_until(&mut env, "the test to finish", |env| {
        matches!(test_state(env, &form), TestUi::Done(..) | TestUi::Failed(_))
    });
    let TestUi::Done(text, summary) = test_state(&mut env, &form) else {
        panic!("{:?}", test_state(&mut env, &form))
    };
    assert_eq!(text, "Hello! Graph databases model networks.");
    assert!(
        summary.starts_with("First token ") && summary.contains("total "),
        "{summary}"
    );
    support::click_settled(&mut env.cx, "ai-save");
    wait_until(&mut env, "the save", |env| {
        env.cx
            .update(|_, cx| !LlmProfiles::global(cx).read(cx).profiles().is_empty())
    });
    let (profile, default) = env.cx.update(|_, cx| {
        let p = LlmProfiles::global(cx);
        (p.read(cx).profiles()[0].clone(), p.read(cx).default_id())
    });
    assert_eq!(profile.name, "Ollama");
    assert_eq!(profile.model, "qwen2.5-coder");
    assert_eq!(
        default,
        Some(profile.id),
        "the first profile becomes the default"
    );
    assert!(!env.cx.update(|window, cx| {
        use gpui_kit::component::WindowExt as _;
        window.has_active_dialog(cx)
    }));
}

#[gpui_kit::test]
fn invalid_key_shows_the_provider_message_without_the_key(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let llm = server(vec![Route::json(
        "POST",
        "/v1/chat/completions",
        401,
        &fixture("openai-401.json"),
    )]);
    let form = open_preset(&mut env, "openai");
    set(
        &mut env,
        &form,
        |f| &f.base_url,
        &format!("{}/v1", llm.base),
    );
    let key = "sk-test-abcdefghijklmnopWXYZ";
    set(&mut env, &form, |f| &f.key, key);
    env.cx
        .update(|_, cx| form.update(cx, |f, cx| f.run_test(cx)));
    wait_until(&mut env, "the test", |env| {
        matches!(test_state(env, &form), TestUi::Failed(_))
    });
    let TestUi::Failed(message) = test_state(&mut env, &form) else {
        unreachable!()
    };
    assert!(
        message.starts_with("authentication failed (401)"),
        "{message}"
    );
    assert!(message.contains("Incorrect API key provided"), "{message}");
    assert!(!message.contains("sk-test"), "{message}");
    // Nothing was saved.
    assert!(env.storage.list_llm_profiles_blocking().unwrap().is_empty());
}

#[gpui_kit::test]
fn models_are_listed_or_typed_by_hand(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let llm = server(vec![Route::json(
        "GET",
        "/v1/models",
        200,
        &fixture("ollama-models.json"),
    )]);
    let form = open_preset(&mut env, "ollama");
    set(
        &mut env,
        &form,
        |f| &f.base_url,
        &format!("{}/v1", llm.base),
    );
    support::click_settled(&mut env.cx, "ai-fetch-models");
    wait_until(&mut env, "the model list", |env| {
        env.cx
            .update(|_, cx| matches!(form.read(cx).models, Models::Listed(_)))
    });
    assert_eq!(
        env.cx.update(|_, cx| form.read(cx).models.clone()),
        Models::Listed(vec!["llama3.1:8b".into(), "qwen2.5-coder:7b".into()])
    );
    support::click_settled(&mut env.cx, "ai-model-0");
    let model = env
        .cx
        .update(|_, cx| form.read(cx).model.read(cx).value().to_string());
    assert_eq!(model, "llama3.1:8b");

    // Listing fails: the error shows, and a typed model saves.
    set(&mut env, &form, |f| &f.base_url, "http://127.0.0.1:1/v1");
    env.cx
        .update(|_, cx| form.update(cx, |f, cx| f.fetch_models(cx)));
    wait_until(&mut env, "the listing error", |env| {
        env.cx
            .update(|_, cx| matches!(form.read(cx).models, Models::Failed(_)))
    });
    set(&mut env, &form, |f| &f.model, "my-finetune:latest");
    support::click_settled(&mut env.cx, "ai-save");
    wait_until(&mut env, "the save", |env| {
        env.cx
            .update(|_, cx| !LlmProfiles::global(cx).read(cx).profiles().is_empty())
    });
    let saved = env
        .cx
        .update(|_, cx| LlmProfiles::global(cx).read(cx).profiles()[0].model.clone());
    assert_eq!(saved, "my-finetune:latest");
}

#[gpui_kit::test]
fn keys_go_to_the_keychain_and_connections_choose_profiles(cx: &mut TestAppContext) {
    let mut env = setup(cx);
    let form = open_preset(&mut env, "anthropic");
    let key = "sk-ant-api03-a-very-secret-key";
    set(&mut env, &form, |f| &f.key, key);
    support::click_settled(&mut env.cx, "ai-save");
    wait_until(&mut env, "the save", |env| {
        env.cx
            .update(|_, cx| !LlmProfiles::global(cx).read(cx).profiles().is_empty())
    });
    let cloud = env
        .cx
        .update(|_, cx| LlmProfiles::global(cx).read(cx).profiles()[0].clone());
    assert!(cloud.has_key);
    assert_eq!(
        env.secrets.get(&cloud.key_account()).unwrap().as_deref(),
        Some(key)
    );
    for row in env.storage.list_llm_profiles_blocking().unwrap() {
        assert!(!row.json.contains(key));
    }
    // A local profile, used only for "Prod".
    let mut local = llm::preset("ollama").unwrap().profile("Local");
    local.model = "llama3.1:8b".into();
    let task = env.cx.update(|_, cx| {
        LlmProfiles::global(cx).update(cx, |p, cx| p.save(local, llm::KeyChange::Keep, cx))
    });
    let local = env.cx.foreground_executor().block_test(task).unwrap();
    let prod = Uuid::new_v4();
    env.cx.update(|_, cx| {
        LlmProfiles::global(cx).update(cx, |p, cx| {
            p.set_connection_profile(prod, Some(local.id), cx)
        });
    });
    let (for_prod, for_other, share_prod) = env.cx.update(|_, cx| {
        let p = LlmProfiles::global(cx).read(cx);
        (
            p.resolve(Some(prod)).unwrap().id,
            p.resolve(Some(Uuid::new_v4())).unwrap().id,
            p.share_samples(prod),
        )
    });
    assert_eq!(for_prod, local.id);
    assert_eq!(for_other, cloud.id);
    assert!(share_prod, "local models get sample data by default");
    // Persisted for the next start (and the CLI).
    wait_until(&mut env, "the override to persist", |env| {
        let store = llm::ProfileStore::new(env.storage.clone(), env.secrets.clone());
        store
            .resolve(Some(prod))
            .unwrap()
            .is_some_and(|p| p.id == local.id)
    });
}
