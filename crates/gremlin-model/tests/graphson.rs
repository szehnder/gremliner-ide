//! GraphSON v3 decoding against real Gremlin Server 3.7.3 responses (captured from the testenv into
//! `tests/fixtures/`), plus encode/decode round trips.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gremlin_model::graphson::{decode, encode};
use gremlin_model::{GKey, GValue, T};

fn fixture(name: &str) -> GValue {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    decode(&serde_json::from_str(&text).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn first(v: GValue) -> GValue {
    match v {
        GValue::List(mut items) => items.remove(0),
        other => other,
    }
}

const ALL: &[&str] = &[
    "vertex",
    "vertex_meta_properties",
    "edge",
    "path",
    "path_labels",
    "element_map",
    "value_map_tokens",
    "group_count_vertex_keys",
    "tree",
    "profile",
    "bulk_set",
    "scalars",
    "nulls_and_nan",
    "explain",
    "tokens",
    "vertex_property",
    "edge_property",
];

#[test]
fn snapshots() {
    for name in ALL {
        insta::assert_debug_snapshot!(*name, fixture(name));
    }
}

#[test]
fn round_trip_every_fixture() {
    for name in ALL {
        let v = fixture(name);
        let again = decode(&encode(&v)).unwrap();
        // NaN != NaN, so compare via Debug text for the fixture that contains it.
        assert_eq!(
            format!("{v:?}"),
            format!("{again:?}"),
            "{name} did not round-trip"
        );
    }
}

#[test]
fn vertex_fields() {
    let GValue::Vertex(v) = first(fixture("vertex")) else {
        panic!("not a vertex")
    };
    assert_eq!(*v.id, GValue::Int64(1));
    assert_eq!(v.label, "person");
    let name = &v.properties["name"][0];
    assert_eq!(*name.value, GValue::String("marko".into()));
    assert_eq!(*v.properties["age"][0].value, GValue::Int32(29));
}

#[test]
fn crew_multi_and_meta_properties() {
    let GValue::Vertex(v) = first(fixture("vertex_meta_properties")) else {
        panic!("not a vertex")
    };
    let locations = &v.properties["location"];
    assert!(locations.len() > 1, "multi-property");
    assert!(
        locations[0].properties.contains_key("startTime"),
        "meta-property"
    );
}

#[test]
fn edge_fields() {
    let GValue::Edge(e) = first(fixture("edge")) else {
        panic!("not an edge")
    };
    assert_eq!(e.label, "knows");
    assert_eq!(*e.out_v, GValue::Int64(1));
    assert_eq!(*e.in_v, GValue::Int64(2));
    assert_eq!(*e.properties["weight"].value, GValue::Double(0.5));
}

#[test]
fn element_map_has_t_keys_and_typed_values() {
    let GValue::Map(m) = first(fixture("element_map")) else {
        panic!("not a map")
    };
    assert_eq!(m[&GKey::T(T::Label)], GValue::String("person".into()));
    assert_eq!(m[&GKey::String("age".into())], GValue::Int32(29));
    assert_eq!(m[&GKey::T(T::Id)], GValue::Int64(1));
}

#[test]
fn group_count_supports_vertex_keys() {
    let GValue::Map(m) = first(fixture("group_count_vertex_keys")) else {
        panic!("not a map")
    };
    assert_eq!(m.len(), 2);
    assert!(
        m.keys()
            .all(|k| matches!(k, GKey::Vertex { label, .. } if label == "software"))
    );
}

#[test]
fn path_objects_are_vertices() {
    let GValue::Path(p) = first(fixture("path")) else {
        panic!("not a path")
    };
    assert_eq!(p.objects.len(), 3);
    assert!(p.objects.iter().all(|o| matches!(o, GValue::Vertex(_))));
    let GValue::Path(p) = first(fixture("path_labels")) else {
        panic!("not a path")
    };
    assert_eq!(p.labels[0], vec!["a".to_string()]);
}

#[test]
fn scalar_types_are_preserved() {
    let GValue::List(items) = first(fixture("scalars")) else {
        panic!("not a list")
    };
    let names: Vec<&str> = items.iter().map(GValue::type_name).collect();
    assert_eq!(
        names,
        [
            "Int32",
            "Int64",
            "Float",
            "Double",
            "String",
            "Boolean",
            "UUID",
            "Date",
            "BigDecimal",
            "BigInteger",
            "Byte",
            "Int16",
            "Char"
        ]
    );
}

#[test]
fn profile_metrics_tree() {
    let GValue::TraversalMetrics(m) = first(fixture("profile")) else {
        panic!("not metrics")
    };
    assert!(m.dur_ms > 0.0);
    assert!(!m.metrics.is_empty());
    assert!(
        m.metrics
            .iter()
            .all(|s| s.name.is_some() && s.counts.contains_key("traverserCount"))
    );
}

#[test]
fn unknown_types_are_preserved() {
    // TraversalExplanation has no model type; it must survive as Unknown, not fail.
    let v = first(fixture("explain"));
    assert!(
        matches!(
            v,
            GValue::Unknown { .. } | GValue::Map(_) | GValue::String(_)
        ),
        "{v:?}"
    );
}

#[test]
fn fast_decoder_matches_reference_on_every_fixture() {
    for name in ALL {
        let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap();
        let fast =
            gremlin_model::fast::decode_slice(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            format!("{fast:?}"),
            format!("{:?}", fixture(name)),
            "{name}: fast decoder differs"
        );
    }
}

#[test]
fn simd_decoder_matches_reference_on_every_fixture() {
    for name in ALL {
        let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let simd = gremlin_model::fast::decode_owned(std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            format!("{simd:?}"),
            format!("{:?}", fixture(name)),
            "{name}: simd decoder differs"
        );
    }
}
