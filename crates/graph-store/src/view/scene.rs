//! Renderer-agnostic scene building: culls to the viewport, applies level-of-detail rules, and emits
//! node discs, edge triangles and label anchors (all in screen space) into a [`SceneSink`].

use super::{Camera, Rect, Vec2, rects_intersect};
use crate::{GraphStore, LabelIx, NodeIx, flags};

/// Level-of-detail tier for the current zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    /// Labels, arrowheads, full-width edges.
    Full,
    /// No labels; arrowheads only on long edges.
    Medium,
    /// Hairline edges, no arrows, no labels.
    Far,
}

#[derive(Clone, Copy, Debug)]
pub struct SceneParams {
    /// Node radius in world units.
    pub node_radius: f32,
    /// Minimum on-screen radius so nodes never vanish when zoomed out.
    pub min_node_px: f32,
    /// Labels are drawn only when their font size would be at least this many pixels.
    pub min_label_px: f32,
    /// Upper bound on labels per frame (closest to the viewport centre win).
    pub max_labels: usize,
    /// Node screen radius below which the view switches to [`Lod::Far`].
    pub far_below_px: f32,
    /// Node screen radius below which arrowheads are hidden.
    pub arrows_above_px: f32,
    /// Which elements to emit.
    pub subset: Subset,
}

/// Element subset emitted by [`build_scene`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Subset {
    /// Everything visible.
    #[default]
    All,
    /// Only selected nodes, the `extra` node (e.g. hovered), and edges touching a selected node; used to
    /// draw crisp vector highlights over a rasterised view.
    Highlighted { extra: Option<NodeIx> },
    /// All nodes (and labels) but only highlighted edges; used when only the edges are rasterised.
    NodesAndHighlightedEdges,
}

impl Default for SceneParams {
    fn default() -> Self {
        Self {
            node_radius: 6.0,
            min_node_px: 1.25,
            min_label_px: 9.0,
            max_labels: 400,
            far_below_px: 2.0,
            arrows_above_px: 4.0,
            subset: Subset::All,
        }
    }
}

/// Receives primitives in draw order: all edges first, then nodes, then labels.
///
/// Edges arrive as [`SceneSink::edge_segment`] and [`SceneSink::arrow`]; the default implementations
/// tessellate them into [`SceneSink::edge_triangle`]s, so a sink may implement either level.
pub trait SceneSink {
    /// One edge triangle in screen space. `label` is the edge label (style bucket); `highlight` is true
    /// when either endpoint is selected.
    fn edge_triangle(&mut self, label: LabelIx, highlight: bool, tri: [Vec2; 3]);

    /// A straight edge segment from `a` to `b` (screen space) of `width` pixels. `dir` is the unit
    /// direction `a → b` and `len` its length.
    fn edge_segment(
        &mut self,
        label: LabelIx,
        highlight: bool,
        a: Vec2,
        b: Vec2,
        dir: Vec2,
        len: f32,
        width: f32,
    ) {
        let _ = len;
        let (nx, ny) = (-dir[1] * width * 0.5, dir[0] * width * 0.5);
        self.edge_triangle(
            label,
            highlight,
            [
                [a[0] + nx, a[1] + ny],
                [a[0] - nx, a[1] - ny],
                [b[0] + nx, b[1] + ny],
            ],
        );
        self.edge_triangle(
            label,
            highlight,
            [
                [b[0] + nx, b[1] + ny],
                [a[0] - nx, a[1] - ny],
                [b[0] - nx, b[1] - ny],
            ],
        );
    }

