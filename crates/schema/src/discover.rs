//! Sampling discovery (design: schema-discovery). Runs bounded Gremlin queries through a [`QueryRunner`]:
//! labels first (sampled, then a full scan with a timeout), then per-label properties, counts and
//! connectivity, at most [`Options::concurrency`] queries at a time. Timeouts and failures make the
//! affected part partial instead of failing discovery; `progress` receives a snapshot after each step.

use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{self, StreamExt as _};
use gremlin_model::{GKey, GValue, console};

use crate::model::{
    Cardinality, Count, SAMPLES_PER_PROPERTY, Schema, Source, Triple, truncate_sample,
};
use crate::summary::Summary;

/// Limits for sampling discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Elements read per sampling query.
    pub sample: u32,
    /// Counting stops here and is shown as `≥ count_cap`.
    pub count_cap: u64,
    /// Per-query timeout.
    pub timeout: Duration,
    /// Discovery queries in flight at once.
    pub concurrency: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            sample: 10_000,
            count_cap: 100_000,
            timeout: Duration::from_secs(5),
            concurrency: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Timeout,
    Failed(String),
}

/// Runs one read-only discovery query. Implemented over the Gremlin client by `providers`, and by fakes
/// in tests.
pub trait QueryRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        script: &'a str,
        bindings: Vec<(String, GValue)>,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<Vec<GValue>, QueryError>>;
}

/// The binding carrying a label into discovery queries (not `label`: Gremlin Server imports `T.label`).
pub const LABEL_BINDING: &str = "schemaLabel";

/// The scripts discovery runs, so tests and the CLI can recognise them.
pub mod scripts {
    pub fn sampled_vertex_labels(sample: u32) -> String {
        format!("g.V().limit({sample}).label().dedup()")
    }
    pub fn sampled_edge_labels(sample: u32) -> String {
        format!("g.E().limit({sample}).label().dedup()")
    }
    pub const ALL_VERTEX_LABELS: &str = "g.V().label().dedup()";
    pub const ALL_EDGE_LABELS: &str = "g.E().label().dedup()";

    /// Keys with up to five distinct values each, and the keys some vertex holds several values for.
    pub fn vertex_properties(sample: u32) -> String {
        format!(
            "g.V().hasLabel(schemaLabel).limit({sample}).fold().project('samples','multi')\
             .by(unfold().properties().group().by(key).by(value().dedup().limit(5).fold()))\
             .by(unfold().local(properties().groupCount().by(key).unfold()\
             .where(select(values).is(gt(1))).select(keys)).dedup().fold())"
        )
    }
    pub fn edge_properties(sample: u32) -> String {
        format!(
            "g.E().hasLabel(schemaLabel).limit({sample}).properties()\
             .group().by(key).by(value().dedup().limit(5).fold())"
        )
    }
    pub fn vertex_count(cap: u64) -> String {
        format!("g.V().hasLabel(schemaLabel).limit({cap}).count()")
    }
    pub fn edge_count(cap: u64) -> String {
        format!("g.E().hasLabel(schemaLabel).limit({cap}).count()")
    }
    pub fn triples(sample: u32) -> String {
        format!(
            "g.E().hasLabel(schemaLabel).limit({sample})\
             .project('o','i').by(outV().label()).by(inV().label()).dedup()"
        )
    }
}

#[derive(Debug, Clone)]
enum Job {
    VertexProps(String),
    EdgeProps(String),
    VertexCount(String),
    EdgeCount(String),
    Triples(String),
}

impl Job {
    fn label(&self) -> &str {
        match self {
            Job::VertexProps(l)
            | Job::EdgeProps(l)
            | Job::VertexCount(l)
            | Job::EdgeCount(l)
            | Job::Triples(l) => l,
        }
    }

