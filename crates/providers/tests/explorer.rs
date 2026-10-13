//! Graph Explorer searches on the testenv's air-routes graph (skipped when the testenv is down).

#![allow(clippy::unwrap_used)]

use gremlin_client::{Client, ClientConfig, Request, require_testenv};
use gremlin_model::GValue;
use gremlin_syntax::explorer::{Direction, ExplorerPath, Filter, Op, Step};
use schema::Schema;

fn air() -> Schema {
    Schema::from_json(include_str!(
        "../../gremlin-syntax/tests/fixtures/air-routes-schema.json"
    ))
    .unwrap()
}

async fn run(c: &Client, path: &ExplorerPath) -> Vec<GValue> {
    let g = path.generate(Some(&air())).unwrap();
    let mut r = Request::new(g.script);
    for (k, v) in g.bindings {
        r = r.binding(k, v);
    }
    c.execute_request(r).await.unwrap().items
}

#[tokio::test]
async fn scenario_graph_explorer_text_filter() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let mut path = ExplorerPath {
        steps: vec![Step::vertex(Some("airport"))],
        ..Default::default()
    };
    path.steps[0].filters_mut().push(Filter {
        key: "city".into(),
        op: Op::StartingWith,
        value: "San".into(),
    });
    let g = path.generate(Some(&air())).unwrap();
    assert!(g.script.contains("TextP.startingWith(p0)"), "{}", g.script);
    let paths = run(&c, &path).await;
    assert!(!paths.is_empty());
    // Paths carry references; look the cities up.
    let ids: Vec<GValue> = paths
        .iter()
        .map(|p| match p {
            GValue::Path(p) => p.objects[0].element_id().unwrap().clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    let cities = c
        .execute_request(Request::new("g.V(ids).values('city')").binding("ids", GValue::List(ids)))
        .await
        .unwrap()
        .items;
    assert_eq!(cities.len(), paths.len());
    for city in &cities {
        let city = city.as_str().unwrap();
        assert!(city.starts_with("San"), "{city}");
    }
}

#[tokio::test]
async fn scenario_graph_explorer_limit_applied() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url)).await.unwrap();
    let path = ExplorerPath {
        steps: vec![
            Step::vertex(Some("airport")),
            Step::edge(Some("route"), Direction::Out),
            Step::vertex(Some("airport")),
        ],
        limit: 50,
        randomize: true,
    };
    let paths = run(&c, &path).await;
    assert_eq!(
        paths.len(),
        50,
        "at most 50 paths (there are many more routes)"
    );
    assert!(
        paths
            .iter()
            .all(|p| matches!(p, GValue::Path(p) if p.objects.len() == 3))
    );
}
