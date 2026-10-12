//! Discovery against a scripted fake runner: partial results on timeouts and the summary fast path.
//! Live-server scenarios are in `crates/providers/tests/schema.rs`.

use std::sync::Mutex;
use std::time::Duration;

use futures::future::BoxFuture;
use gremlin_model::{GKey, GValue, new_map};
use schema::discover::scripts;
use schema::{Options, QueryError, QueryRunner, Schema, Source, Summary, Triple, discover};

/// Answers by script, records every script it was asked to run.
struct Fake {
    answer: fn(&str, &str) -> Result<Vec<GValue>, QueryError>,
    seen: Mutex<Vec<String>>,
}

impl QueryRunner for Fake {
    fn run<'a>(
        &'a self,
        script: &'a str,
        bindings: Vec<(String, GValue)>,
        _timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<GValue>, QueryError>> {
        let label = bindings
            .iter()
            .find_map(|(_, v)| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        self.seen.lock().expect("lock").push(script.to_owned());
        let out = (self.answer)(script, &label);
        Box::pin(async move { out })
    }
}

fn s(v: &str) -> GValue {
    GValue::String(v.into())
}

fn map(entries: Vec<(&str, GValue)>) -> GValue {
    let mut m = new_map();
    for (k, v) in entries {
        m.insert(GKey::String(k.into()), v);
    }
    GValue::Map(m)
}

/// The modern graph, answered the way Gremlin Server does.
fn modern(script: &str, label: &str) -> Result<Vec<GValue>, QueryError> {
    let opts = Options::default();
    if script.contains(".label().dedup()") {
        return Ok(if script.starts_with("g.V()") {
            vec![s("person"), s("software")]
        } else {
            vec![s("knows"), s("created")]
        });
    }
    if script == scripts::vertex_properties(opts.sample) {
        let samples = match label {
            "person" => map(vec![
                ("name", GValue::List(vec![s("marko"), s("vadas")])),
                (
                    "age",
                    GValue::List(vec![GValue::Int32(29), GValue::Int32(27)]),
                ),
            ]),
            _ => map(vec![
                ("name", GValue::List(vec![s("lop")])),
                ("lang", GValue::List(vec![s("java")])),
            ]),
        };
        return Ok(vec![map(vec![
            ("samples", samples),
            ("multi", GValue::List(vec![])),
        ])]);
    }
    if script == scripts::edge_properties(opts.sample) {
        return Ok(vec![map(vec![(
            "weight",
            GValue::List(vec![GValue::Double(0.5)]),
        )])]);
    }
    if script.ends_with(".count()") {
        return Ok(vec![GValue::Int64(match label {
            "person" => 4,
            "software" => 2,
            "knows" => 2,
            _ => 4,
        })]);
    }
    if script == scripts::triples(opts.sample) {
        let to = if label == "knows" {
            "person"
        } else {
            "software"
        };
        return Ok(vec![map(vec![("o", s("person")), ("i", s(to))])]);
    }
    Err(QueryError::Failed(format!("unexpected script {script}")))
}

fn run(fake: &Fake, summary: Option<Summary>) -> (Schema, usize) {
    let snapshots = Mutex::new(0usize);
    let schema =
        futures::executor::block_on(discover(fake, summary, &Options::default(), 42, &|_| {
            *snapshots.lock().expect("lock") += 1
        }));
    (schema, snapshots.into_inner().expect("lock"))
}

#[test]
fn fake_modern_schema_is_complete() {
    let fake = Fake {
        answer: modern,
        seen: Mutex::new(Vec::new()),
    };
    let (schema, snapshots) = run(&fake, None);
    assert!(!schema.in_progress);
    assert!(!schema.partial.any(), "{:?}", schema.partial);
    assert!(snapshots > 3, "progress is reported incrementally");
    let person = schema.vertex_label("person").expect("person");
    assert_eq!(person.count.map(|c| c.value), Some(4));
    assert_eq!(person.property("age").expect("age").type_text(), "Int32");
    assert!(
        schema
            .triples
            .contains(&Triple::new("person", "created", "software"))
    );
    assert_eq!(schema.discovered_at_ms, 42);
}

#[test]
fn scenario_schema_discovery_timeout_yields_partial_schema() {
    fn slow_properties(script: &str, label: &str) -> Result<Vec<GValue>, QueryError> {
        if script.contains(".properties()") {
            return Err(QueryError::Timeout);
        }
        modern(script, label)
    }
    let fake = Fake {
        answer: slow_properties,
        seen: Mutex::new(Vec::new()),
    };
    let (schema, _) = run(&fake, None);
    let names: Vec<_> = schema
        .vertex_labels
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, ["person", "software"]);
    assert!(schema.partial.properties);
    assert!(!schema.partial.labels);
    assert!(
        schema
            .vertex_label("person")
            .expect("person")
            .properties
            .is_empty()
    );
    assert!(schema.notes.iter().any(|n| n.contains("timed out")));
    // Unaffected parts are still complete.
    assert_eq!(schema.triples.len(), 2);
}

