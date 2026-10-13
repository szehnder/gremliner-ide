//! Completion, signature help and schema warnings against schema fixtures (no server needed). Scenario
//! names map to `openspec/changes/autocomplete/specs/autocomplete/spec.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Instant;

use gremlin_syntax::complete::{ItemKind, complete, edit_distance, schema_issues, signature_help};
use schema::Schema;

fn air() -> Schema {
    Schema::from_json(include_str!("fixtures/air-routes-schema.json")).expect("fixture")
}

fn modern() -> Schema {
    Schema::from_json(include_str!("fixtures/modern-schema.json")).expect("fixture")
}

fn at_end(text: &str, schema: Option<&Schema>) -> gremlin_syntax::complete::Completions {
    complete(text, text.len(), schema)
}

#[test]
fn scenario_autocomplete_suggest_steps() {
    let c = at_end("g.V().ha", None);
    let labels = c.labels();
    for want in ["has", "hasLabel", "hasId", "hasKey", "hasValue", "hasNot"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    for item in c.items.iter().filter(|i| i.label.starts_with("has")) {
        assert!(!item.detail.is_empty(), "{} has a description", item.label);
        assert_eq!(item.kind, ItemKind::Step);
    }
    // Prefix matches come before fuzzy ones, and common steps first.
    assert_eq!(labels[0], "has");
    assert_eq!(c.replace, 6..8, "replaces the typed prefix");
    // Steps that need arguments put the cursor inside the parentheses.
    let has = &c.items[0];
    assert_eq!(has.insert, "has()");
    assert_eq!(has.cursor, Some(4));
    let count = at_end("g.V().cou", None);
    let item = count.items.iter().find(|i| i.label == "count").unwrap();
    assert_eq!((item.insert.as_str(), item.cursor), ("count()", None));
}

#[test]
fn scenario_autocomplete_source_steps_after_g() {
    let c = at_end("g.", None);
    let labels = c.labels();
    for want in ["V", "E", "addV", "inject", "with"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    assert!(
        !labels.contains(&"out"),
        "mid-traversal steps are not sources"
    );
    // Source-only steps are not offered mid-traversal.
    let mid = at_end("g.V().with", None);
    assert!(!mid.labels().contains(&"withSideEffect"));
}

#[test]
fn scenario_autocomplete_vertex_label() {
    let schema = air();
    let c = at_end("g.V().hasLabel('", Some(&schema));
    let labels = c.labels();
    for want in ["airport", "country", "continent"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    let airport = c.items.iter().find(|i| i.label == "airport").unwrap();
    assert_eq!(airport.insert, "airport'", "closes the string");
    assert!(airport.detail.contains("3,504"));
    // Before the quote: items come quoted.
    let c = at_end("g.V().hasLabel(", Some(&schema));
    assert!(c.items.iter().any(|i| i.insert == "'airport'"));
    // With a closing quote already there, none is added.
    let text = "g.V().hasLabel('air')";
    let c = complete(text, text.len() - 2, Some(&schema));
    assert_eq!(c.items[0].insert, "airport");
    assert_eq!(c.replace, 16..19);
    // Without a schema: nothing to suggest, no error.
    assert!(at_end("g.V().hasLabel('", None).items.is_empty());
}

#[test]
fn scenario_autocomplete_reachable_edge_labels_ranked_first() {
    let schema = air();
    let c = at_end("g.V().hasLabel('airport').out('", Some(&schema));
    let labels = c.labels();
    let route = labels.iter().position(|l| *l == "route").unwrap();
    let contains = labels.iter().position(|l| *l == "contains").unwrap();
    assert!(route < contains, "{labels:?}");
    let contains_item = &c.items[contains];
    assert!(
        contains_item.detail.contains("not from airport"),
        "{}",
        contains_item.detail
    );
    // From countries, `contains` is the reachable one.
    let c = at_end("g.V().hasLabel('country').out('", Some(&schema));
    assert_eq!(c.labels()[0], "contains");
    // in(): edges arriving at airports.
    let c = at_end("g.V().hasLabel('airport').in('", Some(&schema));
    let labels = c.labels();
    assert!(labels.contains(&"contains") && labels.contains(&"route"));
}

#[test]
fn scenario_autocomplete_keys_for_inferred_label() {
    let schema = air();
    let c = at_end("g.V().hasLabel('airport').values('", Some(&schema));
    let labels = c.labels();
    for want in ["code", "city", "runways"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    for only_version in ["author", "date"] {
        assert!(
            !labels.contains(&only_version),
            "{only_version} is not an airport key"
        );
    }
    let runways = c.items.iter().find(|i| i.label == "runways").unwrap();
    assert!(runways.detail.starts_with("Int32"), "{}", runways.detail);
    // Through a traversal: out('contains') from countries reaches airports.
    let c = at_end(
        "g.V().hasLabel('country').out('contains').values('",
        Some(&schema),
    );
    assert!(c.labels().contains(&"runways"));
    // Unknown label: every key.
    let c = at_end("g.V().values('", Some(&schema));
    assert!(c.labels().contains(&"author"));
    // by() in order() sees the ordered elements.
    let c = at_end("g.V().hasLabel('airport').order().by('", Some(&schema));
    assert!(c.labels().contains(&"runways") && !c.labels().contains(&"author"));
}

#[test]
fn scenario_autocomplete_sample_value_suggestion() {
    let mut schema = air();
    // Samples are whatever discovery happened to read; make sure some start with "A".
    let airport = schema
        .vertex_labels
        .iter_mut()
        .find(|l| l.name == "airport")
        .unwrap();
    let code = airport
        .properties
        .iter_mut()
        .find(|p| p.key == "code")
        .unwrap();
    code.samples = vec!["AUS".into(), "ATL".into(), "BNA".into()];
    let c = at_end("g.V().has('airport','code','A", Some(&schema));
    let labels = c.labels();
    assert!(
        labels.contains(&"AUS") && labels.contains(&"ATL"),
        "{labels:?}"
    );
    assert!(!labels.contains(&"BNA"), "filtered by what was typed");
    assert_eq!(c.items[0].kind, ItemKind::Value);
    // The two-argument form looks the key up on the flow's labels.
    let c = at_end("g.V().hasLabel('airport').has('code', '", Some(&schema));
    assert!(c.labels().contains(&"BNA"));
    // Numbers are inserted bare outside strings.
    let c = at_end("g.V().has('airport','runways',", Some(&schema));
    assert!(
        c.items.iter().all(|i| !i.insert.starts_with('\'')),
        "{:?}",
        c.labels()
    );
}

#[test]
fn scenario_autocomplete_signature_for_has() {
    let schema = air();
    let text = "g.V().has('airport', ";
    let help = signature_help(text, text.len(), Some(&schema)).unwrap();
    assert_eq!(help.step, "has");
    assert_eq!(
        help.active_signature().label,
        "has(String label, String propertyKey, Object value)"
    );
    assert_eq!(help.active_param, 1);
    assert_eq!(help.active_param_text(), Some("String propertyKey"));
    assert!(help.signatures.len() > 3, "every form is listed");
    // A key first: the key/value form.
    let text = "g.V().has('code', ";
    let help = signature_help(text, text.len(), Some(&schema)).unwrap();
    assert_eq!(help.active_param_text(), Some("Object value"));
    // Varargs keep highlighting the last parameter.
    let text = "g.V().hasLabel('a', 'b', ";
    let help = signature_help(text, text.len(), None).unwrap();
    assert_eq!(help.active_param_text(), Some("String... otherLabels"));
    // Outside parentheses there is no help.
    assert!(signature_help("g.V().has('a')", 14, None).is_none());
}

#[test]
fn scenario_autocomplete_predicate_completion() {
    let c = at_end("g.V().has('age', P.", None);
    let labels = c.labels();
    for want in ["gt", "gte", "lt", "lte", "between", "within", "without"] {
        assert!(labels.contains(&want), "{want} in {labels:?}");
    }
    let gt = c.items.iter().find(|i| i.label == "gt").unwrap();
    assert_eq!((gt.insert.as_str(), gt.cursor), ("gt()", Some(3)));
    let c = at_end("g.V().has('name', TextP.st", None);
    assert_eq!(c.labels()[0], "startingWith");
    let c = at_end("g.V().order().by('age', Order.", None);
    assert!(c.labels().contains(&"desc"));
    // Bare predicates and tokens while typing an argument.
    let c = at_end("g.V().has('age', gt", None);
    assert_eq!(c.labels()[0], "gt");
    let c = at_end("g.V().order().by('age', de", None);
    assert!(c.labels().contains(&"desc"));
    // Snippets complete by trigger.
    let c = at_end("g.V().repeat-u", None);
    let snippet = c
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Snippet)
        .unwrap();
    assert_eq!(snippet.label, "repeat-until");
    assert!(snippet.cursor.is_some());
    assert!(!snippet.insert.contains('|'));
}

#[test]
fn scenario_autocomplete_misspelled_label() {
    let schema = air();
    let text = "g.V().hasLabel('airprt').out('rout').values('cde')";
    let issues = schema_issues(text, &schema);
    assert_eq!(issues.len(), 3, "{issues:#?}");
    assert_eq!(&text[issues[0].range.clone()], "airprt");
    assert!(issues[0].message.contains("Unknown vertex label `airprt`"));
    assert_eq!(issues[0].replacement.as_deref(), Some("airport"));
    assert_eq!(issues[1].replacement.as_deref(), Some("route"));
    assert_eq!(issues[2].replacement.as_deref(), Some("code"));
    // Known names, new labels being created and partial schemas raise nothing.
    assert!(schema_issues("g.V().hasLabel('airport').values('code')", &schema).is_empty());
    assert!(schema_issues("g.addV('brandnew').property('anything', 1)", &schema).is_empty());
    let mut partial = schema.clone();
    partial.partial.labels = true;
    partial.partial.properties = true;
    assert!(schema_issues(text, &partial).is_empty());
    // Far-off names get a warning without a fix.
    let issues = schema_issues("g.V().hasLabel('zzzzzz')", &schema);
    assert_eq!(issues[0].replacement, None);
    assert_eq!(edit_distance("airprt", "airport"), 1);
}

#[test]
fn modern_keys_and_edges() {
    let schema = modern();
    let c = at_end(
        "g.V().hasLabel('person').outE('knows').values('",
        Some(&schema),
    );
    assert_eq!(c.labels(), ["weight"]);
    let c = at_end(
        "g.V().hasLabel('person').out('created').values('",
        Some(&schema),
    );
    assert!(c.labels().contains(&"lang"));
    let c = at_end("g.V().as('a').out().as('b').select('", Some(&schema));
    assert_eq!(c.labels(), ["a", "b"]);
}

/// A 200-line document of realistic queries.
pub fn latency_document() -> String {
    let lines = [
        "g.V().hasLabel('airport').has('code', 'AUS').out('route').values('city')",
        "g.V().has('airport', 'country', 'US').order().by('runways', desc).limit(10)",
        "g.V().hasLabel('country').out('contains').group().by('region').by(count())",
        "g.V().hasLabel('airport').where(out('route').has('code', 'LHR')).values('code')",
        "g.E().hasLabel('route').has('dist', P.gt(5000)).inV().values('desc')",
    ];
    (0..200)
        .map(|i| lines[i % lines.len()])
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn scenario_autocomplete_completion_speed() {
    let schema = air();
    let doc = latency_document();
    // 100 positions spread over the document, each at a dot or quote where completion triggers.
    let triggers: Vec<usize> = doc
        .char_indices()
        .filter(|(_, c)| matches!(c, '.' | '\'' | '('))
        .map(|(i, _)| i + 1)
        .collect();
    let step = (triggers.len() / 100).max(1);
    let positions: Vec<usize> = triggers.into_iter().step_by(step).take(100).collect();
    assert_eq!(positions.len(), 100);
    let mut times: Vec<f64> = positions
        .iter()
        .map(|&offset| {
            let started = Instant::now();
            let c = complete(&doc, offset, Some(&schema));
            let _ = signature_help(&doc, offset, Some(&schema));
            std::hint::black_box(c);
            started.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let p95 = times[94];
    eprintln!("completion p95 {p95:.3} ms, max {:.3} ms", times[99]);
    println!("measure completion_latency={p95:.3} ms");
    assert!(p95 <= 30.0, "p95 {p95:.3} ms");
}

#[test]
fn direction_mismatches_are_reported() {
    let schema = air();
    let issues = gremlin_syntax::complete::direction_issues(
        "g.V().hasLabel('airport').out('contains').values('code')",
        &schema,
    );
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(
        issues[0].message.contains("use in('contains')"),
        "{}",
        issues[0].message
    );
    assert_eq!(issues[0].replacement.as_deref(), Some("in"));
    assert!(
        gremlin_syntax::complete::direction_issues(
            "g.V().hasLabel('country').out('contains')",
            &schema
        )
        .is_empty()
    );
    assert!(
        gremlin_syntax::complete::direction_issues("g.V().out('contains')", &schema).is_empty()
    );
    assert_eq!(
        gremlin_syntax::complete::lambdas("g.V().map{ it.get() }.count()").len(),
        1
    );
}
