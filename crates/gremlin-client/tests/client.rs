//! Integration tests against the Docker testenv (ADR-0026). They skip when it is not running:
//! `cargo xtask testenv up && cargo xtask testenv seed`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use futures::StreamExt as _;
use gremlin_client::testenv::{self, AUTH_PASSWORD, AUTH_URL, AUTH_USER};
use gremlin_client::{Client, ClientConfig, Error, Request, require_testenv};
use gremlin_model::{GKey, GValue, T};

async fn modern(url: &str) -> Client {
    Client::connect(ClientConfig::new(url).alias("gModern"))
        .await
        .expect("connect")
}

fn single(items: Vec<GValue>) -> GValue {
    assert_eq!(items.len(), 1, "expected one item, got {items:?}");
    items.into_iter().next().unwrap()
}

#[tokio::test]
async fn scenario_gremlin_client_count_vertices() {
    let url = require_testenv!();
    let r = modern(&url)
        .await
        .execute("g.V().count()", Default::default())
        .await
        .unwrap();
    assert_eq!(single(r.items), GValue::Int64(6));
    assert!(r.elapsed > Duration::ZERO);
}

#[tokio::test]
async fn scenario_gremlin_client_bindings() {
    let url = require_testenv!();
    let c = modern(&url).await;
    let r = c
        .execute_request(
            Request::new("g.V().has('name', n).values('age')")
                .binding("n", GValue::String("marko".into())),
        )
        .await
        .unwrap();
    assert_eq!(single(r.items), GValue::Int32(29));
}

#[tokio::test]
async fn scenario_gremlin_client_vertex_with_properties() {
    let url = require_testenv!();
    let r = modern(&url)
        .await
        .execute("g.V().has('name','marko').elementMap()", Default::default())
        .await
        .unwrap();
    let GValue::Map(m) = single(r.items) else {
        panic!("expected a map")
    };
    assert_eq!(m[&GKey::T(T::Label)], GValue::String("person".into()));
    assert_eq!(
        m[&GKey::String("name".into())],
        GValue::String("marko".into())
    );
    assert_eq!(m[&GKey::String("age".into())], GValue::Int32(29));
    assert!(m.contains_key(&GKey::T(T::Id)));
}

#[tokio::test]
async fn scenario_gremlin_client_path_result() {
    let url = require_testenv!();
    let r = modern(&url)
        .await
        .execute("g.V(1).out().out().path()", Default::default())
        .await
        .unwrap();
    assert!(!r.items.is_empty());
    for item in r.items {
        let GValue::Path(p) = item else {
            panic!("expected a path")
        };
        assert!(
            p.objects
                .iter()
                .all(|o| matches!(o, GValue::Vertex(v) if !v.label.is_empty()))
        );
    }
}

#[tokio::test]
async fn scenario_gremlin_client_large_result_streams() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap(); // `g` = air-routes
    let expected = c
        .execute("g.E().count()", Default::default())
        .await
        .unwrap()
        .items[0]
        .as_i64()
        .unwrap();
    let mut stream = c
        .submit(Request::new("g.E()").batch_size(64))
        .await
        .unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(
        first.status, 206,
        "first batch arrives before the final one"
    );
    let mut total = first.items.len();
    let mut batches = 1;
    while let Some(b) = stream.next().await {
        total += b.unwrap().items.len();
        batches += 1;
    }
    assert_eq!(total as i64, expected);
    assert!(batches > 2);
}

#[tokio::test]
async fn scenario_gremlin_client_timeout() {
    let url = require_testenv!();
    let c = modern(&url).await;
    let err = c
        .execute_request(Request::new("Thread.sleep(3000); 1").timeout(Duration::from_millis(100)))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "got {err:?}");
    let r = c.execute("g.inject(1)", Default::default()).await.unwrap();
    assert_eq!(single(r.items), GValue::Int32(1));
}

#[tokio::test]
async fn scenario_gremlin_client_cancel() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let mut stream = c
        .submit(Request::new("g.E()").batch_size(64))
        .await
        .unwrap();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.status, 206);
    stream.cancel();
    assert!(stream.next().await.is_none(), "no batches after cancel");
    drop(stream);
    let r = c.execute("g.inject(1)", Default::default()).await.unwrap();
    assert_eq!(single(r.items), GValue::Int32(1));
}

