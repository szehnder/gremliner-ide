//! `completion_latency`: completions plus signature help at a trigger position in a 200-line document
//! with the air-routes schema (budget: p95 ≤ 30 ms per keystroke, spec autocomplete).

#![allow(clippy::unwrap_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use gremlin_syntax::complete::{complete, signature_help};
use schema::Schema;

fn document() -> String {
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

fn completion_latency(c: &mut Criterion) {
    let schema =
        Schema::from_json(include_str!("../tests/fixtures/air-routes-schema.json")).unwrap();
    let doc = document();
    // Near the end of the document: the worst case for scanning from the start.
    let offset = doc.rfind(".values('").unwrap() + ".values('".len();
    c.bench_function("completion_latency", |b| {
        b.iter(|| {
            std::hint::black_box(complete(&doc, offset, Some(&schema)));
            std::hint::black_box(signature_help(&doc, offset, Some(&schema)));
        });
    });
}

criterion_group!(benches, completion_latency);
criterion_main!(benches);
