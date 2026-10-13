//! Neptune on LocalStack without LocalStack Pro: recorded `DescribeDBClusters` responses served by a fake
//! LocalStack edge, and the fake Neptune for the emulated cluster. Scenario names map to
//! `openspec/changes/provider-neptune-localstack/specs/provider-neptune-localstack/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::time::Duration;

use providers::neptune::localstack::{self, Cluster, DiscoveryError, LocalStackProvider};
use providers::neptune::versions::tinkerpop_for;
use providers::testing::{self, CLUSTERS_XML, NO_CLUSTERS_XML, NOT_AVAILABLE_XML};
use providers::{FailureKind, GraphProvider, LocalStackConfig, ProviderError, ProviderKind};

fn config(url: &str) -> LocalStackConfig {
    LocalStackConfig {
        localstack_url: url.into(),
        ..Default::default()
    }
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

async fn fake_neptune(keys: HashMap<String, String>) -> testing::FakeNeptune {
    testing::start(testing::Options {
        tls_host: None,
        keys,
        region: "us-east-1".into(),
        // LocalStack's emulated clusters are plain TinkerPop servers without /status.
        status: false,
    })
    .await
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_list_clusters() {
    assert_eq!(
        localstack::parse_clusters(CLUSTERS_XML).unwrap(),
        vec![Cluster {
            id: "air-routes".into(),
            engine_version: "1.2.1.0".into(),
            endpoint: "localhost.localstack.cloud".into(),
            port: 4510,
            status: "available".into(),
            iam_auth: false,
        }],
        "the Neptune cluster only; aurora-postgresql is skipped"
    );
    // Discovery against a LocalStack edge: one signed Query-protocol call.
    let edge = testing::start_localstack(200, CLUSTERS_XML.into(), "us-east-1").await;
    let clusters = localstack::discover(&config(&edge.url())).await.unwrap();
    assert_eq!(clusters[0].port, 4510);
    let (seen, body) = &edge.requests()[0];
    assert_eq!(seen.method, "POST");
    assert!(body.contains("Action=DescribeDBClusters") && body.contains("neptune"));
    assert_eq!(
        seen.signature_error, None,
        "signed for rds with test credentials"
    );
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_localstack_not_running() {
    let url = format!("http://127.0.0.1:{}", closed_port());
    let err = localstack::discover(&config(&url)).await.unwrap_err();
    assert!(matches!(err, DiscoveryError::NotRunning { .. }), "{err:?}");
    let err = ProviderError::from(err);
    assert_eq!(err.kind, FailureKind::Unreachable);
    assert_eq!(err.reason, format!("LocalStack is not reachable at {url}."));
    assert!(err.suggestion.contains("localstack start"));
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_no_clusters() {
    let edge = testing::start_localstack(200, NO_CLUSTERS_XML.into(), "us-east-1").await;
    let err = ProviderError::from(
        localstack::discover(&config(&edge.url()))
            .await
            .unwrap_err(),
    );
    assert!(err.reason.contains("No Neptune clusters exist"), "{err:?}");
    assert!(
        err.suggestion
            .contains("awslocal neptune create-db-cluster --db-cluster-identifier"),
        "{}",
        err.suggestion
    );
}

#[tokio::test]
async fn neptune_not_in_this_localstack_edition() {
    let edge = testing::start_localstack(501, NOT_AVAILABLE_XML.into(), "us-east-1").await;
    let err = localstack::discover(&config(&edge.url()))
        .await
        .unwrap_err();
    assert!(
        matches!(err, DiscoveryError::NotAvailable { .. }),
        "{err:?}"
    );
    let err = ProviderError::from(err);
    assert!(err.suggestion.contains("LocalStack Pro"));
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_engine_to_tinkerpop_mapping() {
    assert_eq!(tinkerpop_for("1.2.1.0").as_deref(), Some("3.6.2"));
    // A picked cluster: discovery gives the port and engine, the connection reports TinkerPop from it.
    let neptune = fake_neptune(HashMap::new()).await;
    let xml = CLUSTERS_XML
        .replace("localhost.localstack.cloud", "127.0.0.1")
        .replace(
            "<Port>4510</Port>",
            &format!("<Port>{}</Port>", neptune.addr.port()),
        );
    let edge = testing::start_localstack(200, xml, "us-east-1").await;
    let mut cfg = config(&edge.url());
    cfg.cluster_id = Some("air-routes".into());
    cfg.port = 1; // stale; discovery replaces it
    let provider = LocalStackProvider::new(cfg, Duration::from_secs(5));
    assert_eq!(provider.kind(), ProviderKind::NeptuneLocalStack);
    let session = provider
        .connect()
        .await
        .expect("connects to the discovered port");
    assert_eq!(session.info.engine_version.as_deref(), Some("1.2.1.0"));
    assert_eq!(session.info.tinkerpop_version.as_deref(), Some("3.6.2"));
    assert!(session.capabilities.neptune);
    assert!(
        session
            .capabilities
            .restrictions
            .unsupported("asString")
            .is_some(),
        "3.7 string steps are flagged on 3.6.2"
    );
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_signed_requests_with_test_credentials() {
    let keys = HashMap::from([("test".to_owned(), "test".to_owned())]);
    let neptune = fake_neptune(keys).await;
    let cfg = LocalStackConfig {
        localstack_url: format!("http://127.0.0.1:{}", closed_port()),
        host: "127.0.0.1".into(),
        port: neptune.addr.port(),
        iam_auth: true,
        ..Default::default()
    };
    LocalStackProvider::new(cfg, Duration::from_secs(5))
        .connect()
        .await
        .expect("signed connection accepted");
    let handshake = neptune.seen().into_iter().find(|s| s.websocket).unwrap();
    assert_eq!(handshake.signature_error, None);
    assert_eq!(handshake.signed_by.as_deref(), Some("test"));
    assert!(handshake.headers["authorization"].contains("/us-east-1/neptune-db/aws4_request"));
}

#[tokio::test]
async fn scenario_provider_neptune_localstack_manual_port() {
    let neptune = fake_neptune(HashMap::new()).await;
    // The LocalStack URL points at nothing: discovery would fail, so connecting proves it is skipped.
    let cfg = LocalStackConfig {
        localstack_url: format!("http://127.0.0.1:{}", closed_port()),
        cluster_id: None,
        host: "localhost".into(),
        port: neptune.addr.port(),
        ..Default::default()
    };
    let session = LocalStackProvider::new(cfg, Duration::from_secs(5))
        .connect()
        .await
        .expect("connects directly");
    assert!(
        session
            .client
            .config()
            .url
            .contains(&format!("localhost:{}", neptune.addr.port()))
    );
    assert!(neptune.seen().iter().any(|s| s.websocket));
}
