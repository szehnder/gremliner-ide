//! Proves the gremlin-syntax corpus is real Gremlin: every read-only statement is sent to the testenv
//! and must not fail with a compilation or missing-method error (runtime errors such as empty results or
//! missing properties are fine). Run with `cargo test -p gremliner-cli --test corpus_on_server -- --ignored`.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::time::Duration;

use gremlin_client::{Client, ClientConfig, Request};

const NOT_GREMLIN: &[&str] = &[
    "MultipleCompilationErrorsException",
    "No signature of method",
    "MissingPropertyException",
    "No such property",
    "MissingMethodException",
];

#[tokio::test]
#[ignore = "needs the testenv; sends ~300 queries"]
async fn corpus_statements_compile_on_gremlin_server() {
    let url = gremlin_client::testenv::url();
    if !gremlin_client::testenv::is_up(&url) {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../gremlin-syntax/tests/corpus/valid");
    let mut failures = Vec::new();
    let mut sent = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let alias = if path.file_name().unwrap() == "air_routes.groovy" {
            "g"
        } else {
            "gModern"
        };
        let mut config = ClientConfig::new(url.clone()).alias(alias);
        config.request_timeout = Duration::from_secs(5);
        let client = Client::connect(config).await.unwrap();
        let doc = gremlin_syntax::Document::parse(text.as_str());
        for analysis in doc.analyze_all() {
            // Writes would change the shared test graphs; closures and Groovy statements are skipped too.
            let script = analysis.statement.text.clone();
            if analysis.is_mutating || script.contains("withComputer") || !script.starts_with("g.")
            {
                continue;
            }
            sent += 1;
            let request = Request::new(format!("{script};null")).timeout(Duration::from_secs(5));
            if let Err(err) = client.execute_request(request).await {
                let text = format!("{err:?}");
                if NOT_GREMLIN.iter().any(|m| text.contains(m)) {
                    failures.push(format!("{}: {script}\n    {err}", path.display()));
                }
            }
        }
    }
    assert!(sent > 200, "only {sent} statements sent");
    assert!(
        failures.is_empty(),
        "{} not valid Gremlin:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