    /// An arrowhead with its tip at `tip`, pointing along unit `dir`, `len` long and `half_width` wide.
    fn arrow(
        &mut self,
        label: LabelIx,
        highlight: bool,
        tip: Vec2,
        dir: Vec2,
        len: f32,
        half_width: f32,
    ) {
        let base = [tip[0] - dir[0] * len, tip[1] - dir[1] * len];
        let (wx, wy) = (-dir[1] * half_width, dir[0] * half_width);
        self.edge_triangle(
            label,
            highlight,
            [
                tip,
                [base[0] + wx, base[1] + wy],
                [base[0] - wx, base[1] - wy],
            ],
        );
    }
    /// A node disc centred at `center` (screen) with radius `radius` pixels.
    fn node(&mut self, node: NodeIx, label: LabelIx, center: Vec2, radius: f32, node_flags: u8);
    /// A label for `node`, anchored below the disc at `anchor`, with the given font size.
    fn label(&mut self, node: NodeIx, anchor: Vec2, font_px: f32);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStats {
    pub visible_nodes: usize,
    pub visible_edges: usize,
    pub labels: usize,
    pub triangles: usize,
}

impl Lod {
    pub fn for_radius(radius_px: f32, p: &SceneParams) -> Lod {
        if radius_px < p.far_below_px {
            Lod::Far
        } else if radius_px * 0.9 < p.min_label_px {
            Lod::Medium
        } else {
            Lod::Full
        }
    }
}

/// Edge width in screen pixels for a tier.
pub fn edge_width_px(lod: Lod, radius_px: f32) -> f32 {
    match lod {
        Lod::Far => 0.75,
        Lod::Medium => 1.0,
        Lod::Full => (radius_px * 0.12).clamp(1.0, 2.5),
    }
}

/// Builds the scene for `store` seen through `camera` in a viewport of `viewport` logical pixels.
pub fn build_scene(
    store: &GraphStore,
    camera: &Camera,
    viewport: Vec2,
    params: &SceneParams,
    sink: &mut impl SceneSink,
) -> SceneStats {
    let mut stats = SceneStats::default();
    let radius_px = (params.node_radius * camera.zoom).max(params.min_node_px);
    let lod = Lod::for_radius(params.node_radius * camera.zoom, params);
    let pad = radius_px + 2.0;
    let screen: Rect = [-pad, -pad, viewport[0] + pad, viewport[1] + pad];
    let positions = store.positions();
    let node_flags = store.node_flags();
    let width = edge_width_px(lod, radius_px);
    let arrows = lod != Lod::Far && radius_px >= params.arrows_above_px;
    let arrow_len = (radius_px * 0.9).clamp(4.0, 14.0);
    let arrow_half_w = arrow_len * 0.45;

    // Edges.
    let (srcs, dsts) = store.edge_endpoints();
    let edge_labels = store.edge_labels();
    let edge_flags = store.edge_flags();
    for (i, (s, d)) in srcs.iter().zip(dsts).enumerate() {
        let (si, di) = (s.0 as usize, d.0 as usize);
        let (fs, fd) = (node_flags[si], node_flags[di]);
        if (fs | fd | edge_flags[i]) & flags::HIDDEN != 0 {
            continue;
        }
        let a = camera.world_to_screen(positions[si], viewport);
        let b = camera.world_to_screen(positions[di], viewport);
        let bbox = [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]),
            a[1].max(b[1]),
        ];
        if !rects_intersect(bbox, screen) {
            continue;
        }
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let l = (dx * dx + dy * dy).sqrt();
        if l < 0.5 {
            continue; // sub-pixel edge, invisible
        }
        let (ux, uy) = (dx / l, dy / l);
        let highlight = (fs | fd) & flags::SELECTED != 0;
        if !highlight && params.subset != Subset::All {
            continue;
        }
        let label = edge_labels[i];
        stats.visible_edges += 1;

        let draw_arrow = arrows && l > radius_px * 2.0 + arrow_len;
        // Shorten the segment to end at the target's border (and the arrow's base when drawn).
        let end_inset = if draw_arrow {
            radius_px + arrow_len
        } else {
            0.0
        };
        let e = [b[0] - ux * end_inset, b[1] - uy * end_inset];
        sink.edge_segment(label, highlight, a, e, [ux, uy], l - end_inset, width);
        stats.triangles += 2;
        if draw_arrow {
            let tip = [b[0] - ux * radius_px, b[1] - uy * radius_px];
            sink.arrow(label, highlight, tip, [ux, uy], arrow_len, arrow_half_w);
            stats.triangles += 1;
        }
    }

    // Nodes (index order = draw order, so later nodes are on top; hit-testing agrees).
    let labels = store.node_labels();
    let want_labels = lod == Lod::Full;
    let mut label_candidates: Vec<(f32, NodeIx, Vec2)> = Vec::new();
    // Quantised to half pixels so the renderer's shaped-text cache hits while zooming.
    let font_px = ((radius_px * 0.9).clamp(params.min_label_px, 14.0) * 2.0).round() / 2.0;
    let centre = [viewport[0] * 0.5, viewport[1] * 0.5];
    for (i, p) in positions.iter().enumerate() {
        let f = node_flags[i];
        if f & flags::HIDDEN != 0 {
            continue;
        }
        if let Subset::Highlighted { extra } = params.subset
            && f & flags::SELECTED == 0
            && extra != Some(NodeIx(i as u32))
        {
            continue;
        }
        let c = camera.world_to_screen(*p, viewport);
        if c[0] < screen[0] || c[0] > screen[2] || c[1] < screen[1] || c[1] > screen[3] {
            continue;
        }
        stats.visible_nodes += 1;
        sink.node(NodeIx(i as u32), labels[i], c, radius_px, f);
        if want_labels {
            let d2 = (c[0] - centre[0]).powi(2) + (c[1] - centre[1]).powi(2);
            label_candidates.push((d2, NodeIx(i as u32), [c[0], c[1] + radius_px + 2.0]));
        }
    }

    if want_labels {
        if label_candidates.len() > params.max_labels {
            label_candidates.select_nth_unstable_by(params.max_labels, |a, b| a.0.total_cmp(&b.0));
            label_candidates.truncate(params.max_labels);
        }
        for (_, n, anchor) in label_candidates {
            sink.label(n, anchor, font_px);
            stats.labels += 1;
        }
    }
    stats
}

