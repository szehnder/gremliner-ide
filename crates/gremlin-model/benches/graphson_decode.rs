//! GraphSON v3 decode throughput on a ~10 MB response of edges with properties. The budget
//! `graphson_decode_10mb` in perf-budgets.toml (median ≤ 60 ms) applies to `simd_decode_10mb`, the path the
//! client uses (ADR-0033); `graphson_decode_10mb` is the serde_json streaming variant, kept for comparison.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use gremlin_model::graphson::{decode, encode};
use gremlin_model::{Edge, GValue, Property};

fn payload() -> Vec<u8> {
    let mut edges = Vec::new();
    let mut i = 0i64;
    loop {
        let mut properties = gremlin_model::new_map();
        properties.insert(
            "dist".to_string(),
            Property {
                key: "dist".into(),
                value: Box::new(GValue::Int32((i % 5000) as i32)),
            },
        );
        edges.push(GValue::Edge(Edge {
            id: Box::new(GValue::Int64(i)),
            label: "route".into(),
            out_v: Box::new(GValue::Int64(i % 3500)),
            out_v_label: "airport".into(),
            in_v: Box::new(GValue::Int64((i * 7) % 3500)),
            in_v_label: "airport".into(),
            properties,
        }));
        i += 1;
        if i % 1000 == 0
            && serde_json::to_vec(&encode(&GValue::List(edges.clone()))).map_or(0, |v| v.len())
                >= 10_000_000
        {
            break;
        }
    }
    serde_json::to_vec(&encode(&GValue::List(edges))).expect("serialise")
}

fn bench(c: &mut Criterion) {
    let bytes = payload();
    eprintln!("payload: {:.1} MB", bytes.len() as f64 / 1e6);
    let mut g = c.benchmark_group("graphson");
    g.sample_size(20);
    g.bench_function("graphson_decode_10mb", |b| {
        b.iter(|| gremlin_model::fast::decode_slice(&bytes).expect("decode"))
    });
    g.bench_function("simd_decode_10mb", |b| {
        b.iter_batched(
            || bytes.clone(),
            |owned| gremlin_model::fast::decode_owned(owned).expect("decode"),
            criterion::BatchSize::LargeInput,
        )
    });
    g.bench_function("reference_decode_10mb", |b| {
        b.iter(|| {
            decode(&serde_json::from_slice::<serde_json::Value>(&bytes).expect("json"))
                .expect("decode")
        })
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
