//! The reference corpus of valid queries must parse with no diagnostics; the invalid corpus must not.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use gremlin_syntax::{Document, format};

fn files(dir: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(dir);
    let mut out: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "groovy"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn scenario_gremlin_language_valid_corpus_parses_cleanly() {
    let mut statements = 0;
    let mut problems = Vec::new();
    for (name, text) in files("valid") {
        let doc = Document::parse(text.as_str());
        statements += doc.statements().len();
        for d in doc.diagnostics() {
            let (line, col) = gremlin_syntax::line_col(&text, d.range.start);
            problems.push(format!("{name}:{line}:{col}: {}", d.message));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(statements >= 300, "corpus has only {statements} statements");
}

#[test]
fn formatting_the_corpus_is_idempotent_and_preserves_structure() {
    for (name, text) in files("valid") {
        let once = format(&text, 80);
        assert_eq!(format(&once, 80), once, "{name} is not idempotent");
        let before = Document::parse(text.as_str());
        let after = Document::parse(once.as_str());
        assert!(
            after.diagnostics().is_empty(),
            "{name}: formatting introduced errors"
        );
        let shape = |d: &Document| -> Vec<String> {
            d.statements()
                .iter()
                .flat_map(|s| d.steps(s))
                .flat_map(|s| {
                    s.walk()
                        .into_iter()
                        .map(|n| format!("{}{}", n.depth, n.name))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert_eq!(
            shape(&before),
            shape(&after),
            "{name}: formatting changed the steps"
        );
    }
}

#[test]
fn invalid_corpus_reports_errors() {
    for (name, text) in files("invalid") {
        let doc = Document::parse(text.as_str());
        assert!(
            doc.diagnostics()
                .iter()
                .any(|d| d.severity == gremlin_syntax::Severity::Error),
            "{name} should have a syntax error"
        );
    }
}
