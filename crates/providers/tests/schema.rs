//! Schema discovery against the Docker testenv (skipped with a message when it is not running).

#![allow(clippy::unwrap_used)]

use std::time::Instant;

use gremlin_client::require_testenv;
use providers::schema::discover_schema;
use providers::{
    ConnectionConfig, GremlinServerConfig, ProviderConfig, Session, endpoint, provider_for,
};
use schema::{Cardinality, Options, Schema, Triple};

async fn session(url: &str, alias: &str) -> Session {
    let parsed = endpoint::parse(url).unwrap();
    let config = ConnectionConfig::new(
        "schema-test",
        ProviderConfig::GremlinServer(GremlinServerConfig {
            host: parsed.host,
            port: parsed.port.unwrap(),
            alias: alias.into(),
            ..Default::default()
        }),
    );
    provider_for(&config, None)
        .unwrap()
        .connect()
        .await
        .unwrap()
}

async fn discover(url: &str, alias: &str) -> Schema {
    let session = session(url, alias).await;
    discover_schema(&session, &Options::default(), &|_| {}).await
}

#[tokio::test]
async fn scenario_schema_discovery_modern_graph_schema() {
    let url = require_testenv!();
    let schema = discover(&url, "gModern").await;
    assert!(!schema.partial.any(), "{schema:#?}");
    let names = |l: &[schema::LabelInfo]| l.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&schema.vertex_labels), ["person", "software"]);
    assert_eq!(names(&schema.edge_labels), ["created", "knows"]);
    let person = schema.vertex_label("person").unwrap();
    assert_eq!(person.property("name").unwrap().type_text(), "String");
    assert_eq!(person.property("age").unwrap().type_text(), "Int32");
    assert_eq!(person.count.unwrap().value, 4);
    assert!(
        schema
            .triples
            .contains(&Triple::new("person", "knows", "person"))
    );
    assert!(
        schema
            .triples
            .contains(&Triple::new("person", "created", "software"))
    );
    let knows = schema.edge_label("knows").unwrap();
    assert_eq!(knows.property("weight").unwrap().type_text(), "Double");
}

#[tokio::test]
async fn scenario_schema_discovery_sample_values() {
    let url = require_testenv!();
    let started = Instant::now();
    let session = session(&url, "g").await;
    let first_labels = std::sync::Mutex::new(None);
    let schema = discover_schema(&session, &Options::default(), &|s| {
        let mut first = first_labels.lock().unwrap();
        if first.is_none() && !s.vertex_labels.is_empty() {
            *first = Some(started.elapsed());
        }
    })
    .await;
    let total = started.elapsed();
    let airport = schema.vertex_label("airport").unwrap();
    let code = airport.property("code").unwrap();
    assert!(
        !code.samples.is_empty() && code.samples.len() <= 5,
        "{code:?}"
    );
    assert!(code.samples.iter().all(|s| s.chars().count() <= 64));
    assert_eq!(code.type_text(), "String");
    assert_eq!(airport.count.unwrap().value, 3504);
    let route = schema.edge_label("route").unwrap();
    assert!(route.count.unwrap().value > 50_000);
    // Design budgets: labels in under a second, everything in under five.
    let labels_at = first_labels.lock().unwrap().unwrap();
    assert!(labels_at.as_secs_f32() < 1.0, "labels after {labels_at:?}");
    assert!(total.as_secs_f32() < 5.0, "full schema after {total:?}");
    eprintln!("air-routes: labels after {labels_at:?}, complete after {total:?}");
    for t in [
        Triple::new("airport", "route", "airport"),
        Triple::new("country", "contains", "airport"),
        Triple::new("continent", "contains", "airport"),
    ] {
        assert!(
            schema.triples.contains(&t),
            "missing {t:?}: {:?}",
            schema.triples
        );
    }
}

#[tokio::test]
async fn crew_multi_properties_are_flagged() {
    let url = require_testenv!();
    let schema = discover(&url, "gCrew").await;
    let person = schema.vertex_label("person").unwrap();
    assert_eq!(
        person.property("location").unwrap().cardinality,
        Cardinality::Multi
    );
    assert_eq!(
        person.property("name").unwrap().cardinality,
        Cardinality::Single
    );
}
