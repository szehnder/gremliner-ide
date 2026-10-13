//! GraphML round trip through a real TinkerGraph (testenv). Scenario names map to
//! `openspec/changes/export/specs/export/spec.md`.

#![allow(clippy::unwrap_used)]

use gremlin_client::{Client, ClientConfig, Request, require_testenv};
use gremlin_model::GValue;
use gremlin_model::elements::elements;
use gremlin_model::export::graphml;

/// A canonical description of a graph: sorted vertex and edge lines with labels and properties.
const DESCRIBE: &str = r#"
def describe(g) {
  def vs = g.V().map { v ->
    def props = v.get().properties().toList().collect { it.key() + '=' + it.value() }.sort().join(',')
    v.get().label() + '{' + props + '}'
  }.toList().sort()
  def es = g.E().map { e ->
    def x = e.get()
    def props = x.properties().toList().collect { it.key() + '=' + it.value() }.sort().join(',')
    x.outVertex().value('name') + '-' + x.label() + '->' + x.inVertex().value('name') + '{' + props + '}'
  }.toList().sort()
  return vs + es
}
"#;

#[tokio::test]
async fn scenario_export_graphml_round_trip() {
    let url = require_testenv!();
    let c = Client::connect(ClientConfig::new(&url).alias("gModern"))
        .await
        .unwrap();
    let mut items = c
        .execute_request(Request::new("g.V()"))
        .await
        .unwrap()
        .items;
    items.extend(
        c.execute_request(Request::new("g.E()"))
            .await
            .unwrap()
            .items,
    );
    let set = elements(&items);
    assert_eq!((set.vertices.len(), set.edges.len()), (6, 6));
    let xml = graphml(&set);
    // Load the export into an empty TinkerGraph on the server and describe both graphs the same way.
    let script = format!(
        "{DESCRIBE}
        def tg = org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph.open()
        org.apache.tinkerpop.gremlin.structure.io.graphml.GraphMLReader.build().create()
            .readGraph(new ByteArrayInputStream(xml.getBytes('UTF-8')), tg)
        [describe(tg.traversal()), describe(g)]"
    );
    let raw = gremlin_client::Client::connect(ClientConfig::new(&url).alias("gModern"))
        .await
        .unwrap()
        .execute_request(Request::new(script).binding("xml", GValue::String(xml)))
        .await
        .unwrap()
        .items;
    let lists: Vec<Vec<String>> = raw
        .iter()
        .map(|l| match l {
            GValue::List(items) => items
                .iter()
                .map(|i| i.as_str().unwrap().to_owned())
                .collect(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(lists.len(), 2, "{raw:?}");
    // Integer properties come back as the GraphML type that was written (int), so values match exactly.
    assert_eq!(
        lists[0], lists[1],
        "same vertices, edges, labels and properties"
    );
    assert_eq!(lists[0].len(), 12);
}
