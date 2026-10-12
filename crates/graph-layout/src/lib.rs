//! Graph layout algorithms (ADR-0009, ADR-0010): ForceAtlas2 with Barnes-Hut repulsion (the default),
//! hierarchical (Sugiyama), radial, circular, grid and concentric, plus a [`worker`] thread that runs them
//! off the UI thread and publishes position frames.
//!
//! Layouts work on a [`LayoutGraph`] (node count plus edges, independent of the store) and a positions
//! slice. Pinned nodes never move. Everything is deterministic for a given input and seed.

pub mod dataset;
pub mod fa2;
pub mod hierarchical;
pub mod simple;
pub mod worker;

pub use fa2::{Fa2Options, ForceAtlas2};
pub use hierarchical::{Direction, HierarchicalOptions};

pub type Vec2 = [f32; 2];

/// Edges as node index pairs (directed: `[source, target]`), plus an undirected adjacency in CSR form.
#[derive(Clone, Debug, Default)]
pub struct LayoutGraph {
    node_count: usize,
    edges: Vec<[u32; 2]>,
    offsets: Vec<u32>,
    neighbours: Vec<u32>,
}

impl LayoutGraph {
    /// Builds the graph. Self-loops and edges with out-of-range endpoints are dropped.
    pub fn new(node_count: usize, edges: impl IntoIterator<Item = [u32; 2]>) -> Self {
        let edges: Vec<[u32; 2]> = edges
            .into_iter()
            .filter(|[a, b]| a != b && (*a as usize) < node_count && (*b as usize) < node_count)
            .collect();
        let mut degree = vec![0u32; node_count];
        for [a, b] in &edges {
            degree[*a as usize] += 1;
            degree[*b as usize] += 1;
        }
        let mut offsets = Vec::with_capacity(node_count + 1);
        offsets.push(0u32);
        for d in &degree {
            offsets.push(offsets.last().copied().unwrap_or(0) + d);
        }
        let mut fill: Vec<u32> = offsets[..node_count].to_vec();
        let mut neighbours = vec![0u32; edges.len() * 2];
        for [a, b] in &edges {
            neighbours[fill[*a as usize] as usize] = *b;
            fill[*a as usize] += 1;
            neighbours[fill[*b as usize] as usize] = *a;
            fill[*b as usize] += 1;
        }
        Self {
            node_count,
            edges,
            offsets,
            neighbours,
        }
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn edges(&self) -> &[[u32; 2]] {
        &self.edges
    }

    /// Undirected neighbours of `n` (with multiplicity).
    pub fn neighbours(&self, n: usize) -> &[u32] {
        &self.neighbours[self.offsets[n] as usize..self.offsets[n + 1] as usize]
    }

    pub fn degree(&self, n: usize) -> usize {
        (self.offsets[n + 1] - self.offsets[n]) as usize
    }
}

/// Did a step finish the layout?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepResult {
    Running,
    Converged,
}

/// An iterative layout: each step moves the unpinned nodes.
pub trait Layout: Send {
    fn step(&mut self, g: &LayoutGraph, pos: &mut [Vec2], pinned: &[bool]) -> StepResult;
}

/// The layouts on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    ForceAtlas2,
    Hierarchical(Direction),
    Radial,
    Circular,
    Grid,
    Concentric,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 7] = [
        LayoutKind::ForceAtlas2,
        LayoutKind::Hierarchical(Direction::TopDown),
        LayoutKind::Hierarchical(Direction::LeftRight),
        LayoutKind::Radial,
        LayoutKind::Circular,
        LayoutKind::Grid,
        LayoutKind::Concentric,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LayoutKind::ForceAtlas2 => "Force",
            LayoutKind::Hierarchical(Direction::TopDown) => "Hierarchical (top-down)",
            LayoutKind::Hierarchical(Direction::LeftRight) => "Hierarchical (left-right)",
            LayoutKind::Radial => "Radial",
            LayoutKind::Circular => "Circular",
            LayoutKind::Grid => "Grid",
            LayoutKind::Concentric => "Concentric",
        }
    }

    /// Stable identifier for persistence.
    pub fn id(self) -> &'static str {
        match self {
            LayoutKind::ForceAtlas2 => "fa2",
            LayoutKind::Hierarchical(Direction::TopDown) => "hierarchical-td",
            LayoutKind::Hierarchical(Direction::LeftRight) => "hierarchical-lr",
            LayoutKind::Radial => "radial",
            LayoutKind::Circular => "circular",
            LayoutKind::Grid => "grid",
            LayoutKind::Concentric => "concentric",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// Iterative layouts animate step by step; the others compute targets once and are tweened.
    pub fn is_iterative(self) -> bool {
        self == LayoutKind::ForceAtlas2
    }
}

