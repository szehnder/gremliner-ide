//! Parameter values and schema-informed types. Scenario names map to
//! `openspec/changes/query-parameters/specs/query-parameters/spec.md`.

#![allow(clippy::unwrap_used)]

use gremlin_model::GValue;
use gremlin_syntax::params::{Hint, hint, parse_value};
use schema::Schema;

fn air() -> Schema {
    Schema::from_json(include_str!("fixtures/air-routes-schema.json")).unwrap()
}

#[test]
fn scenario_query_parameters_number_and_string_values() {
    assert_eq!(parse_value("\"AUS\"", None), GValue::String("AUS".into()));
    assert_eq!(parse_value("10", None), GValue::Int64(10));
    assert_eq!(parse_value("true", None), GValue::Bool(true));
    assert_eq!(parse_value("null", None), GValue::Null);
    assert_eq!(parse_value("2.5", None), GValue::Double(2.5));
    // Unquoted text that is not JSON is a string.
    assert_eq!(parse_value("AUS", None), GValue::String("AUS".into()));
    assert_eq!(
        parse_value("[1, \"a\"]", None),
        GValue::List(vec![GValue::Int64(1), GValue::String("a".into())])
    );
}

#[test]
fn schema_types_pick_int32_for_int32_properties() {
    let schema = air();
    let q = "g.V().has('airport','runways',gt(r)).has('code',code).limit(n)";
    assert_eq!(hint(q, "r", Some(&schema)), Some(Hint::Int32));
    assert_eq!(hint(q, "code", Some(&schema)), Some(Hint::Text));
    assert_eq!(hint(q, "n", Some(&schema)), None);
    assert_eq!(parse_value("3", Some(Hint::Int32)), GValue::Int32(3));
    assert_eq!(parse_value("3", Some(Hint::Double)), GValue::Double(3.0));
}
