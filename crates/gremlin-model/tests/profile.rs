//! Neptune profile and explain parsing from recorded responses. Scenario names map to
//! `openspec/changes/query-profile-explain/specs/query-profile-explain/spec.md`.

#![allow(clippy::unwrap_used)]

use gremlin_model::profile::{ExplainReport, ProfileReport};

#[test]
fn scenario_query_profile_explain_parse_neptune_profile() {
    let r = ProfileReport::from_neptune_text(include_str!(
        "../../../testenv/fixtures/neptune/profile-1.2.txt"
    ));
    let optimized = r.optimized.as_deref().unwrap();
    assert!(
        optimized.contains("NeptuneGraphQueryStep(PropertyValue)"),
        "{optimized}"
    );
    assert_eq!(r.steps.len(), 2);
    let first = &r.steps[0];
    assert_eq!(first.name, "NeptuneGraphQueryStep(PropertyValue)");
    assert_eq!((first.count, first.traversers), (Some(59), Some(59)));
    assert!((first.dur_ms - 2.010).abs() < 1e-9 && (first.pct - 70.12).abs() < 1e-9);
    assert!((r.total_ms - 2.867).abs() < 1e-9);
    assert_eq!(r.bottleneck(), Some(0));
    let titles: Vec<&str> = r.sections.iter().map(|s| s.title.as_str()).collect();
    for t in [
        "Query String",
        "Physical Pipeline",
        "Runtime (ms)",
        "Index Operations",
    ] {
        assert!(titles.contains(&t), "{titles:?}");
    }
    let index = r
        .sections
        .iter()
        .find(|s| s.title == "Index Operations")
        .unwrap();
    assert!(index.text.contains("# of statement index ops: 63"));
}

#[test]
fn neptune_explain_sections() {
    let r = ExplainReport::from_neptune_text(include_str!(
        "../../../testenv/fixtures/neptune/explain-1.2.txt"
    ));
    assert!(r.original.unwrap().contains("CountGlobalStep"));
    assert!(
        r.final_traversal
            .unwrap()
            .contains("NeptuneCountGlobalStep")
    );
    assert!(r.sections.iter().any(|s| s.title == "Physical Pipeline"));
}
