//! `fa2_iteration_50k`: one ForceAtlas2 iteration on a 50k-node / 100k-edge graph (budget < 50 ms,
//! graph-layouts design).

use criterion::{Criterion, criterion_group, criterion_main};
use graph_layout::{Fa2Options, ForceAtlas2, Layout, LayoutGraph, Rng, seeded_positions};

fn graph(nodes: usize, edges: usize) -> LayoutGraph {
    let mut rng = Rng::new(9);
    let below = |rng: &mut Rng, n: usize| (rng.next_u64() % n as u64) as u32;
    let mut list: Vec<[u32; 2]> = Vec::with_capacity(edges);
    for i in 1..nodes as u32 {
        list.push([below(&mut rng, i as usize), i]);
    }
    for _ in 0..edges.saturating_sub(nodes) {
        list.push([below(&mut rng, nodes), below(&mut rng, nodes)]);
    }
    LayoutGraph::new(nodes, list)
}

fn fa2_iteration_50k(c: &mut Criterion) {
    let g = graph(50_000, 100_000);
    let pinned = vec![false; g.node_count()];
    let mut pos = seeded_positions(g.node_count(), 1);
    let mut fa2 = ForceAtlas2::new(&g, Fa2Options::default());
    // Warm up into a realistic mid-layout state.
    for _ in 0..20 {
        fa2.step(&g, &mut pos, &pinned);
    }
    c.bench_function("fa2_iteration_50k", |b| {
        b.iter(|| fa2.step(&g, &mut pos, &pinned));
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = fa2_iteration_50k
}
criterion_main!(benches);