    fn script(&self, opts: &Options) -> String {
        match self {
            Job::VertexProps(_) => scripts::vertex_properties(opts.sample),
            Job::EdgeProps(_) => scripts::edge_properties(opts.sample),
            Job::VertexCount(_) => scripts::vertex_count(opts.count_cap),
            Job::EdgeCount(_) => scripts::edge_count(opts.count_cap),
            Job::Triples(_) => scripts::triples(opts.sample),
        }
    }
}

/// Discover the schema. `summary` is the database's statistics summary when the provider has one; it
/// replaces label, property-key and vertex-count discovery. `now_ms` stamps the result.
pub async fn discover(
    runner: &dyn QueryRunner,
    summary: Option<Summary>,
    opts: &Options,
    now_ms: i64,
    progress: &(dyn Fn(&Schema) + Send + Sync),
) -> Schema {
    let mut schema = Schema::new(now_ms);
    schema.in_progress = true;
    let from_summary = summary.is_some();
    match summary {
        Some(summary) => {
            schema.source = Source::Summary;
            summary.apply(&mut schema);
        }
        None => discover_labels(runner, opts, &mut schema, progress).await,
    }
    schema.normalize();
    progress(&schema);

    let mut jobs = Vec::new();
    for label in &schema.vertex_labels {
        jobs.push(Job::VertexProps(label.name.clone()));
        if !from_summary || label.count.is_none() {
            jobs.push(Job::VertexCount(label.name.clone()));
        }
    }
    for label in &schema.edge_labels {
        jobs.push(Job::Triples(label.name.clone()));
        jobs.push(Job::EdgeProps(label.name.clone()));
        if label.count.is_none() {
            jobs.push(Job::EdgeCount(label.name.clone()));
        }
    }
    let mut results = stream::iter(jobs.into_iter().map(|job| async move {
        let script = job.script(opts);
        let bindings = vec![(LABEL_BINDING.to_owned(), GValue::String(job.label().into()))];
        let result = runner.run(&script, bindings, opts.timeout).await;
        (job, result)
    }))
    .buffer_unordered(opts.concurrency.max(1));
    while let Some((job, result)) = results.next().await {
        apply_job(&mut schema, &job, result, opts);
        schema.normalize();
        progress(&schema);
    }
    schema.in_progress = false;
    schema
}

async fn discover_labels(
    runner: &dyn QueryRunner,
    opts: &Options,
    schema: &mut Schema,
    progress: &(dyn Fn(&Schema) + Send + Sync),
) {
    // Sampled first: fast, so the browser has something to show almost immediately.
    let (v, e) = (
        scripts::sampled_vertex_labels(opts.sample),
        scripts::sampled_edge_labels(opts.sample),
    );
    let (sv, se) = futures::join!(
        runner.run(&v, Vec::new(), opts.timeout),
        runner.run(&e, Vec::new(), opts.timeout)
    );
    let add =
        |schema: &mut Schema, vertices: bool, result: Result<Vec<GValue>, QueryError>| match result
        {
            Ok(items) => {
                for name in items.iter().filter_map(GValue::as_str) {
                    if vertices {
                        schema.vertex_label_mut(name);
                    } else {
                        schema.edge_label_mut(name);
                    }
                }
                true
            }
            Err(err) => {
                schema.partial.labels = true;
                note(schema, "labels", &err);
                false
            }
        };
    add(schema, true, sv);
    add(schema, false, se);
    schema.normalize();
    progress(schema);
    // Then a full scan for completeness, which may time out on large graphs.
    let (fv, fe) = futures::join!(
        runner.run(scripts::ALL_VERTEX_LABELS, Vec::new(), opts.timeout),
        runner.run(scripts::ALL_EDGE_LABELS, Vec::new(), opts.timeout)
    );
    let full_v = add(schema, true, fv);
    let full_e = add(schema, false, fe);
    if full_v && full_e {
        // Earlier sampled failures are superseded by the complete scans.
        schema.partial.labels = false;
    }
}

fn note(schema: &mut Schema, what: &str, err: &QueryError) {
    let text = match err {
        QueryError::Timeout => format!("{what}: timed out"),
        QueryError::Failed(msg) => format!("{what}: {msg}"),
    };
    if !schema.notes.contains(&text) {
        schema.notes.push(text);
    }
}

