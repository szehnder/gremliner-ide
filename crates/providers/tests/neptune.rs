//! Neptune provider without AWS: fixture AWS files, a fake Neptune that verifies SigV4 like the real one,
//! and the recorded `/status` response. Scenario names map to
//! `openspec/changes/provider-neptune-aws/specs/provider-neptune-aws/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use aws_credential_types::Credentials;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider, future};
use gremlin_client::Request;
use providers::neptune::{
    AwsFiles, CredentialCache, NeptuneProvider, NeptuneStatus, list_profiles,
};
use providers::{FailureKind, GraphProvider, NeptuneConfig};

const HOST: &str = "mydb.cluster-abc123.us-east-2.neptune.amazonaws.com";
const REGION: &str = "us-east-2";

fn fixture_files() -> AwsFiles {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aws");
    AwsFiles {
        config: dir.join("config"),
        credentials: dir.join("credentials"),
    }
}

/// Hands out `AKID1`, `AKID2`, ... each valid for `ttl`, with a session token like STS/SSO credentials.
#[derive(Debug)]
struct Rotating {
    issued: AtomicUsize,
    ttl: Duration,
}

impl Rotating {
    fn keys(n: usize) -> HashMap<String, String> {
        (1..=n)
            .map(|i| (format!("AKID{i}"), format!("secret-{i}")))
            .collect()
    }
}

impl ProvideCredentials for Rotating {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        let i = self.issued.fetch_add(1, Ordering::SeqCst) + 1;
        future::ProvideCredentials::ready(Ok(Credentials::new(
            format!("AKID{i}"),
            format!("secret-{i}"),
            Some(format!("session-token-{i}")),
            Some(SystemTime::now() + self.ttl),
            "rotating",
        )))
    }
}

fn cache(ttl: Duration, window: Duration) -> (Arc<CredentialCache>, Arc<Rotating>) {
    let rotating = Arc::new(Rotating {
        issued: AtomicUsize::new(0),
        ttl,
    });
    let provider = SharedCredentialsProvider::new(RotatingRef(rotating.clone()));
    (
        Arc::new(CredentialCache::new(provider, None, false).refresh_before(window)),
        rotating,
    )
}

#[derive(Debug)]
struct RotatingRef(Arc<Rotating>);

impl ProvideCredentials for RotatingRef {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        self.0.provide_credentials()
    }
}

fn config_for(server: &providers::testing::FakeNeptune, tls: bool) -> NeptuneConfig {
    NeptuneConfig {
        host: HOST.into(),
        port: 8182,
        region: String::new(),
        iam_auth: true,
        profile: None,
        tunnel: Some(server.addr.to_string()),
        accept_invalid_certs: true,
        tls,
    }
}

#[test]
fn scenario_provider_neptune_aws_profiles_listed() {
    let profiles = list_profiles(&fixture_files());
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["default", "analytics", "ci-keys", "legacy-sso", "sso-dev"],
        "config and credentials profiles, default first; sso-session and services sections are not profiles"
    );
    let by = |n: &str| profiles.iter().find(|p| p.name == n).unwrap();
    assert!(by("sso-dev").sso && by("legacy-sso").sso);
    assert!(!by("analytics").sso && !by("ci-keys").sso);
    assert_eq!(by("analytics").region.as_deref(), Some("eu-west-2"));
    // Missing files list nothing rather than failing.
    let none = AwsFiles {
        config: "/nonexistent/config".into(),
        credentials: "/nonexistent/credentials".into(),
    };
    assert!(list_profiles(&none).is_empty());
}

#[tokio::test]
async fn scenario_provider_neptune_aws_expired_sso_session() {
    let cache =
        CredentialCache::resolve(Some("sso-dev"), Some(REGION), Some(&fixture_files())).await;
    let err = cache.get().await.unwrap_err();
    assert_eq!(err.kind, FailureKind::AuthExpired, "{err:?}");
    assert!(
        err.suggestion.contains("aws sso login --profile sso-dev"),
        "{}",
        err.suggestion
    );
    // Connecting reports the same, before any handshake.
    let config = NeptuneConfig {
        host: HOST.into(),
        profile: Some("sso-dev".into()),
        tunnel: Some("127.0.0.1:9".into()),
        ..Default::default()
    };
    let provider = NeptuneProvider::new(config, Duration::from_secs(5)).with_files(fixture_files());
    let err = provider.connect().await.unwrap_err();
    assert_eq!(err.kind, FailureKind::AuthExpired);
}

