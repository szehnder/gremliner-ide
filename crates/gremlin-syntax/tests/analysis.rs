//! Scenario tests for `openspec/changes/gremlin-language/specs/gremlin-language/spec.md`.

#![allow(clippy::unwrap_used)]

use std::time::Instant;

use gremlin_syntax::{Document, Severity, StepNode, analyze, is_mutating, line_col};

fn names(steps: &[StepNode]) -> Vec<&str> {
    steps.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn scenario_gremlin_language_nested_anonymous_traversal() {
    let doc = Document::parse("g.V().where(__.out('knows').has('age', P.gt(30))).values('name')");
    let stmt = &doc.statements()[0];
    let steps = doc.steps(stmt);
    assert_eq!(names(&steps), ["V", "where", "values"]);
    assert_eq!(names(&steps[1].children), ["out", "has"]);
    assert!(
        steps[1]
            .children
            .iter()
            .all(|s| s.depth == 1 && s.children.is_empty())
    );
    assert!(doc.diagnostics().is_empty(), "{:?}", doc.diagnostics());
}

#[test]
fn scenario_gremlin_language_unclosed_parenthesis() {
    let text = "g.V().has('name', 'marko'";
    let diags = Document::parse(text).diagnostics();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(diags[0].range, text.len()..text.len());
    assert!(
        diags[0].message.contains("closing parenthesis"),
        "{}",
        diags[0].message
    );
}

#[test]
fn scenario_gremlin_language_error_recovery() {
    let doc = Document::parse("g.V().has('name',, 'x')\n\ng.V().out('knows').count()");
    let statements = doc.statements();
    assert_eq!(statements.len(), 2, "{statements:?}");
    assert!(statements[0].has_error);
    assert!(!statements[1].has_error);
    assert_eq!(names(&doc.steps(&statements[1])), ["V", "out", "count"]);
    assert!(!doc.diagnostics().is_empty());
}

#[test]
fn scenario_gremlin_language_pick_the_statement_under_the_cursor() {
    let text = "g.V().count()\n\ng.V().hasLabel('airport').\n  values('code')\n\ng.E().count()";
    let doc = Document::parse(text);
    let cursor = text.find("values").unwrap() + 2;
    let stmt = doc.statement_at(cursor).unwrap();
    assert_eq!(stmt.index, 1);
    assert_eq!(stmt.text, "g.V().hasLabel('airport').\n  values('code')");
    assert_eq!(&text[stmt.range.clone()], stmt.text);
    // Semicolons separate statements too.
    let doc = Document::parse("g.V().count(); g.E().count()");
    assert_eq!(doc.statements().len(), 2);
}

#[test]
fn scenario_gremlin_language_detect_drop() {
    assert!(analyze("g.V().has('name','x').drop()")[0].is_mutating);
    assert!(is_mutating("g.V(1).property('age', 30)"));
    assert!(
        is_mutating("g.V().coalesce(__.has('x'), __.addV('person'))"),
        "nested mutations count"
    );
}

#[test]
fn scenario_gremlin_language_read_only_query() {
    let a = &analyze("g.V().hasLabel('airport').values('code')")[0];
    assert!(!a.is_mutating);
    assert!(!is_mutating("g.V().properties('name').value()"));
    assert!(!is_mutating("g.V().has('name', 'drop')"));
}

#[test]
fn scenario_gremlin_language_unparseable_treated_as_mutating() {
    assert!(analyze("g.V().has('name', 'marko'")[0].is_mutating);
    assert!(
        is_mutating("g.V().filter{ it.get().remove() }"),
        "closures cannot be analysed"
    );
}

#[test]
fn scenario_gremlin_language_detect_parameters() {
    let a = &analyze("g.V().has('airport','code',code).out().limit(n)")[0];
    assert_eq!(a.parameters, ["code", "n"]);
    // Tokens, enums, predicates and assigned variables are not parameters.
    let doc = Document::parse(
        "x = 5\ng.V().order().by('age', desc).has('a', within(names)).limit(x).group().by(label)",
    );
    let all = doc.analyze_all();
    assert_eq!(all[1].parameters, ["names"]);
}

#[test]
fn scenario_gremlin_language_top_level_prefix() {
    let doc = Document::parse("g.V().has('code','AUS').out('route').values('code')");
    let stmt = &doc.statements()[0];
    let out = doc
        .steps(stmt)
        .into_iter()
        .find(|s| s.name == "out")
        .unwrap();
    assert_eq!(
        doc.prefix_at(stmt, out.id).unwrap(),
        "g.V().has('code','AUS').out('route')"
    );
    assert_eq!(doc.prefix_at(stmt, 0).unwrap(), "g.V()");
}

#[test]
fn nested_prefixes_run_in_their_parent_context() {
    let doc = Document::parse(
        "g.V().hasLabel('person').where(__.out('knows').has('age', P.gt(30))).values('name')",
    );
    let stmt = &doc.statements()[0];
    let steps = doc.steps(stmt);
    let nested_has = steps[2].children[1].id;
    assert_eq!(
        doc.prefix_at(stmt, nested_has).unwrap(),
        "g.V().hasLabel('person').out('knows').has('age', P.gt(30))"
    );
    let bare = Document::parse("g.V().local(out('route').limit(2)).count()");
    let stmt = &bare.statements()[0];
    let limit = bare.steps(stmt)[1].children[1].id;
    assert_eq!(
        bare.prefix_at(stmt, limit).unwrap(),
        "g.V().out('route').limit(2)"
    );
}

#[test]
fn unknown_steps_are_warnings() {
    let diags = Document::parse("g.V().outt('x')").diagnostics();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].message, "Unknown step `outt`");
    assert_eq!(line_col("a\nbc", 3), (2, 2));
}

#[test]
fn scenario_gremlin_language_keystroke_latency() {
    let text: String = (0..50)
        .map(|i| format!("g.V().hasLabel('airport').\n  has('code','A{i}').\n  out('route').\n  values('city')\n\n"))
        .collect();
    assert!(text.lines().count() >= 200);
    let mut doc = Document::parse(text.clone());
    let mut times = Vec::new();
    for i in 0..200 {
        let at = (text.len() / 200) * i;
        let at = (0..=at).rev().find(|&p| text.is_char_boundary(p)).unwrap();
        let started = Instant::now();
        doc.edit(at..at, "x");
        let _ = doc.diagnostics();
        times.push(started.elapsed());
        doc.edit(at..at + 1, "");
    }
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    // The spec budget is 8 ms on the reference machine; unoptimised test builds get 4x headroom.
    let budget_ms = if cfg!(debug_assertions) { 32 } else { 8 };
    assert!(p95.as_millis() <= budget_ms, "p95 {p95:?}");
}
