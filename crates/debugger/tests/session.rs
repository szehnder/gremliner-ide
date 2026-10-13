//! Debug sessions and their generated queries. Scenario names map to
//! `openspec/changes/query-debugger/specs/query-debugger/spec.md`.

#![allow(clippy::unwrap_used)]

use debugger::{ElementKind, Session, StartError, format_count};

const AUS: &str = "g.V().has('code','AUS').out('route').has('country','US').values('code')";

fn names(s: &Session) -> Vec<String> {
    s.steps().iter().map(|s| s.name.clone()).collect()
}

#[test]
fn main_chain_and_prefixes() {
    let s = Session::start(AUS).unwrap();
    assert_eq!(names(&s), ["V", "has", "out", "has", "values"]);
    assert_eq!(s.current(), 0);
    assert_eq!(s.prefix(0), "g.V()");
    assert_eq!(s.prefix(2), "g.V().has('code','AUS').out('route')");
    assert_eq!(
        s.results_query(2, 500),
        "g.V().has('code','AUS').out('route').limit(500)"
    );
    assert_eq!(
        s.count_query(1, 100_000),
        "g.V().has('code','AUS').limit(100000).count()"
    );
}

#[test]
fn scenario_query_debugger_step_into_a_nested_traversal() {
    let q = "g.V().has('code','AUS').where(out('route').has('code','LHR')).values('city')";
    let mut s = Session::start(q).unwrap();
    s.jump(2);
    assert_eq!(
        s.call_text(s.current()),
        "where(out('route').has('code','LHR'))"
    );
    s.step_into();
    assert_eq!(s.call_text(s.current()), "out('route')");
    assert_eq!(
        s.prefix(s.current()),
        "g.V().has('code','AUS').flatMap(__.out('route'))"
    );
    s.step_over();
    assert_eq!(s.call_text(s.current()), "has('code','LHR')");
    assert_eq!(
        s.prefix(s.current()),
        "g.V().has('code','AUS').flatMap(__.out('route').has('code','LHR'))"
    );
    // The end of the nested traversal steps over to the step after the parent.
    s.step_over();
    assert_eq!(s.call_text(s.current()), "values('city')");
    // Step out of a nested step returns to the parent.
    s.jump(3);
    s.step_out();
    assert_eq!(s.step(s.current()).name, "where");
}

#[test]
fn branches_are_separate_chains() {
    let s = Session::start("g.V().union(out('a').limit(1), in('b')).count()").unwrap();
    let union = 1;
    let chains: Vec<Vec<String>> = s
        .chains()
        .iter()
        .map(|c| c.iter().map(|&i| s.call_text(i).to_owned()).collect())
        .collect();
    assert_eq!(chains[1], ["out('a')", "limit(1)"]);
    assert_eq!(chains[2], ["in('b')"]);
    let in_b = s.steps().iter().position(|st| st.name == "in").unwrap();
    assert_eq!(s.step(in_b).parent, Some(union));
    assert_eq!(s.prefix(in_b), "g.V().flatMap(__.in('b'))");
    let repeat = Session::start("g.V().repeat(out()).times(2)").unwrap();
    let out = repeat
        .steps()
        .iter()
        .position(|st| st.name == "out")
        .unwrap();
    assert!(repeat.prefix_note(out).unwrap().contains("first iteration"));
}

#[test]
fn scenario_query_debugger_start_debugging() {
    let s = Session::start(AUS).unwrap();
    assert_eq!(s.current(), 0);
    assert_eq!(s.call_text(0), "V()");
    assert_eq!(s.step(0).name_span, 2..3);
}

#[test]
fn scenario_query_debugger_mutating_query_refused() {
    let err = Session::start("g.V().drop()").unwrap_err();
    assert_eq!(err, StartError::Mutating);
    assert!(err.message().contains("cannot be debugged"));
    assert_eq!(Session::start("").unwrap_err(), StartError::Empty);
    assert_eq!(Session::start("g.V(").unwrap_err(), StartError::Syntax);
    assert_eq!(
        Session::start("g.V(); g.E()").unwrap_err(),
        StartError::NotATraversal
    );
}

#[test]
fn scenario_query_debugger_step_back() {
    let mut s = Session::start(AUS).unwrap();
    s.step_over();
    s.step_over();
    assert_eq!(s.current(), 2, "third step");
    s.step_back();
    assert_eq!(s.current(), 1);
    assert_eq!(s.call_text(1), "has('code','AUS')");
    s.step_back();
    s.step_back();
    assert_eq!(s.current(), 0, "stays on the first step");
}

#[test]
fn scenario_query_debugger_modulator_step() {
    let q = "g.V().hasLabel('airport').group().by('country').by(count())";
    let mut s = Session::start(q).unwrap();
    let by = s.steps().iter().position(|st| st.name == "by").unwrap();
    s.jump(by);
    assert_eq!(s.step(s.current()).name, "group");
    assert!(s.note().unwrap().contains("evaluated as part of `group()`"));
    // Navigation skips modulators.
    let mut s = Session::start(q).unwrap();
    s.jump(2);
    s.step_over();
    assert_eq!(s.current(), 2, "group is the last navigable step");
    // A count inside a by() is folded too.
    let count = s.steps().iter().position(|st| st.name == "count").unwrap();
    assert!(s.step(count).folded);
}

#[test]
fn simulation_and_inspection_queries() {
    let s = Session::start(AUS).unwrap();
    assert_eq!(
        s.survivors_query(2, ElementKind::Vertex),
        "g.V(dbgIds).as('dbgStart').has('country','US').values('code').select('dbgStart').dedup().id()"
    );
    assert_eq!(
        s.inspection_queries(2, ElementKind::Vertex),
        vec![
            (
                3,
                "g.V(dbgIds).has('country','US').limit(1).count()".to_owned()
            ),
            (
                4,
                "g.V(dbgIds).has('country','US').values('code').limit(1).count()".to_owned()
            ),
        ]
    );
    assert!(s.simulation_unavailable(2).is_none());
    let counted = Session::start("g.V().out().count()").unwrap();
    assert!(
        counted
            .simulation_unavailable(1)
            .unwrap()
            .contains("count()")
    );
    assert_eq!(format_count(1204, 100_000), "1,204");
    assert_eq!(format_count(100_000, 100_000), "≥ 100,000");
}
