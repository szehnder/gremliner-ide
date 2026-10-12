//! Hierarchical (Sugiyama-style) layout: break cycles (DFS back edges are reversed), assign layers by
//! longest path from the sources, insert dummy nodes on long edges, reduce crossings with barycentric
//! sweeps, then assign coordinates by pulling nodes towards their neighbours' mean under a minimum gap.
//! Weakly connected components are laid out side by side.

use std::collections::BTreeSet;

use crate::{LayoutGraph, Vec2};

/// Dummy nodes allowed per real node before long edges are no longer routed through layers.
const DUMMY_BUDGET: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    TopDown,
    LeftRight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HierarchicalOptions {
    pub direction: Direction,
    /// Distance between layers (world units).
    pub layer_gap: f32,
    /// Minimum distance between nodes in a layer.
    pub node_gap: f32,
}

impl Default for HierarchicalOptions {
    fn default() -> Self {
        Self {
            direction: Direction::TopDown,
            layer_gap: 90.0,
            node_gap: 40.0,
        }
    }
}

/// Positions for every node of `g`.
pub fn layout(g: &LayoutGraph, opts: &HierarchicalOptions) -> Vec<Vec2> {
    let n = g.node_count();
    let mut out = vec![[0.0, 0.0]; n];
    // Directed, de-duplicated adjacency.
    let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for [a, b] in g.edges() {
        succ[*a as usize].insert(*b as usize);
    }
    let succ: Vec<Vec<usize>> = succ.into_iter().map(|s| s.into_iter().collect()).collect();

    let mut x_offset = 0.0f32;
    for comp in components(g) {
        let local = layout_component(&comp, &succ, opts);
        // local: (node, x, layer) with x starting at 0.
        let width = local.iter().map(|(_, x, _)| *x).fold(0.0f32, f32::max);
        for (node, x, layer) in local {
            let along = x + x_offset;
            let across = layer as f32 * opts.layer_gap;
            out[node] = match opts.direction {
                Direction::TopDown => [along, across],
                Direction::LeftRight => [across, along],
            };
        }
        x_offset += width + opts.node_gap * 2.0;
    }
    out
}

/// Weakly connected components, each in ascending node order; components ordered by smallest node.
fn components(g: &LayoutGraph) -> Vec<Vec<usize>> {
    let n = g.node_count();
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut comp = vec![start];
        let mut stack = vec![start];
        while let Some(v) = stack.pop() {
            for &w in g.neighbours(v) {
                let w = w as usize;
                if !seen[w] {
                    seen[w] = true;
                    comp.push(w);
                    stack.push(w);
                }
            }
        }
        comp.sort_unstable();
        out.push(comp);
    }
    out
}