#[test]
fn labels_survive_a_full_scan_timeout() {
    fn slow_scan(script: &str, label: &str) -> Result<Vec<GValue>, QueryError> {
        if script == scripts::ALL_VERTEX_LABELS {
            return Err(QueryError::Timeout);
        }
        modern(script, label)
    }
    let fake = Fake {
        answer: slow_scan,
        seen: Mutex::new(Vec::new()),
    };
    let (schema, _) = run(&fake, None);
    assert!(schema.partial.labels);
    assert_eq!(schema.vertex_labels.len(), 2, "sampled labels kept");
}

#[test]
fn scenario_schema_discovery_neptune_summary_used() {
    let text = include_str!("fixtures/neptune-summary-detailed.json");
    let summary = Summary::from_neptune_json(text).expect("summary");
    fn air(script: &str, _label: &str) -> Result<Vec<GValue>, QueryError> {
        if script.ends_with(".count()") {
            return Ok(vec![GValue::Int64(10)]);
        }
        Ok(Vec::new())
    }
    let fake = Fake {
        answer: air,
        seen: Mutex::new(Vec::new()),
    };
    let (schema, _) = run(&fake, Some(summary));
    assert_eq!(schema.source, Source::Summary);
    let names: Vec<_> = schema
        .vertex_labels
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names, ["airport", "continent", "country", "version"]);
    let airport = schema.vertex_label("airport").expect("airport");
    assert_eq!(airport.count.map(|c| c.value), Some(3504));
    assert!(airport.property("runways").is_some());
    assert_eq!(airport.properties.len(), 9);
    assert!(schema.edge_label("route").is_some());

    let seen = fake.seen.lock().expect("lock");
    assert!(
        !seen.iter().any(|q| q.contains(".label().dedup()")),
        "no label sampling: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|q| q.starts_with("g.V()") && q.ends_with(".count()")),
        "vertex counts come from the summary: {seen:?}"
    );
}

#[test]
fn summary_without_graph_summary_is_none() {
    assert!(Summary::from_neptune_json(r#"{"status":"200 OK","payload":{}}"#).is_none());
    assert!(Summary::from_neptune_json("not json").is_none());
}

/// Pending once (after asking to be polled again), then ready: lets other queries start meanwhile.
struct YieldNow(bool);

impl std::future::Future for YieldNow {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

#[test]
fn concurrency_is_bounded() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counting {
        in_flight: AtomicUsize,
        peak: AtomicUsize,
    }
    impl QueryRunner for Counting {
        fn run<'a>(
            &'a self,
            script: &'a str,
            bindings: Vec<(String, GValue)>,
            _timeout: Duration,
        ) -> BoxFuture<'a, Result<Vec<GValue>, QueryError>> {
            let label = bindings
                .iter()
                .find_map(|(_, v)| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            Box::pin(async move {
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.peak.fetch_max(now, Ordering::SeqCst);
                for _ in 0..3 {
                    YieldNow(false).await;
                }
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                modern(script, &label)
            })
        }
    }
    let runner = Counting {
        in_flight: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
    };
    let schema =
        futures::executor::block_on(discover(&runner, None, &Options::default(), 0, &|_| {}));
    assert!(!schema.partial.any());
    let peak = runner.peak.load(Ordering::SeqCst);
    assert!((2..=3).contains(&peak), "peak {peak}");
}