/// Options for every layout kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOptions {
    pub fa2: Fa2Options,
    pub hierarchical: HierarchicalOptions,
    /// Distance between neighbouring nodes in the one-shot layouts (world units).
    pub spacing: f32,
    pub seed: u64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            fa2: Fa2Options::default(),
            hierarchical: HierarchicalOptions::default(),
            spacing: 40.0,
            seed: 42,
        }
    }
}

/// Target positions for a one-shot layout. Pinned nodes keep their positions; `focus` is the centre of a
/// radial layout (the selection).
pub fn one_shot(
    kind: LayoutKind,
    g: &LayoutGraph,
    pos: &[Vec2],
    pinned: &[bool],
    focus: &[u32],
    opts: &LayoutOptions,
) -> Vec<Vec2> {
    let mut out = match kind {
        LayoutKind::ForceAtlas2 => {
            let mut p = pos.to_vec();
            let mut fa2 = ForceAtlas2::new(g, opts.fa2);
            while fa2.step(g, &mut p, pinned) == StepResult::Running {}
            p
        }
        LayoutKind::Hierarchical(direction) => {
            let mut h = opts.hierarchical;
            h.direction = direction;
            hierarchical::layout(g, &h)
        }
        LayoutKind::Radial => simple::radial(g, focus, opts.spacing),
        LayoutKind::Circular => simple::circular(g, opts.spacing),
        LayoutKind::Grid => simple::grid(g, opts.spacing),
        LayoutKind::Concentric => simple::concentric(g, opts.spacing),
    };
    if kind != LayoutKind::ForceAtlas2 {
        anchor(&mut out, pos, pinned, focus);
    }
    for (i, p) in out.iter_mut().enumerate() {
        if pinned.get(i).copied().unwrap_or(false) {
            *p = pos[i];
        }
    }
    out
}

/// Moves a freshly computed layout so it stays where the graph was: the focus node (radial) keeps its
/// place, otherwise the centroid of the unpinned nodes is preserved.
fn anchor(out: &mut [Vec2], pos: &[Vec2], pinned: &[bool], focus: &[u32]) {
    if out.is_empty() {
        return;
    }
    let (from, to) = match focus.first() {
        Some(&f) if (f as usize) < out.len() => (out[f as usize], pos[f as usize]),
        _ => {
            let free: Vec<usize> = (0..out.len())
                .filter(|i| !pinned.get(*i).copied().unwrap_or(false))
                .collect();
            if free.is_empty() {
                return;
            }
            let c = |v: &[Vec2]| {
                let (sx, sy) = free
                    .iter()
                    .fold((0.0, 0.0), |(x, y), i| (x + v[*i][0], y + v[*i][1]));
                [sx / free.len() as f32, sy / free.len() as f32]
            };
            (c(out), c(pos))
        }
    };
    let d = [to[0] - from[0], to[1] - from[1]];
    for p in out.iter_mut() {
        p[0] += d[0];
        p[1] += d[1];
    }
}

/// SplitMix64: tiny, deterministic, good enough for initial positions and jitter.
#[derive(Clone, Debug)]
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

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Random initial positions in a disc sized for `n` nodes.
pub fn seeded_positions(n: usize, seed: u64) -> Vec<Vec2> {
    let mut rng = Rng::new(seed);
    let r = 20.0 * (n as f32).sqrt().max(1.0);
    (0..n)
        .map(|_| {
            let a = rng.unit() * std::f32::consts::TAU;
            let d = r * rng.unit().sqrt();
            [d * a.cos(), d * a.sin()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csr_adjacency() {
        let g = LayoutGraph::new(4, [[0, 1], [0, 2], [2, 2], [1, 9]]);
        assert_eq!(
            g.edges().len(),
            2,
            "self-loop and out-of-range edge dropped"
        );
        assert_eq!(g.neighbours(0), &[1, 2]);
        assert_eq!(g.neighbours(2), &[0]);
        assert_eq!(g.degree(3), 0);
    }

    #[test]
    fn kinds_round_trip_ids() {
        for k in LayoutKind::ALL {
            assert_eq!(LayoutKind::from_id(k.id()), Some(k));
        }
    }

    #[test]
    fn pinned_nodes_keep_positions_in_one_shot_layouts() {
        let g = LayoutGraph::new(5, [[0, 1], [1, 2], [2, 3], [3, 4]]);
        let pos = seeded_positions(5, 1);
        let pinned = [false, false, true, false, false];
        for kind in LayoutKind::ALL {
            let out = one_shot(kind, &g, &pos, &pinned, &[0], &LayoutOptions::default());
            assert_eq!(out[2], pos[2], "{kind:?}");
            assert!(
                out.iter().all(|p| p[0].is_finite() && p[1].is_finite()),
                "{kind:?}"
            );
        }
    }
}
