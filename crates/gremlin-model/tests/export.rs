//! Data exports. Scenario names map to `openspec/changes/export/specs/export/spec.md`.

#![allow(clippy::unwrap_used)]

use gremlin_model::GValue;
use gremlin_model::export::json_lines;

#[test]
fn scenario_export_json_lines() {
    let items: Vec<GValue> = (0..6).map(GValue::Int64).collect();
    let text = json_lines(&items);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 6);
    for l in lines {
        serde_json::from_str::<serde_json::Value>(l).unwrap();
    }
}
