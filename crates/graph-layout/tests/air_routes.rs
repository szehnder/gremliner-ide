//! ForceAtlas2 on the full air-routes graph (testenv/data/air-routes.graphml): convergence time budget.

use std::time::Instant;

use graph_layout::{Fa2Options, ForceAtlas2, Layout, StepResult, dataset, seeded_positions};

#[test]
fn scenario_graph_layouts_air_routes_converges_quickly() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testenv/data/air-routes.graphml"
    );
    let text = std::fs::read_to_string(path).expect("air-routes.graphml");
    let g = dataset::parse_graphml(&text);
    assert!(g.node_count() > 3_000 && g.edges().len() > 50_000);
    let mut pos = seeded_positions(g.node_count(), 42);
    let pinned = vec![false; g.node_count()];
    let opts = Fa2Options::default();
    let mut fa2 = ForceAtlas2::new(&g, opts);
    let started = Instant::now();
    let mut result = StepResult::Running;
    while result == StepResult::Running {
        result = fa2.step(&g, &mut pos, &pinned);
    }
    let elapsed = started.elapsed();
    println!(
        "measure fa2_air_routes_converge={:.1} ms",
        elapsed.as_secs_f64() * 1000.0
    );
    eprintln!(
        "air-routes: {} nodes, {} edges, {} iterations in {elapsed:?} ({:.2} ms/iteration)",
        g.node_count(),
        g.edges().len(),
        fa2.iteration(),
        elapsed.as_secs_f64() * 1000.0 / fa2.iteration() as f64
    );
    assert!(
        fa2.iteration() < opts.max_iterations,
        "reached the convergence threshold (not the iteration cap)"
    );
    assert!(elapsed.as_secs_f32() < 2.0, "converged in {elapsed:?}");
}
