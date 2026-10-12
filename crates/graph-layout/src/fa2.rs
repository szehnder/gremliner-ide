//! ForceAtlas2 (Jacomy et al., 2014) with Barnes-Hut repulsion: degree-weighted repulsion, linear
//! attraction along edges, gravity towards the origin, and the paper's adaptive global and local speeds
//! (swinging/traction). Repulsion is computed per node in parallel against an arena quadtree rebuilt every
//! iteration; everything else is sequential so results are deterministic.

use rayon::prelude::*;

use crate::{Layout, LayoutGraph, StepResult, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fa2Options {
    /// Repulsion strength (larger spreads the graph out).
    pub scaling: f32,
    /// Pull towards the origin, which keeps disconnected components together.
    pub gravity: f32,
    /// Gravity grows with distance (compact, round layouts).
    pub strong_gravity: bool,
    /// Divide hub attraction by degree so hubs sit in the middle of their neighbours ("dissuade hubs").
    pub dissuade_hubs: bool,
    /// Barnes-Hut accuracy: larger is faster and coarser.
    pub theta: f32,
    pub jitter_tolerance: f32,
    /// Converged when the mean node displacement per iteration falls below this fraction of the layout's
    /// size.
    pub tolerance: f32,
    pub max_iterations: usize,
    /// After this many iterations, movement is damped by `cooling` per iteration so layouts that keep
    /// jittering (small graphs do) still come to rest.
    pub cool_after: usize,
    pub cooling: f32,
}

impl Default for Fa2Options {
    fn default() -> Self {
        Self {
            scaling: 60.0,
            // Strong gravity keeps leaves and small components from drifting off: with plain gravity
            // their equilibrium distance grows with the total mass of the graph.
            gravity: 0.1,
            strong_gravity: true,
            dissuade_hubs: false,
            theta: 1.2,
            jitter_tolerance: 1.0,
            tolerance: 1e-4,
            max_iterations: 1_500,
            cool_after: 400,
            cooling: 0.99,
        }
    }
}

pub struct ForceAtlas2 {
    opts: Fa2Options,
    mass: Vec<f32>,
    forces: Vec<Vec2>,
    old_forces: Vec<Vec2>,
    speed: f32,
    speed_efficiency: f32,
    iteration: usize,
    /// Recent mean displacements, to smooth the convergence test.
    recent: [f32; 8],
}

impl ForceAtlas2 {
    pub fn new(g: &LayoutGraph, opts: Fa2Options) -> Self {
        let n = g.node_count();
        Self {
            opts,
            mass: (0..n).map(|i| g.degree(i) as f32 + 1.0).collect(),
            forces: vec![[0.0, 0.0]; n],
            old_forces: vec![[0.0, 0.0]; n],
            speed: 1.0,
            speed_efficiency: 1.0,
            iteration: 0,
            recent: [f32::INFINITY; 8],
        }
    }

    pub fn iteration(&self) -> usize {
        self.iteration
    }

    /// Mean displacement over the last few iterations, as a fraction of the layout's size.
    pub fn recent_displacement(&self) -> f32 {
        self.recent.iter().sum::<f32>() / self.recent.len() as f32
    }

    fn repulsion_and_gravity(&mut self, pos: &[Vec2], pinned: &[bool]) {
        let tree = QuadTree::build(pos, &self.mass);
        let kr = self.opts.scaling;
        let kg = self.opts.gravity;
        let strong = self.opts.strong_gravity;
        let theta2 = self.opts.theta * self.opts.theta;
        let mass = &self.mass;
        self.forces.par_iter_mut().enumerate().for_each(|(i, f)| {
            if pinned.get(i).copied().unwrap_or(false) {
                *f = [0.0, 0.0];
                return;
            }
            let p = pos[i];
            let mut acc = tree.repulsion(i as u32, p, mass[i], kr, theta2);
            // Gravity towards the origin.
            let d = (p[0] * p[0] + p[1] * p[1]).sqrt();
            if d > 0.0 {
                let factor = if strong {
                    kg * mass[i]
                } else {
                    kg * mass[i] / d
                };
                acc[0] -= p[0] * factor;
                acc[1] -= p[1] * factor;
            }
            *f = acc;
        });
    }

    fn attraction(&mut self, g: &LayoutGraph, pos: &[Vec2]) {
        let hubs = self.opts.dissuade_hubs;
        for [a, b] in g.edges() {
            let (a, b) = (*a as usize, *b as usize);
            let dx = pos[a][0] - pos[b][0];
            let dy = pos[a][1] - pos[b][1];
            let (fa, fb) = if hubs {
                (1.0 / self.mass[a], 1.0 / self.mass[b])
            } else {
                (1.0, 1.0)
            };
            self.forces[a][0] -= dx * fa;
            self.forces[a][1] -= dy * fa;
            self.forces[b][0] += dx * fb;
            self.forces[b][1] += dy * fb;
        }
    }
}

impl Layout for ForceAtlas2 {
    fn step(&mut self, g: &LayoutGraph, pos: &mut [Vec2], pinned: &[bool]) -> StepResult {
        let n = pos.len().min(g.node_count());
        if n == 0 {
            return StepResult::Converged;
        }
        std::mem::swap(&mut self.forces, &mut self.old_forces);
        self.repulsion_and_gravity(&pos[..n], pinned);
        self.attraction(g, pos);
        let is_pinned = |i: usize| pinned.get(i).copied().unwrap_or(false);

        // Global speed from swinging (erratic movement) and traction (useful movement).
        let (mut swinging, mut traction) = (0.0f32, 0.0f32);
        for i in 0..n {
            if is_pinned(i) {
                continue;
            }
            let (f, o) = (self.forces[i], self.old_forces[i]);
            let sw = ((f[0] - o[0]).powi(2) + (f[1] - o[1]).powi(2)).sqrt();
            let tr = ((f[0] + o[0]).powi(2) + (f[1] + o[1]).powi(2)).sqrt() * 0.5;
            swinging += self.mass[i] * sw;
            traction += self.mass[i] * tr;
        }
        let nf = n as f32;
        let estimated_jitter = 0.05 * nf.sqrt();
        let min_jt = estimated_jitter.sqrt();
        let max_jt: f32 = 10.0;
        let mut jt = self.opts.jitter_tolerance
            * min_jt.max(max_jt.min(estimated_jitter * traction / (nf * nf)));
        let min_speed_efficiency = 0.05;
        if traction > 0.0 && swinging / traction > 2.0 {
            if self.speed_efficiency > min_speed_efficiency {
                self.speed_efficiency *= 0.5;
            }
            jt = jt.max(self.opts.jitter_tolerance);
        }
        let target_speed = if swinging > 0.0 {
            jt * self.speed_efficiency * traction / swinging
        } else {
            self.speed
        };
        if swinging > jt * traction {
            if self.speed_efficiency > min_speed_efficiency {
                self.speed_efficiency *= 0.7;
            }
        } else if self.speed < 1000.0 {
            self.speed_efficiency *= 1.3;
        }
        let max_rise = 0.5;
        self.speed += (target_speed - self.speed).min(max_rise * self.speed);

        // Move: each node's speed is damped by its own swinging (and by cooling late in the run).
        let cool = self
            .opts
            .cooling
            .powi(self.iteration.saturating_sub(self.opts.cool_after) as i32);
        let mut moved = 0.0f32;
        let mut free = 0usize;
        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for (i, p) in pos.iter_mut().enumerate().take(n) {
            if is_pinned(i) {
                continue;
            }
            let (f, o) = (self.forces[i], self.old_forces[i]);
            let sw = self.mass[i] * ((f[0] - o[0]).powi(2) + (f[1] - o[1]).powi(2)).sqrt();
            let mut factor = cool * self.speed / (1.0 + (self.speed * sw).sqrt());
            // Never jump further than 10 units per unit of force magnitude in one step.
            let fl = (f[0] * f[0] + f[1] * f[1]).sqrt();
            if fl * factor > 10.0 * self.opts.scaling.sqrt() {
                factor = 10.0 * self.opts.scaling.sqrt() / fl;
            }
            p[0] += f[0] * factor;
            p[1] += f[1] * factor;
            moved += fl * factor;
            free += 1;
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1.0);
        self.iteration += 1;
        let mean = if free == 0 {
            0.0
        } else {
            moved / free as f32 / extent
        };
        self.recent[self.iteration % self.recent.len()] = mean;
        if free == 0
            || self.iteration >= self.opts.max_iterations
            || (self.iteration > self.recent.len()
                && self.recent_displacement() < self.opts.tolerance)
        {
            StepResult::Converged
        } else {
            StepResult::Running
        }
    }
}

// ---- Barnes-Hut quadtree ---------------------------------------------------------------------------

const NONE: u32 = u32::MAX;
/// Below this cell size, coincident points share a leaf instead of splitting forever.
const MAX_DEPTH: u32 = 24;

#[derive(Clone, Copy)]
struct Cell {
    centre: Vec2,
    half: f32,
    mass: f32,
    /// Mass-weighted position sum (divided by mass on use).
    com: Vec2,
    /// First child index (four consecutive cells), or NONE for a leaf.
    children: u32,
    /// The single body in a leaf, or NONE.
    body: u32,
    depth: u32,
}

struct QuadTree {
    cells: Vec<Cell>,
}

impl QuadTree {
    fn build(pos: &[Vec2], mass: &[f32]) -> Self {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for p in pos {
            min = [min[0].min(p[0]), min[1].min(p[1])];
            max = [max[0].max(p[0]), max[1].max(p[1])];
        }
        let half = ((max[0] - min[0]).max(max[1] - min[1]) * 0.5).max(1.0) * 1.01;
        let root = Cell {
            centre: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            half,
            mass: 0.0,
            com: [0.0, 0.0],
            children: NONE,
            body: NONE,
            depth: 0,
        };
        let mut tree = QuadTree {
            cells: Vec::with_capacity(pos.len() * 2 + 1),
        };
        tree.cells.push(root);
        for (i, p) in pos.iter().enumerate() {
            tree.insert(i as u32, *p, mass[i], pos, mass);
        }
        tree
    }

    fn quadrant(c: &Cell, p: Vec2) -> u32 {
        u32::from(p[0] >= c.centre[0]) | (u32::from(p[1] >= c.centre[1]) << 1)
    }

    fn split(&mut self, ix: usize) {
        let c = self.cells[ix];
        let h = c.half * 0.5;
        let first = self.cells.len() as u32;
        for q in 0..4u32 {
            let dx = if q & 1 == 1 { h } else { -h };
            let dy = if q & 2 == 2 { h } else { -h };
            self.cells.push(Cell {
                centre: [c.centre[0] + dx, c.centre[1] + dy],
                half: h,
                mass: 0.0,
                com: [0.0, 0.0],
                children: NONE,
                body: NONE,
                depth: c.depth + 1,
            });
        }
        self.cells[ix].children = first;
    }

    fn insert(&mut self, body: u32, p: Vec2, m: f32, pos: &[Vec2], mass: &[f32]) {
        let mut ix = 0usize;
        loop {
            {
                let c = &mut self.cells[ix];
                c.mass += m;
                c.com[0] += p[0] * m;
                c.com[1] += p[1] * m;
            }
            let c = self.cells[ix];
            if c.children != NONE {
                ix = (c.children + Self::quadrant(&c, p)) as usize;
                continue;
            }
            if c.body == NONE && c.mass == m {
                // Empty leaf (its mass is just this body's).
                self.cells[ix].body = body;
                return;
            }
            if c.depth >= MAX_DEPTH {
                // Coincident points: keep aggregating in this leaf.
                return;
            }
            // Occupied leaf: push the existing body down, then continue with the new one.
            let existing = c.body;
            self.split(ix);
            self.cells[ix].body = NONE;
            if existing != NONE {
                // The children are fresh and empty: the existing body goes straight into its quadrant
                // (its mass is already counted at `ix`).
                let ep = pos[existing as usize];
                let em = mass[existing as usize];
                let child =
                    (self.cells[ix].children + Self::quadrant(&self.cells[ix], ep)) as usize;
                let cc = &mut self.cells[child];
                cc.mass += em;
                cc.com[0] += ep[0] * em;
                cc.com[1] += ep[1] * em;
                cc.body = existing;
            }
            ix = (self.cells[ix].children + Self::quadrant(&self.cells[ix], p)) as usize;
        }
    }

    /// Repulsion on body `i` at `p` with mass `m`.
    fn repulsion(&self, i: u32, p: Vec2, m: f32, kr: f32, theta2: f32) -> Vec2 {
        let mut f = [0.0f32, 0.0f32];
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);
        while let Some(ix) = stack.pop() {
            let c = &self.cells[ix as usize];
            if c.mass <= 0.0 {
                continue;
            }
            let leaf = c.children == NONE;
            if leaf && c.body == i {
                // The body itself; coincident extras (beyond MAX_DEPTH) are ignored.
                continue;
            }
            let com = [c.com[0] / c.mass, c.com[1] / c.mass];
            let dx = p[0] - com[0];
            let dy = p[1] - com[1];
            let d2 = dx * dx + dy * dy;
            let size = c.half * 2.0;
            if leaf || size * size < theta2 * d2 {
                if d2 > 1e-6 {
                    let factor = kr * m * c.mass / d2;
                    f[0] += dx * factor;
                    f[1] += dy * factor;
                }
            } else {
                for q in 0..4 {
                    stack.push(c.children + q);
                }
            }
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_positions;

    fn ring_of_cliques(cliques: usize, size: usize) -> LayoutGraph {
        let mut edges = Vec::new();
        for c in 0..cliques {
            let base = (c * size) as u32;
            for a in 0..size as u32 {
                for b in (a + 1)..size as u32 {
                    edges.push([base + a, base + b]);
                }
            }
            edges.push([base, (((c + 1) % cliques) * size) as u32]);
        }
        LayoutGraph::new(cliques * size, edges)
    }

    fn run(g: &LayoutGraph, pos: &mut [Vec2], pinned: &[bool]) -> usize {
        let mut fa2 = ForceAtlas2::new(g, Fa2Options::default());
        while fa2.step(g, pos, pinned) == StepResult::Running {}
        fa2.iteration()
    }

    #[test]
    fn scenario_graph_layouts_same_seed_same_result() {
        let g = ring_of_cliques(8, 6);
        let mut a = seeded_positions(g.node_count(), 7);
        let mut b = seeded_positions(g.node_count(), 7);
        let pinned = vec![false; g.node_count()];
        run(&g, &mut a, &pinned);
        run(&g, &mut b, &pinned);
        assert_eq!(a, b);
    }

    #[test]
    fn scenario_graph_layouts_pinned_node_stays() {
        let g = ring_of_cliques(4, 5);
        let mut pos = seeded_positions(g.node_count(), 3);
        let mut pinned = vec![false; g.node_count()];
        pinned[7] = true;
        let before = pos[7];
        run(&g, &mut pos, &pinned);
        assert_eq!(pos[7], before);
        assert_ne!(
            pos[8],
            seeded_positions(g.node_count(), 3)[8],
            "others moved"
        );
    }

    #[test]
    fn converges_and_separates_clusters() {
        let g = ring_of_cliques(6, 8);
        let mut pos = seeded_positions(g.node_count(), 11);
        let iterations = run(&g, &mut pos, &vec![false; g.node_count()]);
        assert!(
            iterations < Fa2Options::default().max_iterations,
            "converged in {iterations}"
        );
        // Mean distance within a clique is well below the mean distance between clique centres.
        let centre = |c: usize| {
            let (sx, sy) = (0..8).fold((0.0, 0.0), |(x, y), k| {
                (x + pos[c * 8 + k][0], y + pos[c * 8 + k][1])
            });
            [sx / 8.0, sy / 8.0]
        };
        let dist = |a: Vec2, b: Vec2| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let within: f32 = (0..6)
            .map(|c| {
                (1..8)
                    .map(|k| dist(pos[c * 8], pos[c * 8 + k]))
                    .sum::<f32>()
                    / 7.0
            })
            .sum::<f32>()
            / 6.0;
        let between: f32 = (0..6)
            .map(|c| dist(centre(c), centre((c + 1) % 6)))
            .sum::<f32>()
            / 6.0;
        assert!(within * 1.5 < between, "within {within}, between {between}");
        // Nodes do not collapse onto each other.
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                assert!(dist(pos[i], pos[j]) > 1.0, "{i} and {j} overlap");
            }
        }
    }

    #[test]
    fn quadtree_mass_is_conserved() {
        let pos = seeded_positions(500, 5);
        let mass: Vec<f32> = (0..500).map(|i| 1.0 + (i % 7) as f32).collect();
        let tree = QuadTree::build(&pos, &mass);
        let total: f32 = mass.iter().sum();
        assert!((tree.cells[0].mass - total).abs() < 1e-2);
        // Every body is in exactly one leaf.
        let leaves = tree
            .cells
            .iter()
            .filter(|c| c.children == NONE && c.body != NONE)
            .count();
        assert_eq!(leaves, 500);
    }
}