#[tokio::test]
async fn scenario_gremlin_client_syntax_error() {
    let url = require_testenv!();
    let err = modern(&url)
        .await
        .execute("g.V().nosuchstep()", Default::default())
        .await
        .unwrap_err();
    match err {
        Error::Server { code, message, .. } => {
            assert!(code >= 500, "code {code}");
            assert!(message.contains("nosuchstep"), "message: {message}");
        }
        other => panic!("expected server error, got {other:?}"),
    }
}

#[tokio::test]
async fn scenario_gremlin_client_sasl_credentials() {
    let _ = require_testenv!(AUTH_URL);
    let c = Client::connect(ClientConfig::new(AUTH_URL).credentials(AUTH_USER, AUTH_PASSWORD))
        .await
        .unwrap();
    let r = c.execute("g.inject(1)", Default::default()).await.unwrap();
    assert_eq!(single(r.items), GValue::Int32(1));
}

#[tokio::test]
async fn scenario_gremlin_client_wrong_credentials() {
    let _ = require_testenv!(AUTH_URL);
    let secret = "not-the-password-xyz";
    let c = Client::connect(ClientConfig::new(AUTH_URL).credentials(AUTH_USER, secret))
        .await
        .unwrap();
    let err = c
        .execute("g.inject(1)", Default::default())
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "got {err:?}");
    assert!(
        !err.to_string().contains(secret),
        "error leaks the password: {err}"
    );
    assert!(!format!("{err:?}").contains(secret));
    assert!(
        !format!("{:?}", c.config()).contains(secret),
        "config Debug leaks the password"
    );
}

#[tokio::test]
async fn scenario_gremlin_client_concurrent_requests() {
    let url = require_testenv!();
    let c = modern(&url).await;
    let tasks: Vec<_> = (0..10)
        .map(|i| {
            let c = c.clone();
            tokio::spawn(async move {
                let r = c
                    .execute_request(Request::new("g.inject(x)").binding("x", GValue::Int64(i)))
                    .await
                    .unwrap();
                (i, r.items)
            })
        })
        .collect();
    for t in tasks {
        let (i, items) = t.await.unwrap();
        assert_eq!(
            items,
            vec![GValue::Int64(i)],
            "request {i} got someone else's result"
        );
    }
}

/// A TCP proxy we can kill to simulate the server going away.
async fn proxy(listen: std::net::SocketAddr, target: String) -> tokio::task::JoinHandle<()> {
    let listener = tokio::net::TcpListener::bind(listen).await.unwrap();
    tokio::spawn(async move {
        let mut conns = tokio::task::JoinSet::new();
        loop {
            let Ok((mut inbound, _)) = listener.accept().await else {
                return;
            };
            let target = target.clone();
            conns.spawn(async move {
                if let Ok(mut outbound) = tokio::net::TcpStream::connect(target).await {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                }
            });
        }
    })
}

#[tokio::test]
async fn scenario_gremlin_client_reconnect_after_server_restart() {
    let url = require_testenv!();
    let target = url
        .trim_start_matches("ws://")
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let p = proxy(addr, target.clone()).await;
    let c = Client::connect(ClientConfig {
        pool_size: 1,
        ..ClientConfig::new(format!("ws://{addr}/gremlin")).alias("gModern")
    })
    .await
    .unwrap();
    assert_eq!(
        c.execute("g.inject(1)", Default::default())
            .await
            .unwrap()
            .items,
        vec![GValue::Int32(1)]
    );

    // "Server restart": kill the proxy and all its connections, then bring it back on the same port.
    p.abort();
    let _ = p.await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let first = c.execute("g.inject(1)", Default::default()).await;
    let _p = proxy(addr, target).await;
    let r = match first {
        Ok(r) => r, // already reconnected
        Err(e) => {
            assert!(e.is_connection_error(), "unexpected error {e:?}");
            c.execute("g.inject(2)", Default::default())
                .await
                .expect("reconnects automatically")
        }
    };
    assert_eq!(r.items.len(), 1);
}

#[test]
fn testenv_helper_is_wired() {
    assert!(testenv::url().starts_with("ws://"));
}
