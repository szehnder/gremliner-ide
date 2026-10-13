//! Graph Explorer query generation against the air-routes schema. Scenario names map to
//! `openspec/changes/graph-explorer/specs/graph-explorer/spec.md`.

#![allow(clippy::unwrap_used)]

use gremlin_model::GValue;
use gremlin_syntax::explorer::{
    Direction, ExplorerPath, Filter, Op, Step, ValueType, property_type,
};
use schema::Schema;

fn air() -> Schema {
    Schema::from_json(include_str!("fixtures/air-routes-schema.json")).unwrap()
}

#[test]
fn scenario_graph_explorer_build_a_two_hop_path() {
    let path = ExplorerPath {
        steps: vec![
            Step::vertex(Some("airport")),
            Step::edge(Some("route"), Direction::Out),
            Step::vertex(Some("airport")),
            Step::edge(Some("contains"), Direction::In),
            Step::vertex(Some("country")),
        ],
        ..Default::default()
    };
    let g = path.generate(Some(&air())).unwrap();
    assert_eq!(
        g.script,
        "g.V().hasLabel('airport').outE('route').inV().hasLabel('airport').inE('contains').outV().hasLabel('country').path().limit(200)"
    );
    assert!(g.bindings.is_empty());
}

#[test]
fn scenario_graph_explorer_numeric_filter_operators() {
    let schema = air();
    let step = Step::vertex(Some("airport"));
    let t = property_type(Some(&schema), &step, "runways");
    assert_eq!(t, ValueType::Integer);
    let ops = Op::for_type(t);
    for op in [Op::Gt, Op::Lt, Op::Between, Op::Inside, Op::Outside] {
        assert!(ops.contains(&op), "{op:?}");
    }
    assert!(!ops.contains(&Op::StartingWith));
    // Typed binding and a range predicate.
    let mut path = ExplorerPath {
        steps: vec![step],
        ..Default::default()
    };
    path.steps[0].filters_mut().push(Filter {
        key: "runways".into(),
        op: Op::Between,
        value: "3, 5".into(),
    });
    let g = path.generate(Some(&schema)).unwrap();
    assert_eq!(
        g.script,
        "g.V().has('airport', 'runways', P.between(p0, p1)).path().limit(200)"
    );
    assert_eq!(g.bindings[0].1, GValue::Int64(3));
    // A bad number is reported, not sent.
    path.steps[0].filters_mut()[0].value = "three, 5".into();
    assert!(
        path.generate(Some(&schema))
            .unwrap_err()
            .contains("not a whole number")
    );
}

#[test]
fn path_must_end_on_a_vertex() {
    let mut path = ExplorerPath::default();
    assert_eq!(path.problem(), None);
    path.push();
    assert!(path.problem().unwrap().contains("must end on a vertex"));
    assert!(path.generate(None).is_err());
    path.push();
    assert_eq!(path.problem(), None);
    assert!(
        path.warning().is_some(),
        "two 'any' vertices and an 'any' edge"
    );
}
