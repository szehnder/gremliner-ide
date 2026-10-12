//! `gremliner-cli layout-bench`: time a layout on a GraphML dataset (by default the testenv's
//! air-routes) and report iterations and convergence time (ADR-0010, graph-layouts budget: < 2 s).

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context as _, bail};
use graph_layout::{
    ForceAtlas2, Layout, LayoutKind, LayoutOptions, StepResult, dataset, one_shot, seeded_positions,
};

pub struct Args {
    pub dataset: String,
    pub layout: String,
    pub seed: u64,
}

/// `air-routes` → `testenv/data/air-routes.graphml`; anything else is a path.
fn resolve(dataset: &str) -> PathBuf {
    let named = PathBuf::from(format!("testenv/data/{dataset}.graphml"));
    if named.exists() {
        named
    } else {
        PathBuf::from(dataset)
    }
}

pub fn run(a: &Args) -> anyhow::Result<()> {
    let path = resolve(&a.dataset);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {} (run from the repository root)", path.display()))?;
    let t = Instant::now();
    let g = dataset::parse_graphml(&text);
    println!(
        "{}: {} nodes, {} edges (parsed in {:.0} ms)",
        path.display(),
        g.node_count(),
        g.edges().len(),
        t.elapsed().as_secs_f32() * 1000.0
    );
    let Some(kind) = LayoutKind::from_id(&a.layout) else {
        let ids: Vec<&str> = LayoutKind::ALL.iter().map(|k| k.id()).collect();
        bail!("unknown layout {:?}; one of {}", a.layout, ids.join(", "));
    };
    let opts = LayoutOptions {
        seed: a.seed,
        ..LayoutOptions::default()
    };
    let mut pos = seeded_positions(g.node_count(), a.seed);
    let pinned = vec![false; g.node_count()];
    let t = Instant::now();
    if kind == LayoutKind::ForceAtlas2 {
        let mut fa2 = ForceAtlas2::new(&g, opts.fa2);
        let mut result = StepResult::Running;
        while result == StepResult::Running {
            result = fa2.step(&g, &mut pos, &pinned);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let capped = fa2.iteration() >= opts.fa2.max_iterations;
        println!(
            "{}: {} iterations in {ms:.0} ms ({:.2} ms/iteration){}",
            kind.name(),
            fa2.iteration(),
            ms / fa2.iteration().max(1) as f64,
            if capped {
                " — hit the iteration cap without converging"
            } else {
                ", converged"
            }
        );
    } else {
        let out = one_shot(kind, &g, &pos, &pinned, &[], &opts);
        println!(
            "{}: {} positions in {:.0} ms",
            kind.name(),
            out.len(),
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}