fn apply_job(
    schema: &mut Schema,
    job: &Job,
    result: Result<Vec<GValue>, QueryError>,
    opts: &Options,
) {
    let items = match result {
        Ok(items) => items,
        Err(err) => {
            let (flag, what) = match job {
                Job::VertexProps(_) | Job::EdgeProps(_) => {
                    (&mut schema.partial.properties, "properties")
                }
                Job::VertexCount(_) | Job::EdgeCount(_) => (&mut schema.partial.counts, "counts"),
                Job::Triples(_) => (&mut schema.partial.triples, "connections"),
            };
            *flag = true;
            note(schema, &format!("{what} of {}", job.label()), &err);
            return;
        }
    };
    if let Job::Triples(label) = job {
        schema.triples.extend(triples_from(label, &items));
        return;
    }
    let first = items.into_iter().next();
    match job {
        Job::VertexProps(label) => {
            let Some(GValue::Map(map)) = first else {
                return;
            };
            let target = schema.vertex_label_mut(label);
            if let Some(samples) = map_get(&map, "samples") {
                apply_samples(target, samples);
            }
            if let Some(GValue::List(multi) | GValue::Set(multi)) = map_get(&map, "multi") {
                for key in multi.iter().filter_map(GValue::as_str) {
                    target.property_mut(key).cardinality = Cardinality::Multi;
                }
            }
        }
        Job::EdgeProps(label) => {
            let target = schema.edge_label_mut(label);
            if let Some(samples) = &first {
                apply_samples(target, samples);
            }
        }
        Job::VertexCount(label) | Job::EdgeCount(label) => {
            let Some(n) = first.as_ref().and_then(GValue::as_i64) else {
                return;
            };
            let n = u64::try_from(n).unwrap_or(0);
            let count = Count {
                value: n,
                capped: n >= opts.count_cap,
            };
            if count.capped {
                schema.partial.counts = true;
            }
            let target = if matches!(job, Job::VertexCount(_)) {
                schema.vertex_label_mut(label)
            } else {
                schema.edge_label_mut(label)
            };
            target.count = Some(count);
        }
        Job::Triples(_) => {}
    }
}

fn map_get<'a>(map: &'a gremlin_model::IndexMap<GKey, GValue>, key: &str) -> Option<&'a GValue> {
    map.get(&GKey::String(key.to_owned()))
}

/// `{key: [values…]}` → property keys with inferred types and text samples.
fn apply_samples(target: &mut crate::model::LabelInfo, samples: &GValue) {
    let GValue::Map(map) = samples else {
        return;
    };
    for (key, values) in map {
        let GKey::String(key) = key else {
            continue;
        };
        let values: &[GValue] = match values {
            GValue::List(v) | GValue::Set(v) => v,
            other => std::slice::from_ref(other),
        };
        let prop = target.property_mut(key);
        prop.types = infer_types(values);
        prop.samples = values
            .iter()
            .take(SAMPLES_PER_PROPERTY)
            .map(|v| truncate_sample(&console::format(v)))
            .collect();
    }
}

/// Type names by frequency (ties keep first-seen order).
pub fn infer_types(values: &[GValue]) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for v in values {
        let name = v.type_name();
        match counts.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => *c += 1,
            None => counts.push((name.to_owned(), 1)),
        }
    }
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    counts.into_iter().map(|(n, _)| n).collect()
}

/// Every `{o, i}` pair from a triples query.
pub fn triples_from(label: &str, items: &[GValue]) -> Vec<Triple> {
    items
        .iter()
        .filter_map(|item| {
            let GValue::Map(map) = item else {
                return None;
            };
            let out = map_get(map, "o")?.as_str()?;
            let inn = map_get(map, "i")?.as_str()?;
            Some(Triple::new(out, label, inn))
        })
        .collect()
}