#[tokio::test]
async fn static_profile_credentials_resolve() {
    let cache =
        CredentialCache::resolve(Some("ci-keys"), Some(REGION), Some(&fixture_files())).await;
    assert_eq!(cache.get().await.unwrap().access_key_id(), "AKIDFIXTURECI");
    let missing =
        CredentialCache::resolve(Some("nope"), Some(REGION), Some(&fixture_files())).await;
    let err = missing.get().await.unwrap_err();
    assert_eq!(err.kind, FailureKind::Auth, "{err:?}");
    assert!(err.reason.contains("\"nope\""));
}

#[tokio::test]
async fn scenario_provider_neptune_aws_signed_handshake() {
    let server = providers::testing::start(providers::testing::Options {
        tls_host: None,
        keys: Rotating::keys(3),
        region: REGION.into(),
        status: true,
    })
    .await;
    let (creds, _) = cache(Duration::from_secs(3600), Duration::ZERO);
    let provider = NeptuneProvider::new(config_for(&server, false), Duration::from_secs(5))
        .with_credentials(creds);
    let session = provider
        .connect()
        .await
        .expect("connects with a valid signature");
    let seen = server.seen();
    let ws = seen.iter().find(|s| s.websocket).expect("a handshake");
    assert_eq!(ws.signature_error, None);
    assert_eq!(ws.signed_by.as_deref(), Some("AKID1"));
    for header in ["authorization", "x-amz-date", "x-amz-security-token"] {
        assert!(ws.headers.contains_key(header), "missing {header}");
    }
    assert_eq!(ws.headers["x-amz-security-token"], "session-token-1");
    // The HTTP status call is signed too, and its answer fills in the server info.
    let status = seen
        .iter()
        .find(|s| s.path == "/status")
        .expect("status read");
    assert_eq!(status.signed_by.as_deref(), Some("AKID1"));
    assert_eq!(session.info.engine_version.as_deref(), Some("1.4.5.0.R2"));
    assert_eq!(session.info.tinkerpop_version.as_deref(), Some("3.7.3"));
    assert!(session.capabilities.neptune && session.api.is_some());
}

#[tokio::test]
async fn bad_signature_is_explained() {
    let server = providers::testing::start(providers::testing::Options {
        tls_host: None,
        keys: HashMap::from([("AKID1".to_owned(), "a-different-secret".to_owned())]),
        region: REGION.into(),
        status: true,
    })
    .await;
    let (creds, _) = cache(Duration::from_secs(3600), Duration::ZERO);
    let provider = NeptuneProvider::new(config_for(&server, false), Duration::from_secs(5))
        .with_credentials(creds);
    let err = provider.connect().await.unwrap_err();
    assert_eq!(err.kind, FailureKind::Auth, "{err:?}");
    assert!(err.suggestion.contains("neptune-db:connect"), "{err:?}");
}

#[tokio::test]
async fn scenario_provider_neptune_aws_connect_through_localhost_tunnel() {
    let server = providers::testing::start(providers::testing::Options {
        tls_host: Some(HOST.into()),
        keys: Rotating::keys(3),
        region: REGION.into(),
        status: true,
    })
    .await;
    let (creds, _) = cache(Duration::from_secs(3600), Duration::ZERO);
    let config = config_for(&server, true);
    assert!(config.tunnel_addr().unwrap().starts_with("127.0.0.1:"));
    let provider = NeptuneProvider::new(config, Duration::from_secs(5)).with_credentials(creds);
    let session = provider
        .connect()
        .await
        .expect("connects through the tunnel");
    let seen = server.seen();
    assert!(seen.len() >= 2, "{seen:?}");
    for request in &seen {
        assert_eq!(request.sni.as_deref(), Some(HOST), "TLS SNI is the cluster");
        assert_eq!(
            request.headers["host"],
            format!("{HOST}:8182"),
            "Host is the cluster"
        );
        assert_eq!(request.signature_error, None, "signed for the cluster host");
    }
    assert!(session.capabilities.statistics);
}

