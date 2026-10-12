//! Uniform-grid spatial index over node positions for O(1)-ish hit-testing and rectangle queries.

use super::{Rect, Vec2, rect_contains};
use crate::{GraphStore, NodeIx, flags};

/// Compressed (CSR) uniform grid. Rebuild when the store revision changes.
pub struct SpatialGrid {
    origin: Vec2,
    cell: f32,
    cols: usize,
    rows: usize,
    cell_start: Vec<u32>,
    items: Vec<u32>,
    revision: u64,
}

impl SpatialGrid {
    /// Builds the grid with roughly `target_per_cell` nodes per cell.
    pub fn build(store: &GraphStore, target_per_cell: f32) -> Self {
        let revision = store.revision();
        let Some(b) = store.bounds() else {
            return Self {
                origin: [0.0, 0.0],
                cell: 1.0,
                cols: 1,
                rows: 1,
                cell_start: vec![0, 0],
                items: vec![],
                revision,
            };
        };
        let n = store.node_count().max(1) as f32;
        let w = (b[2] - b[0]).max(1.0);
        let h = (b[3] - b[1]).max(1.0);
        let cells_wanted = (n / target_per_cell.max(1.0)).max(1.0);
        let cell = ((w * h) / cells_wanted).sqrt().max(1e-3);
        let cols = ((w / cell).ceil() as usize + 1).min(4096);
        let rows = ((h / cell).ceil() as usize + 1).min(4096);
        let mut grid = Self {
            origin: [b[0], b[1]],
            cell,
            cols,
            rows,
            cell_start: vec![0; cols * rows + 1],
            items: vec![],
            revision,
        };

        let positions = store.positions();
        let node_flags = store.node_flags();
        let visible = |i: usize| node_flags[i] & flags::HIDDEN == 0;
        for (i, p) in positions.iter().enumerate() {
            if visible(i) {
                let c = grid.cell_index(*p);
                grid.cell_start[c + 1] += 1;
            }
        }
        for c in 1..grid.cell_start.len() {
            grid.cell_start[c] += grid.cell_start[c - 1];
        }
        let mut fill = grid.cell_start.clone();
        grid.items = vec![0; *grid.cell_start.last().unwrap_or(&0) as usize];
        for (i, p) in positions.iter().enumerate() {
            if visible(i) {
                let c = grid.cell_index(*p);
                grid.items[fill[c] as usize] = i as u32;
                fill[c] += 1;
            }
        }
        grid
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn cell_coords(&self, p: Vec2) -> (usize, usize) {
        let cx = ((p[0] - self.origin[0]) / self.cell)
            .floor()
            .clamp(0.0, (self.cols - 1) as f32) as usize;
        let cy = ((p[1] - self.origin[1]) / self.cell)
            .floor()
            .clamp(0.0, (self.rows - 1) as f32) as usize;
        (cx, cy)
    }

    fn cell_index(&self, p: Vec2) -> usize {
        let (cx, cy) = self.cell_coords(p);
        cy * self.cols + cx
    }

    fn for_each_in(&self, r: Rect, mut f: impl FnMut(u32)) {
        let (x0, y0) = self.cell_coords([r[0], r[1]]);
        let (x1, y1) = self.cell_coords([r[2], r[3]]);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let c = cy * self.cols + cx;
                for &i in &self.items[self.cell_start[c] as usize..self.cell_start[c + 1] as usize]
                {
                    f(i);
                }
            }
        }
    }

    /// The topmost node (highest draw index; nodes are drawn in index order) whose centre is within
    /// `radius` world units of `p`.
    pub fn node_at(&self, store: &GraphStore, p: Vec2, radius: f32) -> Option<NodeIx> {
        let positions = store.positions();
        let radii = store.node_radii();
        // Nodes larger than the pick radius (stylesheet sizes) are hit anywhere on their disc.
        let reach = radius.max(store.max_node_radius());
        let mut best: Option<u32> = None;
        self.for_each_in(
            [p[0] - reach, p[1] - reach, p[0] + reach, p[1] + reach],
            |i| {
                let q = positions[i as usize];
                let d2 = (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2);
                let r = radius.max(radii[i as usize]);
                if d2 <= r * r && best.is_none_or(|b| i > b) {
                    best = Some(i);
                }
            },
        );
        best.map(NodeIx)
    }

    /// All visible nodes whose centres lie inside the world rectangle `r`.
    pub fn nodes_in_rect(&self, store: &GraphStore, r: Rect) -> Vec<NodeIx> {
        let positions = store.positions();
        let mut out = Vec::new();
        self.for_each_in(r, |i| {
            if rect_contains(r, positions[i as usize]) {
                out.push(NodeIx(i));
            }
        });
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{Camera, rect_from_points};

    #[test]
    fn scenario_graph_rendering_click_selects_the_topmost_node() {
        let mut g = GraphStore::new();
        let below = g.add_node(1.into(), "x", "", [0.0, 0.0]);
        let on_top = g.add_node(2.into(), "x", "", [0.5, 0.0]); // overlaps, drawn later
        let _far = g.add_node(3.into(), "x", "", [100.0, 100.0]);
        g.set_flag(below, flags::SELECTED, true);
        let grid = SpatialGrid::build(&g, 4.0);
        let hit = grid.node_at(&g, [0.25, 0.0], 1.0).unwrap();
        assert_eq!(hit, on_top);
        // Click semantics: the previous selection is replaced.
        g.clear_flag(flags::SELECTED);
        g.set_flag(hit, flags::SELECTED, true);
        assert_eq!(g.selected().collect::<Vec<_>>(), vec![on_top]);
    }

    #[test]
    fn scenario_graph_rendering_rectangle_selection() {
        let mut g = GraphStore::new();
        for i in 0..5 {
            g.add_node((i as i64).into(), "x", "", [i as f32 * 10.0, 5.0]);
        }
        for i in 5..20 {
            g.add_node((i as i64).into(), "x", "", [i as f32 * 10.0, 500.0]);
        }
        let grid = SpatialGrid::build(&g, 2.0);
        let cam = Camera {
            center: [100.0, 250.0],
            zoom: 1.0,
        };
        let vp = [800.0, 600.0];
        // Shift-drag a screen rectangle around the first five nodes.
        let a = cam.screen_to_world(cam.world_to_screen([-5.0, -5.0], vp), vp);
        let b = cam.screen_to_world(cam.world_to_screen([45.0, 15.0], vp), vp);
        let picked = grid.nodes_in_rect(&g, rect_from_points(a, b));
        assert_eq!(picked, (0..5).map(NodeIx).collect::<Vec<_>>());
    }

    #[test]
    fn hidden_nodes_are_not_hit() {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "x", "", [0.0, 0.0]);
        g.add_node(2.into(), "x", "", [50.0, 50.0]);
        g.set_flag(a, flags::HIDDEN, true);
        let grid = SpatialGrid::build(&g, 4.0);
        assert_eq!(grid.node_at(&g, [0.0, 0.0], 1.0), None);
    }

    #[test]
    fn many_nodes_query_matches_bruteforce() {
        let g = crate::synth::generate(5000, 0, 3, 1);
        let grid = SpatialGrid::build(&g, 8.0);
        let r = [-50.0, -50.0, 80.0, 30.0];
        let brute: Vec<NodeIx> = g
            .positions()
            .iter()
            .enumerate()
            .filter(|(_, p)| rect_contains(r, **p))
            .map(|(i, _)| NodeIx(i as u32))
            .collect();
        assert_eq!(grid.nodes_in_rect(&g, r), brute);
    }
}