/// Lays out one component; returns `(node, x, layer)` for its real nodes.
fn layout_component(
    comp: &[usize],
    succ: &[Vec<usize>],
    opts: &HierarchicalOptions,
) -> Vec<(usize, f32, usize)> {
    let index: std::collections::HashMap<usize, usize> =
        comp.iter().enumerate().map(|(i, v)| (*v, i)).collect();
    let m = comp.len();
    // Local directed edges.
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, v) in comp.iter().enumerate() {
        for w in &succ[*v] {
            if let Some(&j) = index.get(w) {
                edges.push((i, j));
            }
        }
    }
    // 1. Cycle removal: reverse DFS back edges.
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); m];
    for (a, b) in &edges {
        out_adj[*a].push(*b);
    }
    let mut state = vec![0u8; m]; // 0 new, 1 on stack, 2 done
    let mut reversed: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut indeg = vec![0usize; m];
    for (_, b) in &edges {
        indeg[*b] += 1;
    }
    // Start DFS from sources first so trees keep their natural direction.
    let mut roots: Vec<usize> = (0..m).filter(|v| indeg[*v] == 0).collect();
    roots.extend(0..m);
    for root in roots {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        state[root] = 1;
        while let Some(&mut (v, ref mut k)) = stack.last_mut() {
            if *k < out_adj[v].len() {
                let w = out_adj[v][*k];
                *k += 1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => {
                        reversed.insert((v, w));
                    }
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    let dag: Vec<(usize, usize)> = edges
        .iter()
        .map(|&(a, b)| {
            if reversed.contains(&(a, b)) {
                (b, a)
            } else {
                (a, b)
            }
        })
        .collect();

    // 2. Longest-path layering (Kahn's order).
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); m];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); m];
    let mut indeg = vec![0usize; m];
    for &(a, b) in &dag {
        succs[a].push(b);
        preds[b].push(a);
        indeg[b] += 1;
    }
    let mut layer = vec![0usize; m];
    let mut queue: std::collections::VecDeque<usize> = (0..m).filter(|v| indeg[*v] == 0).collect();
    while let Some(v) = queue.pop_front() {
        for &w in &succs[v] {
            layer[w] = layer[w].max(layer[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }

    // 3. Dummy nodes on edges spanning several layers (nodes >= m are dummies). Dense cyclic graphs
    // get deep layerings where dummies would explode; past the budget long edges are drawn straight.
    let span: usize = dag.iter().map(|&(a, b)| layer[b] - layer[a] - 1).sum();
    let use_dummies = span <= m * DUMMY_BUDGET;
    let mut node_layer = layer.clone();
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); m];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); m];
    for &(a, b) in &dag {
        let mut prev = a;
        let chain = if use_dummies {
            (layer[a] + 1)..layer[b]
        } else {
            0..0
        };
        for l in chain {
            let d = node_layer.len();
            node_layer.push(l);
            up.push(Vec::new());
            down.push(Vec::new());
            down[prev].push(d);
            up[d].push(prev);
            prev = d;
        }
        down[prev].push(b);
        up[b].push(prev);
    }
    let total = node_layer.len();
    let layers = node_layer.iter().max().map_or(0, |l| l + 1);
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    // Initial order: DFS discovery from the top layer keeps subtrees together.
    let mut placed = vec![false; total];
    let mut order_stack: Vec<usize> = (0..m).filter(|v| node_layer[*v] == 0).rev().collect();
    while let Some(v) = order_stack.pop() {
        if placed[v] {
            continue;
        }
        placed[v] = true;
        rows[node_layer[v]].push(v);
        for &w in down[v].iter().rev() {
            if !placed[w] {
                order_stack.push(w);
            }
        }
    }
    for v in 0..total {
        if !placed[v] {
            rows[node_layer[v]].push(v);
        }
    }

    // 4. Crossing reduction: barycentric sweeps.
    let mut position = vec![0.0f32; total];
    let refresh = |rows: &Vec<Vec<usize>>, position: &mut Vec<f32>| {
        for row in rows {
            for (i, v) in row.iter().enumerate() {
                position[*v] = i as f32;
            }
        }
    };
    refresh(&rows, &mut position);
    for sweep in 0..8 {
        let downwards = sweep % 2 == 0;
        let range: Vec<usize> = if downwards {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for l in range {
            let nbrs = if downwards { &up } else { &down };
            let mut keyed: Vec<(f32, f32, usize)> = rows[l]
                .iter()
                .map(|&v| {
                    let ns = &nbrs[v];
                    let bary = if ns.is_empty() {
                        position[v]
                    } else {
                        ns.iter().map(|w| position[*w]).sum::<f32>() / ns.len() as f32
                    };
                    (bary, position[v], v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            rows[l] = keyed.into_iter().map(|(_, _, v)| v).collect();
            for (i, v) in rows[l].iter().enumerate() {
                position[*v] = i as f32;
            }
        }
    }

    // 5. Coordinates: start packed, then pull towards neighbours under the minimum gap.
    let gap = opts.node_gap;
    let mut x = vec![0.0f32; total];
    for row in &rows {
        for (i, v) in row.iter().enumerate() {
            x[*v] = i as f32 * gap;
        }
    }
    for iteration in 0..12 {
        let downwards = iteration % 2 == 0;
        let order: Vec<usize> = if downwards {
            (0..layers).collect()
        } else {
            (0..layers).rev().collect()
        };
        for l in order {
            let row = &rows[l];
            let desired: Vec<f32> = row
                .iter()
                .map(|&v| {
                    let ns: Vec<usize> = up[v].iter().chain(down[v].iter()).copied().collect();
                    if ns.is_empty() {
                        x[v]
                    } else {
                        ns.iter().map(|w| x[*w]).sum::<f32>() / ns.len() as f32
                    }
                })
                .collect();
            // Left-to-right: as close to desired as the gap allows; then right-to-left to balance.
            let mut xs = desired.clone();
            for i in 1..xs.len() {
                xs[i] = xs[i].max(xs[i - 1] + gap);
            }
            for i in (0..xs.len().saturating_sub(1)).rev() {
                xs[i] = xs[i].min(xs[i + 1] - gap).max(desired[i].min(xs[i]));
            }
            // Re-impose the gap after balancing (the right-to-left pass may have broken it).
            for i in 1..xs.len() {
                xs[i] = xs[i].max(xs[i - 1] + gap);
            }
            for (v, nx) in row.iter().zip(xs) {
                x[*v] = nx;
            }
        }
    }
    let min_x = (0..m).map(|v| x[v]).fold(f32::INFINITY, f32::min);
    let min_x = if min_x.is_finite() { min_x } else { 0.0 };
    (0..m)
        .map(|v| (comp[v], x[v] - min_x, node_layer[v]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A binary-ish tree of depth 4 plus a few extra leaves.
    fn tree() -> LayoutGraph {
        let mut edges = Vec::new();
        for parent in 0..15u32 {
            edges.push([parent, parent * 2 + 1]);
            edges.push([parent, parent * 2 + 2]);
        }
        LayoutGraph::new(31, edges)
    }

    fn y(p: Vec2) -> f32 {
        p[1]
    }

    #[test]
    fn scenario_graph_layouts_hierarchical_layout_of_a_tree() {
        let g = tree();
        let pos = layout(&g, &HierarchicalOptions::default());
        for [a, b] in g.edges() {
            assert!(
                y(pos[*a as usize]) < y(pos[*b as usize]),
                "edge {a}->{b} points down"
            );
        }
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                let d = ((pos[i][0] - pos[j][0]).powi(2) + (pos[i][1] - pos[j][1]).powi(2)).sqrt();
                assert!(d >= 39.0, "{i} and {j} overlap ({d})");
            }
        }
        // The root is centred over its subtree.
        let xs: Vec<f32> = pos.iter().map(|p| p[0]).collect();
        let (lo, hi) = (
            xs.iter().copied().fold(f32::INFINITY, f32::min),
            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        );
        assert!(((pos[0][0] - (lo + hi) / 2.0).abs()) < (hi - lo) * 0.2);
    }

    #[test]
    fn cycles_and_long_edges_are_handled() {
        // 0→1→2→0 cycle, plus a long edge 0→4 over 3.
        let g = LayoutGraph::new(5, [[0, 1], [1, 2], [2, 0], [2, 3], [3, 4], [0, 4]]);
        let pos = layout(&g, &HierarchicalOptions::default());
        assert!(pos.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        // All but the reversed back edge point down.
        let down = g
            .edges()
            .iter()
            .filter(|[a, b]| pos[*a as usize][1] < pos[*b as usize][1])
            .count();
        assert_eq!(down, 5);
    }

    #[test]
    fn left_right_swaps_axes() {
        let g = tree();
        let opts = HierarchicalOptions {
            direction: Direction::LeftRight,
            ..Default::default()
        };
        let pos = layout(&g, &opts);
        for [a, b] in g.edges() {
            assert!(pos[*a as usize][0] < pos[*b as usize][0]);
        }
    }
}
