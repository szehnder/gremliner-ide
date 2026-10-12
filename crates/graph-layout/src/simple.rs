//! One-shot layouts: radial (rings by hop distance from the focus), circular, grid and concentric (rings
//! by degree). Node order follows a breadth-first traversal so neighbours end up close together.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use crate::{LayoutGraph, Vec2};

/// Breadth-first order over all components, each starting at its highest-degree node.
fn bfs_order(g: &LayoutGraph) -> Vec<usize> {
    let n = g.node_count();
    let mut by_degree: Vec<usize> = (0..n).collect();
    by_degree.sort_by_key(|v| (std::cmp::Reverse(g.degree(*v)), *v));
    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in by_degree {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut q = VecDeque::from([start]);
        while let Some(v) = q.pop_front() {
            order.push(v);
            for &w in g.neighbours(v) {
                let w = w as usize;
                if !seen[w] {
                    seen[w] = true;
                    q.push_back(w);
                }
            }
        }
    }
    order
}

/// Rings by hop distance from `focus` (or the highest-degree node). Children are ordered by their
/// parent's angle; unreachable nodes go on an outer ring.
pub fn radial(g: &LayoutGraph, focus: &[u32], spacing: f32) -> Vec<Vec2> {
    let n = g.node_count();
    let mut out = vec![[0.0, 0.0]; n];
    if n == 0 {
        return out;
    }
    let mut roots: Vec<usize> = focus
        .iter()
        .map(|f| *f as usize)
        .filter(|f| *f < n)
        .collect();
    if roots.is_empty() {
        roots.push(
            (0..n)
                .max_by_key(|v| (g.degree(*v), std::cmp::Reverse(*v)))
                .unwrap_or(0),
        );
    }
    let mut hop = vec![usize::MAX; n];
    let mut parent = vec![usize::MAX; n];
    let mut rings: Vec<Vec<usize>> = vec![Vec::new()];
    let mut q = VecDeque::new();
    for r in &roots {
        if hop[*r] == usize::MAX {
            hop[*r] = 0;
            rings[0].push(*r);
            q.push_back(*r);
        }
    }
    while let Some(v) = q.pop_front() {
        for &w in g.neighbours(v) {
            let w = w as usize;
            if hop[w] == usize::MAX {
                hop[w] = hop[v] + 1;
                parent[w] = v;
                if rings.len() <= hop[w] {
                    rings.push(Vec::new());
                }
                rings[hop[w]].push(w);
                q.push_back(w);
            }
        }
    }
    let unreached: Vec<usize> = (0..n).filter(|v| hop[*v] == usize::MAX).collect();
    if !unreached.is_empty() {
        rings.push(unreached);
    }
    let ring_gap = spacing * 2.5;
    let mut angle = vec![0.0f32; n];
    let mut radius = 0.0f32;
    for (k, ring) in rings.iter_mut().enumerate() {
        if k == 0 && ring.len() == 1 {
            out[ring[0]] = [0.0, 0.0];
            continue;
        }
        // Keep children next to their parents: order by the parent's angle.
        ring.sort_by(|a, b| {
            let pa = parent
                .get(*a)
                .filter(|p| **p != usize::MAX)
                .map_or(0.0, |p| angle[*p]);
            let pb = parent
                .get(*b)
                .filter(|p| **p != usize::MAX)
                .map_or(0.0, |p| angle[*p]);
            pa.total_cmp(&pb).then(a.cmp(b))
        });
        let needed = ring.len() as f32 * spacing / TAU;
        radius = (radius + ring_gap).max(needed).max(k as f32 * ring_gap);
        for (i, v) in ring.iter().enumerate() {
            let a = TAU * i as f32 / ring.len() as f32;
            angle[*v] = a;
            out[*v] = [radius * a.cos(), radius * a.sin()];
        }
    }
    out
}

/// All nodes on one circle, in breadth-first order.
pub fn circular(g: &LayoutGraph, spacing: f32) -> Vec<Vec2> {
    let n = g.node_count();
    let mut out = vec![[0.0, 0.0]; n];
    let radius = (n as f32 * spacing / TAU).max(spacing);
    for (i, v) in bfs_order(g).into_iter().enumerate() {
        let a = TAU * i as f32 / n.max(1) as f32;
        out[v] = [radius * a.cos(), radius * a.sin()];
    }
    out
}

/// A square grid, in breadth-first order.
pub fn grid(g: &LayoutGraph, spacing: f32) -> Vec<Vec2> {
    let n = g.node_count();
    let mut out = vec![[0.0, 0.0]; n];
    let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
    for (i, v) in bfs_order(g).into_iter().enumerate() {
        out[v] = [(i % cols) as f32 * spacing, (i / cols) as f32 * spacing];
    }
    out
}

/// Rings by degree: the best-connected node in the centre, then rings filled in degree order.
pub fn concentric(g: &LayoutGraph, spacing: f32) -> Vec<Vec2> {
    let n = g.node_count();
    let mut out = vec![[0.0, 0.0]; n];
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|v| (std::cmp::Reverse(g.degree(*v)), *v));
    let mut i = 0usize;
    let mut ring = 0usize;
    while i < n {
        if ring == 0 {
            out[order[0]] = [0.0, 0.0];
            i = 1;
            ring = 1;
            continue;
        }
        let r = ring as f32 * spacing * 1.5;
        let capacity = ((TAU * r / spacing).floor() as usize).max(1);
        let count = capacity.min(n - i);
        for k in 0..count {
            let a = TAU * k as f32 / count as f32 + ring as f32 * 0.4;
            out[order[i + k]] = [r * a.cos(), r * a.sin()];
        }
        i += count;
        ring += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: Vec2, b: Vec2) -> f32 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
    }

    fn no_overlap(pos: &[Vec2], min: f32) {
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                assert!(
                    dist(pos[i], pos[j]) >= min,
                    "{i} and {j}: {}",
                    dist(pos[i], pos[j])
                );
            }
        }
    }

    #[test]
    fn scenario_graph_layouts_radial_around_selection() {
        // A path 0-1-2-3 with a branch 1-4 and a satellite 5 hanging off 4.
        let g = LayoutGraph::new(6, [[0, 1], [1, 2], [2, 3], [1, 4], [4, 5]]);
        let pos = radial(&g, &[1], 40.0);
        assert_eq!(pos[1], [0.0, 0.0], "the selected node is at the centre");
        let r = |v: usize| dist(pos[v], [0.0, 0.0]);
        // Hop 1: 0, 2, 4. Hop 2: 3, 5.
        assert!((r(0) - r(2)).abs() < 1e-3 && (r(0) - r(4)).abs() < 1e-3);
        assert!((r(3) - r(5)).abs() < 1e-3);
        assert!(r(3) > r(0) + 1.0, "rings grow with hop distance");
        no_overlap(&pos, 39.0);
    }

    #[test]
    fn circular_grid_concentric_do_not_overlap() {
        let edges: Vec<[u32; 2]> = (1..60u32).map(|i| [i / 3, i]).collect();
        let g = LayoutGraph::new(60, edges);
        no_overlap(&circular(&g, 40.0), 39.0);
        no_overlap(&grid(&g, 40.0), 39.0);
        let c = concentric(&g, 40.0);
        no_overlap(&c, 39.0);
        // The highest-degree node is in the centre.
        let hub = (0..60)
            .max_by_key(|v| (g.degree(*v), std::cmp::Reverse(*v)))
            .unwrap();
        assert_eq!(c[hub], [0.0, 0.0]);
    }
}
