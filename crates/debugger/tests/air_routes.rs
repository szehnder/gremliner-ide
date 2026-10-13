//! The generated queries on the testenv's air-routes graph (skipped when the testenv is down).
//! Scenario names map to `openspec/changes/query-debugger/specs/query-debugger/spec.md`.

#![allow(clippy::unwrap_used)]

use debugger::{ElementKind, IDS, Session};
use gremlin_client::{Client, ClientConfig, Request, require_testenv};
use gremlin_model::GValue;

const AUS: &str = "g.V().has('code','AUS').out('route').has('country','US').values('code')";

async fn run(c: &Client, q: &str, ids: Option<Vec<GValue>>) -> Vec<GValue> {
    let mut r = Request::new(q);
    if let Some(ids) = ids {
        r = r.binding(IDS, GValue::List(ids));
    }
    c.execute_request(r).await.unwrap().items
}

async fn code_of(c: &Client, code: &str) -> GValue {
    run(c, &format!("g.V().has('code','{code}').id()"), None).await[0].clone()
}

#[tokio::test]
async fn scenario_query_debugger_results_at_a_step() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let s = Session::start(AUS).unwrap();
    let all = run(&c, &format!("{}.count()", s.prefix(2)), None).await;
    let n = all[0].as_i64().unwrap();
    assert!(n > 50, "AUS has many routes: {n}");
    let limited = run(&c, &s.results_query(2, 10), None).await;
    assert_eq!(limited.len(), 10, "limited to the maximum");
    assert!(limited.iter().all(|v| matches!(v, GValue::Vertex(_))));
}

#[tokio::test]
async fn scenario_query_debugger_counts_after_each_step() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let s = Session::start(AUS).unwrap();
    let mut counts = Vec::new();
    for i in 0..s.steps().len() {
        counts.push(
            run(&c, &s.count_query(i, 100_000), None).await[0]
                .as_i64()
                .unwrap(),
        );
    }
    assert_eq!(counts[1], 1, "one airport after has('code','AUS')");
    assert!(counts[2] > 50, "its routes after out('route')");
    assert!(
        counts[3] < counts[2] && counts[3] > 0,
        "US destinations only"
    );
    assert_eq!(counts[4], counts[3]);
}

#[tokio::test]
async fn scenario_query_debugger_mark_failed_traversers() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let s = Session::start(AUS).unwrap();
    let at = run(&c, &format!("{}.id()", s.results_query(2, 1000)), None).await;
    let survivors = run(
        &c,
        &s.survivors_query(2, ElementKind::Vertex),
        Some(at.clone()),
    )
    .await;
    let lhr = code_of(&c, "LHR").await;
    let dfw = code_of(&c, "DFW").await;
    assert!(at.contains(&lhr) && at.contains(&dfw));
    assert!(
        survivors.contains(&dfw),
        "a US destination produces a result"
    );
    assert!(
        !survivors.contains(&lhr),
        "LHR is marked as not producing a result"
    );
}

#[tokio::test]
async fn scenario_query_debugger_find_where_an_element_drops() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let s = Session::start(AUS).unwrap();
    let lhr = code_of(&c, "LHR").await;
    let mut dropped = None;
    for (step, q) in s.inspection_queries(2, ElementKind::Vertex) {
        let n = run(&c, &q, Some(vec![lhr.clone()])).await[0]
            .as_i64()
            .unwrap();
        if n == 0 {
            dropped = Some(step);
            break;
        }
    }
    assert_eq!(dropped.map(|i| s.call_text(i)), Some("has('country','US')"));
}