#[tokio::test]
async fn scenario_provider_neptune_aws_refresh_before_expiry() {
    let server = providers::testing::start(providers::testing::Options {
        tls_host: None,
        keys: Rotating::keys(5),
        region: REGION.into(),
        status: true,
    })
    .await;
    let (creds, rotating) = cache(Duration::from_millis(600), Duration::ZERO);
    let mut config = config_for(&server, false);
    config.tls = false;
    let provider = NeptuneProvider::new(config, Duration::from_secs(5)).with_credentials(creds);
    let session = provider.connect().await.unwrap();
    let first = server
        .seen()
        .iter()
        .find(|s| s.websocket)
        .unwrap()
        .signed_by
        .clone();
    // The credentials expire during the session; then the connections drop.
    tokio::time::sleep(Duration::from_millis(800)).await;
    server.drop_connections();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let result = session
        .client
        .execute_request(Request::new("g.inject(1)"))
        .await
        .expect("reconnects without prompting");
    assert_eq!(result.items.len(), 1);
    let handshakes: Vec<_> = server.seen().into_iter().filter(|s| s.websocket).collect();
    assert!(handshakes.len() >= 2, "{handshakes:?}");
    assert!(handshakes.iter().all(|h| h.signature_error.is_none()));
    let last = handshakes.last().unwrap();
    assert_ne!(
        last.signed_by, first,
        "re-signed with refreshed credentials"
    );
    assert!(rotating.issued.load(Ordering::SeqCst) >= 2);
}

#[test]
fn scenario_provider_neptune_aws_status_parsed() {
    let status = NeptuneStatus::parse(providers::testing::STATUS).unwrap();
    assert_eq!(status.engine_version, "1.4.5.0.R2");
    assert_eq!(status.tinkerpop_version.as_deref(), Some("3.7.3"));
    assert_eq!(status.role.as_deref(), Some("writer"));
    assert_eq!(status.iam_auth, Some(true));
    assert_eq!(status.query_timeout_ms, Some(120_000));
    let caps = providers::Capabilities::neptune(status.tinkerpop_version.as_deref(), true);
    assert!(caps.profile && caps.explain && caps.statistics);
    assert!(NeptuneStatus::parse(r#"{"status":"ok"}"#).is_err());
}

#[test]
fn scenario_provider_neptune_aws_lambda_flagged() {
    let caps = providers::Capabilities::neptune(Some("3.7.3"), true);
    let doc = gremlin_syntax::Document::parse("g.V().map{ it.get().value('code') }.toList()");
    let issues = gremlin_syntax::dialect::restriction_issues(&doc, &caps.restrictions);
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0]
            .message
            .contains("Neptune does not support lambdas")
    );
    // Gremlin Server allows them.
    let server = providers::Capabilities::gremlin_server(Some("3.7.3"));
    assert!(gremlin_syntax::dialect::restriction_issues(&doc, &server.restrictions).is_empty());
}

/// The Docker Gremlin Server as a Neptune stand-in (ADR-0026): no IAM, no TLS, no `/status`. Connecting
/// still works; Neptune restrictions apply and the HTTP features are simply absent.
#[tokio::test]
async fn neptune_provider_on_the_testenv_stand_in() {
    let url = gremlin_client::require_testenv!();
    let parsed = providers::endpoint::parse(&url).unwrap();
    let config = NeptuneConfig {
        host: parsed.host,
        port: parsed.port.unwrap_or(8182),
        iam_auth: false,
        tls: false,
        ..Default::default()
    };
    let session = NeptuneProvider::new(config, Duration::from_secs(10))
        .connect()
        .await
        .expect("connects");
    assert!(session.capabilities.neptune && !session.capabilities.restrictions.lambdas);
    assert!(session.api.is_none() && session.info.engine_version.is_none());
    assert!(
        session.schema_summary().await.is_none(),
        "falls back to sampling"
    );
    let r = session
        .client
        .execute_request(Request::new("g.V().limit(1).count()"))
        .await
        .unwrap();
    assert_eq!(r.items.len(), 1);
}
