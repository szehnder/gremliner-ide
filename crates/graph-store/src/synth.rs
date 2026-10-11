//! Deterministic synthetic graphs for benchmarks and tests.

use crate::{ExtId, GraphStore};

/// SplitMix64: tiny and deterministic.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Builds a graph with `nodes` nodes and `edges` edges, `labels` vertex labels, positioned in clustered
/// blobs (one per label) so it looks plausible before a real layout runs.
pub fn generate(nodes: usize, edges: usize, labels: usize, seed: u64) -> GraphStore {
    let mut rng = Rng::new(seed);
    let mut g = GraphStore::new();
    let labels = labels.max(1);
    let spread = (nodes as f32).sqrt() * 12.0;
    let centers: Vec<[f32; 2]> = (0..labels)
        .map(|l| {
            let a = l as f32 / labels as f32 * std::f32::consts::TAU;
            [a.cos() * spread * 0.6, a.sin() * spread * 0.6]
        })
        .collect();
    let label_names: Vec<String> = (0..labels).map(|l| format!("L{l}")).collect();
    for i in 0..nodes {
        let l = i % labels;
        // Gaussian-ish blob via sum of uniforms.
        let r = spread * 0.35 * (rng.unit() + rng.unit() + rng.unit()) / 3.0;
        let a = rng.unit() * std::f32::consts::TAU;
        let c = centers[l];
        g.add_node(
            ExtId::Int(i as i64),
            &label_names[l],
            &format!("n{i}"),
            [c[0] + r * a.cos(), c[1] + r * a.sin()],
        );
    }
    if nodes == 0 {
        return g;
    }
    for e in 0..edges {
        let from = rng.below(nodes);
        // Mostly local edges (same label neighbourhood) with some long-range hub links.
        let to = if rng.unit() < 0.8 {
            let off = rng.below(64 * labels) as i64 - 32 * labels as i64;
            ((from as i64 + off).rem_euclid(nodes as i64)) as usize
        } else {
            ((rng.unit().powi(2)) * nodes as f32) as usize % nodes
        };
        g.add_edge(
            ExtId::Int(e as i64),
            crate::NodeIx(from as u32),
            crate::NodeIx(to as u32),
            "link",
        );
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate(1000, 2000, 4, 9);
        let b = generate(1000, 2000, 4, 9);
        assert_eq!(a.positions(), b.positions());
        assert_eq!(a.edge_endpoints(), b.edge_endpoints());
        assert_eq!(a.node_count(), 1000);
        assert_eq!(a.edge_count(), 2000);
        assert_eq!(a.vertex_label_count(), 4);
    }
}