/// A sink that only counts; used by the headless benchmark and tests.
#[derive(Default)]
pub struct CountingSink {
    pub triangles: usize,
    pub nodes: usize,
    pub labels: usize,
    /// Running checksum so the optimiser cannot skip work in benchmarks.
    pub checksum: f32,
}

impl SceneSink for CountingSink {
    fn edge_triangle(&mut self, _: LabelIx, _: bool, tri: [Vec2; 3]) {
        self.triangles += 1;
        self.checksum += tri[0][0];
    }
    fn node(&mut self, _: NodeIx, _: LabelIx, center: Vec2, _: f32, _: u8) {
        self.nodes += 1;
        self.checksum += center[1];
    }
    fn label(&mut self, _: NodeIx, _: Vec2, _: f32) {
        self.labels += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_nodes() -> GraphStore {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "x", "a", [0.0, 0.0]);
        let b = g.add_node(2.into(), "x", "b", [100.0, 0.0]);
        g.add_edge(1.into(), a, b, "e");
        g
    }

    #[test]
    fn scenario_graph_rendering_labels_hide_when_zoomed_out() {
        let g = two_nodes();
        let p = SceneParams::default();
        let vp = [800.0, 600.0];
        let near = Camera {
            center: [50.0, 0.0],
            zoom: 3.0,
        };
        let mut s = CountingSink::default();
        assert_eq!(build_scene(&g, &near, vp, &p, &mut s).labels, 2);

        let far = Camera {
            center: [50.0, 0.0],
            zoom: 0.5,
        };
        let mut s = CountingSink::default();
        let stats = build_scene(&g, &far, vp, &p, &mut s);
        assert_eq!(
            stats.labels, 0,
            "labels below the minimum size are not drawn"
        );
        assert_eq!(stats.visible_nodes, 2);

        let mut s = CountingSink::default();
        assert_eq!(
            build_scene(&g, &near, vp, &p, &mut s).labels,
            2,
            "labels reappear when zooming back in"
        );
    }

    #[test]
    fn arrows_only_when_zoomed_in() {
        let g = two_nodes();
        let p = SceneParams::default();
        let vp = [800.0, 600.0];
        let mut s = CountingSink::default();
        build_scene(
            &g,
            &Camera {
                center: [50.0, 0.0],
                zoom: 2.0,
            },
            vp,
            &p,
            &mut s,
        );
        assert_eq!(s.triangles, 3, "segment (2) + arrowhead (1)");
        let mut s = CountingSink::default();
        build_scene(
            &g,
            &Camera {
                center: [50.0, 0.0],
                zoom: 0.2,
            },
            vp,
            &p,
            &mut s,
        );
        assert_eq!(s.triangles, 2, "no arrowhead when far");
    }

    #[test]
    fn highlighted_subset() {
        let mut g = two_nodes();
        let c = g.add_node(3.into(), "x", "c", [50.0, 50.0]);
        g.set_flag(NodeIx(0), flags::SELECTED, true);
        let p = SceneParams {
            subset: Subset::Highlighted { extra: Some(c) },
            ..SceneParams::default()
        };
        let mut s = CountingSink::default();
        let stats = build_scene(
            &g,
            &Camera {
                center: [50.0, 0.0],
                zoom: 1.0,
            },
            [800.0, 600.0],
            &p,
            &mut s,
        );
        assert_eq!(stats.visible_nodes, 2, "selected node + hovered extra");
        assert_eq!(stats.visible_edges, 1, "edge touching the selection");
    }

    #[test]
    fn culls_offscreen() {
        let g = two_nodes();
        let mut s = CountingSink::default();
        let stats = build_scene(
            &g,
            &Camera {
                center: [10_000.0, 0.0],
                zoom: 1.0,
            },
            [800.0, 600.0],
            &SceneParams::default(),
            &mut s,
        );
        assert_eq!(stats, SceneStats::default());
    }

    #[test]
    fn hidden_nodes_and_their_edges_are_skipped() {
        let mut g = two_nodes();
        g.set_flag(NodeIx(1), flags::HIDDEN, true);
        let mut s = CountingSink::default();
        let stats = build_scene(
            &g,
            &Camera {
                center: [50.0, 0.0],
                zoom: 1.0,
            },
            [800.0, 600.0],
            &SceneParams::default(),
            &mut s,
        );
        assert_eq!(stats.visible_nodes, 1);
        assert_eq!(stats.visible_edges, 0);
    }

    #[test]
    fn label_budget_is_respected() {
        let g = crate::synth::generate(5000, 0, 2, 3);
        let p = SceneParams {
            max_labels: 100,
            ..SceneParams::default()
        };
        let b = g.bounds().unwrap();
        let mut cam = Camera::fit(b, [1600.0, 1000.0], 10.0, 1.0);
        cam.zoom = 2.0;
        let mut s = CountingSink::default();
        let stats = build_scene(&g, &cam, [1600.0, 1000.0], &p, &mut s);
        assert!(stats.labels <= 100);
    }
}
