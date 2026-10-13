//! Integration tests against the Docker testenv (skipped with a message when it is not running).

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use gremlin_client::require_testenv;
use gremlin_client::testenv::{AUTH_PASSWORD, AUTH_URL, AUTH_USER};
use providers::endpoint;
use providers::{
    ConnectionConfig, FailureKind, GremlinServerConfig, ProviderConfig, gremlin_server,
    provider_for, test_connection,
};

fn config_for(url: &str, alias: &str) -> ConnectionConfig {
    let parsed = endpoint::parse(url).unwrap();
    ConnectionConfig::new(
        "test",
        ProviderConfig::GremlinServer(GremlinServerConfig {
            host: parsed.host,
            port: parsed.port.unwrap(),
            alias: alias.into(),
            ..Default::default()
        }),
    )
}

#[tokio::test]
async fn scenario_provider_gremlin_server_detect_tinkerpop_version() {
    let url = require_testenv!();
    let config = config_for(&url, "g");
    let session = provider_for(&config, None)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let version = session.info.tinkerpop_version.clone().unwrap();
    assert!(version.starts_with("3.7."), "{version}");
    assert!(session.capabilities.profile && session.capabilities.explain);
    assert!(session.capabilities.script_eval);
}

#[tokio::test]
async fn scenario_provider_gremlin_server_use_gmodern_alias() {
    let url = require_testenv!();
    let session = provider_for(&config_for(&url, "gModern"), None)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let count = gremlin_server::execute(&session, "g.V().count()")
        .await
        .unwrap();
    assert_eq!(count[0].as_i64(), Some(6));
}

#[tokio::test]
async fn scenario_connections_successful_test() {
    let url = require_testenv!();
    let report = test_connection(&config_for(&url, "g"), None).await;
    let info = report.result.unwrap();
    assert!(info.latency < Duration::from_secs(5));
    assert!(info.tinkerpop_version.is_some());
}

#[tokio::test]
async fn scenario_connections_unreachable_host() {
    // A port where nothing listens: bind, note the port, close.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut config = config_for(&format!("ws://127.0.0.1:{port}/gremlin"), "g");
    config.timeout_ms = 3_000;
    let started = Instant::now();
    let report = test_connection(&config, None).await;
    let err = report.result.unwrap_err();
    assert_eq!(err.kind, FailureKind::Unreachable, "{err:?}");
    assert_eq!(
        err.reason,
        format!("Nothing is listening on 127.0.0.1:{port}.")
    );
    assert!(!err.suggestion.is_empty());
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn authenticates_and_reports_bad_passwords() {
    require_testenv!(AUTH_URL);
    let mut config = config_for(AUTH_URL, "g");
    if let ProviderConfig::GremlinServer(c) = &mut config.provider {
        c.username = Some(AUTH_USER.into());
        c.has_password = true;
    }
    assert!(
        test_connection(&config, Some(AUTH_PASSWORD.into()))
            .await
            .result
            .is_ok()
    );
    let bad = test_connection(&config, Some("wrong-password".into())).await;
    assert_eq!(bad.result.unwrap_err().kind, FailureKind::Auth);
}

fn modern_session() -> Option<(tokio::runtime::Runtime, providers::Session)> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = gremlin_client::testenv::url();
    if !gremlin_client::testenv::is_up(&url) {
        eprintln!("SKIPPED: testenv not reachable at {url}");
        return None;
    }
    let p = providers::endpoint::parse(&url).unwrap();
    let config = providers::GremlinServerConfig {
        host: p.host,
        port: p.port.unwrap_or(8182),
        alias: "gModern".into(),
        ..Default::default()
    };
    let provider = providers::gremlin_server::GremlinServerProvider::new(
        config,
        None,
        Duration::from_secs(10),
    );
    let session = rt
        .block_on(providers::GraphProvider::connect(&provider))
        .unwrap();
    Some((rt, session))
}

#[test]
fn scenario_query_profile_explain_profile_on_gremlin_server() {
    let Some((rt, session)) = modern_session() else {
        return;
    };
    let report = rt
        .block_on(providers::analysis::profile(
            &session,
            "g.V().out().out().path()",
            Duration::from_secs(10),
        ))
        .unwrap();
    assert!(report.steps.len() >= 3, "{report:?}");
    assert!(
        report
            .steps
            .iter()
            .all(|s| s.traversers.is_some() && s.count.is_some())
    );
    let sum: f64 = report.steps.iter().map(|s| s.pct).sum();
    assert!((sum - 100.0).abs() < 1.0, "percentages sum to {sum}");
    assert!(report.steps.iter().any(|s| s.name.starts_with("PathStep")));
    // The final step produced the two paths.
    assert_eq!(report.steps.last().and_then(|s| s.count), Some(2));
}

#[test]
fn scenario_query_profile_explain_explain_on_gremlin_server() {
    let Some((rt, session)) = modern_session() else {
        return;
    };
    let report = rt
        .block_on(providers::analysis::explain(
            &session,
            "g.V().hasLabel('person').count()",
            Duration::from_secs(10),
        ))
        .unwrap();
    assert!(report.strategies.len() > 3, "{report:?}");
    assert!(
        report
            .strategies
            .iter()
            .any(|s| s.category == "optimization")
    );
    assert!(report.original.as_deref().unwrap().contains("HasStep"));
    let last = report.final_traversal.unwrap();
    assert!(
        last.contains("CountGlobalStep") || last.contains("Count"),
        "{last}"
    );
}
