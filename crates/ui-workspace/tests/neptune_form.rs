//! Headless UI test for the Neptune section of the connection dialog. Scenario names map to
//! `openspec/changes/provider-neptune-aws/specs/provider-neptune-aws/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, WindowOptions, px,
    size,
};
use providers::neptune::AwsFiles;
use providers::{ProviderConfig, ProviderKind};
use storage::Storage;
use storage::secrets::{MemoryStore, SecretStore};
use ui_workspace::connections::ConnectionManager;
use ui_workspace::connections::form::{self, ConnectionForm};
use ui_workspace::io;
use ui_workspace::{ShellConfig, Workspace, WorkspaceOptions};

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

fn setup(cx: &mut TestAppContext) -> VisualTestContext {
    cx.executor().allow_parking();
    let storage = Storage::open_in_memory().unwrap();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    cx.update(|cx| {
        gpui_kit::init(cx);
        io::install(RUNTIME.handle().clone(), cx);
        ui_workspace::init(
            ShellConfig {
                storage: Some(storage),
                theme_override: None,
                secrets: Some(secrets),
            },
            cx,
        );
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../providers/tests/fixtures/aws");
        ConnectionManager::global(cx).update(cx, |m, _| {
            m.set_aws_files(Some(AwsFiles {
                config: dir.join("config"),
                credentials: dir.join("credentials"),
            }))
        });
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
    vcx
}

fn text(
    form: &Entity<ConnectionForm>,
    field: fn(&ConnectionForm) -> &Entity<gpui_kit::component::input::InputState>,
    cx: &mut VisualTestContext,
) -> String {
    cx.update(|_, cx| field(form.read(cx)).read(cx).value().to_string())
}

/// Clear a field, then type into it one character at a time, like a user (not a paste).
fn type_into(
    form: &Entity<ConnectionForm>,
    field: fn(&ConnectionForm) -> &Entity<gpui_kit::component::input::InputState>,
    value: &str,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        let state = field(form.read(cx)).clone();
        state.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx)
        });
    });
    cx.run_until_parked();
    for c in value.chars() {
        let c = c.to_string();
        cx.update(move |window, cx| window.input(&c, cx));
    }
    cx.run_until_parked();
}

#[gpui_kit::test]
fn scenario_provider_neptune_aws_region_inferred(cx: &mut TestAppContext) {
    let mut cx = setup(cx);
    let form = cx.update(|window, cx| form::open(None, window, cx));
    cx.run_until_parked();
    support::click_settled(&mut cx, "conn-provider-neptune");
    type_into(
        &form,
        |f| &f.host,
        "mydb.cluster-abc123.us-east-2.neptune.amazonaws.com",
        &mut cx,
    );
    assert_eq!(text(&form, |f| &f.region, &mut cx), "us-east-2");
    assert_eq!(
        cx.update(|_, cx| form.read(cx).kind),
        ProviderKind::NeptuneAws
    );
}

#[gpui_kit::test]
fn neptune_section_profiles_and_tunnel(cx: &mut TestAppContext) {
    let mut cx = setup(cx);
    let form = cx.update(|window, cx| form::open(None, window, cx));
    cx.run_until_parked();
    support::click_settled(&mut cx, "conn-provider-neptune");
    type_into(&form, |f| &f.name, "Prod graph", &mut cx);
    type_into(
        &form,
        |f| &f.host,
        "db.cluster-xyz.eu-west-1.neptune.amazonaws.com",
        &mut cx,
    );
    // Profiles from the fixture AWS files, SSO ones marked.
    let names: Vec<String> = cx.update(|_, cx| {
        form.read(cx)
            .aws_profiles
            .iter()
            .map(|p| p.name.clone())
            .collect()
    });
    assert!(names.contains(&"sso-dev".to_owned()), "{names:?}");
    let shown = cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .within("aws-profiles")
            .try_find("aws-profile-sso-dev")
            .is_some()
    });
    assert!(shown, "profile chips render");
    support::click_settled(&mut cx, "aws-profile-analytics");
    type_into(&form, |f| &f.tunnel, "localhost:8182", &mut cx);
    let (config, _) = cx.update(|_, cx| form.read(cx).to_config(cx)).unwrap();
    let ProviderConfig::NeptuneAws(n) = config.provider else {
        panic!("not Neptune: {config:?}")
    };
    assert_eq!(n.region, "eu-west-1");
    assert_eq!(n.profile.as_deref(), Some("analytics"));
    assert_eq!(n.tunnel.as_deref(), Some("localhost:8182"));
    assert!(n.iam_auth && n.tls);
    // Back to the default chain.
    support::click_settled(&mut cx, "aws-profile-default-chain");
    let (config, _) = cx.update(|_, cx| form.read(cx).to_config(cx)).unwrap();
    let ProviderConfig::NeptuneAws(n) = config.provider else {
        panic!()
    };
    assert_eq!(n.profile, None);
}

#[gpui_kit::test]
async fn localstack_section_discovers_and_picks_a_cluster(cx: &mut TestAppContext) {
    let mut cx = setup(cx);
    let edge = RUNTIME.block_on(providers::testing::start_localstack(
        200,
        providers::testing::CLUSTERS_XML.into(),
        "us-east-1",
    ));
    let form = cx.update(|window, cx| form::open(None, window, cx));
    cx.run_until_parked();
    support::click_settled(&mut cx, "conn-provider-localstack");
    assert_eq!(
        text(&form, |f| &f.host, &mut cx),
        "localhost.localstack.cloud",
        "LocalStack defaults"
    );
    type_into(&form, |f| &f.name, "Local Neptune", &mut cx);
    type_into(&form, |f| &f.localstack_url, &edge.url(), &mut cx);
    support::click_settled(&mut cx, "ls-discover");
    for _ in 0..200 {
        cx.run_until_parked();
        let done = cx.update(|_, cx| {
            matches!(
                form.read(cx).ls_discovery,
                form::DiscoveryState::Done | form::DiscoveryState::Failed(_)
            )
        });
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        cx.update(|_, cx| matches!(form.read(cx).ls_discovery, form::DiscoveryState::Done)),
        "discovery finished"
    );
    support::click_settled(&mut cx, "ls-cluster-air-routes");
    let (config, _) = cx.update(|_, cx| form.read(cx).to_config(cx)).unwrap();
    let ProviderConfig::NeptuneLocalStack(l) = config.provider else {
        panic!("{config:?}")
    };
    assert_eq!(l.cluster_id.as_deref(), Some("air-routes"));
    assert_eq!(
        (l.host.as_str(), l.port),
        ("localhost.localstack.cloud", 4510)
    );
    assert_eq!(l.engine_version.as_deref(), Some("1.2.1.0"));
    assert_eq!(l.localstack_url, edge.url());
    // Manual entry instead.
    support::click_settled(&mut cx, "ls-manual");
    let (config, _) = cx.update(|_, cx| form.read(cx).to_config(cx)).unwrap();
    let ProviderConfig::NeptuneLocalStack(l) = config.provider else {
        panic!()
    };
    assert_eq!(l.cluster_id, None);
}
