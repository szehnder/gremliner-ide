#![allow(clippy::unwrap_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use gremlin_syntax::Document;

fn doc_200_lines() -> String {
    (0..50)
        .map(|i| format!("g.V().hasLabel('airport').\n  has('code','A{i}').\n  out('route').\n  values('city')\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn incremental(c: &mut Criterion) {
    let text = doc_200_lines();
    let mut doc = Document::parse(text.clone());
    let at = text.len() / 2;
    let mut toggle = false;
    c.bench_function("incremental_reparse_200_lines", |b| {
        b.iter(|| {
            // Type a character, then delete it, re-analysing diagnostics each time.
            if toggle {
                doc.edit(at..at + 1, "");
            } else {
                doc.edit(at..at, "x");
            }
            toggle = !toggle;
            std::hint::black_box(doc.diagnostics());
        });
    });
}

criterion_group!(benches, incremental);
criterion_main!(benches);
