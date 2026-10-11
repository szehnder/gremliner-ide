//! Renderer boundary (ADR-0006 / ADR-0007). [`GpuiPrimitives`] draws the scene with GPUI quads and
//! batched paths; a wgpu-surface renderer could implement [`GraphRenderer`] instead without touching
//! the rest of the canvas.

use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, PaintQuad, Pixels, SharedString, TextAlign,
    TextRun, TransformationMatrix, Window, point, px, quad, size,
};
use graph_store::view::{Camera, Lod, SceneParams, SceneSink, SceneStats, Vec2, build_scene};
use graph_store::{GraphStore, LabelIx, NodeIx, flags};

use crate::theme::CanvasStyle;

pub struct FrameInput<'a> {
    pub store: &'a GraphStore,
    pub camera: &'a Camera,
    pub params: &'a SceneParams,
    pub style: &'a CanvasStyle,
    pub hovered: Option<NodeIx>,
    pub bounds: Bounds<Pixels>,
}

pub trait GraphRenderer {
    /// Paints the graph. Returns scene statistics and the labels still to draw (text painting needs
    /// `&mut App`, which callers hold separately).
    fn paint(
        &mut self,
        input: &FrameInput<'_>,
        window: &mut Window,
        cx: &App,
    ) -> (SceneStats, Vec<PendingLabel>);
}

pub struct PendingLabel {
    pub text: SharedString,
    pub anchor: Vec2,
    pub font_px: f32,
}

/// Draws with GPUI primitives: one transformed sprite per edge/arrowhead and one instanced quad per node.
///
/// Each batch (edges, nodes) is painted inside its own `paint_layer`, which gives the whole batch a
/// single draw order. Without layers GPUI inserts every primitive into its overlap tree to compute an
/// order, which costs ~1.5–2 µs per primitive in dense graphs (see docs/benchmarks/graph-canvas-spike.md).
#[derive(Default)]
pub struct GpuiPrimitives {
    node_quads: Vec<PaintQuad>,
    edges: Vec<EdgeSprite>,
}

/// One edge or arrowhead drawn as a transformed sprite.
#[derive(Clone, Copy)]
struct EdgeSprite {
    /// Maps the unit square (in device pixels, relative to the tile origin) onto the target shape.
    transform: TransformationMatrix,
    highlight: bool,
    arrow: bool,
}

struct Sink<'a> {
    input: &'a FrameInput<'a>,
    origin: Vec2,
    node_quads: &'a mut Vec<PaintQuad>,
    edges: &'a mut Vec<EdgeSprite>,
    labels: Vec<PendingLabel>,
}

/// Sprite tiles: a horizontal stripe covering the middle half of the tile height (soft top/bottom
/// edges from bilinear sampling give cheap anti-aliasing), and an arrowhead pointing +x.
const STRIPE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="0" y="4" width="16" height="8" fill="white"/></svg>"#;
const ARROW_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><polygon points="0,1 16,8 0,15" fill="white"/></svg>"#;
/// Logical size of the tile request; the tile is rasterised once and reused for every edge.
const TILE: f32 = 16.0;
/// Fraction of the stripe tile height that is opaque (rect height 8 of 16).
const STRIPE_FILL: f32 = 0.5;

/// Affine map taking the tile square at device-pixel origin `o` (side `side`) to a rectangle starting at
/// `start` along unit `dir` with length `len` and full height `height` (centred on the axis).
fn rect_transform(
    o: Vec2,
    side: f32,
    start: Vec2,
    dir: Vec2,
    len: f32,
    height: f32,
) -> TransformationMatrix {
    let normal = [-dir[1], dir[0]];
    let sx = len / side;
    let sy = height / side;
    // Columns: tile x axis -> dir * sx, tile y axis -> normal * sy. Row-major storage.
    let rs = [[dir[0] * sx, normal[0] * sy], [dir[1] * sx, normal[1] * sy]];
    // Tile point (o + u*side) must land at start - normal*height/2 + ...; solve for translation.
    let corner = [
        start[0] - normal[0] * height * 0.5,
        start[1] - normal[1] * height * 0.5,
    ];
    let mapped_o = [
        rs[0][0] * o[0] + rs[0][1] * o[1],
        rs[1][0] * o[0] + rs[1][1] * o[1],
    ];
    TransformationMatrix {
        rotation_scale: rs,
        translation: [corner[0] - mapped_o[0], corner[1] - mapped_o[1]],
    }
}

impl SceneSink for Sink<'_> {
    fn edge_triangle(&mut self, _label: LabelIx, _highlight: bool, _tri: [Vec2; 3]) {
        // Unused: edges arrive as segments/arrows and are drawn as sprites.
    }

    fn edge_segment(
        &mut self,
        _label: LabelIx,
        highlight: bool,
        a: Vec2,
        _b: Vec2,
        dir: Vec2,
        len: f32,
        width: f32,
    ) {
        // Transform is computed later in device pixels (needs the scale factor); store logical values.
        let start = [a[0] + self.origin[0], a[1] + self.origin[1]];
        self.edges.push(EdgeSprite {
            transform: TransformationMatrix {
                rotation_scale: [[dir[0], dir[1]], [len, width / STRIPE_FILL]],
                translation: start,
            },
            highlight,
            arrow: false,
        });
    }

    fn arrow(
        &mut self,
        _label: LabelIx,
        highlight: bool,
        tip: Vec2,
        dir: Vec2,
        len: f32,
        half_width: f32,
    ) {
        let base = [
            tip[0] - dir[0] * len + self.origin[0],
            tip[1] - dir[1] * len + self.origin[1],
        ];
        // The arrow polygon spans y 1..15 of 16, so scale the height up slightly.
        self.edges.push(EdgeSprite {
            transform: TransformationMatrix {
                rotation_scale: [[dir[0], dir[1]], [len, half_width * 2.0 * 16.0 / 14.0]],
                translation: base,
            },
            highlight,
            arrow: true,
        });
    }

    fn node(&mut self, node: NodeIx, label: LabelIx, center: Vec2, radius: f32, node_flags: u8) {
        let style = self.input.style;
        let fill: Hsla = style.palette[label.0 as usize % style.palette.len()];
        let (border_w, border_color) = if node_flags & flags::SELECTED != 0 {
            ((radius * 0.35).clamp(1.5, 3.0), style.selection)
        } else if self.input.hovered == Some(node) {
            ((radius * 0.25).clamp(1.0, 2.0), style.hover)
        } else if node_flags & flags::PINNED != 0 && radius > 4.0 {
            (1.0, style.pinned)
        } else {
            (0.0, style.selection)
        };
        let o = self.origin;
        let bounds = Bounds::new(
            point(px(center[0] - radius + o[0]), px(center[1] - radius + o[1])),
            gpui_kit::size(px(radius * 2.0), px(radius * 2.0)),
        );
        self.node_quads.push(quad(
            bounds,
            Corners::all(px(radius)),
            fill,
            Edges::all(px(border_w)),
            border_color,
            BorderStyle::Solid,
        ));
    }

    fn label(&mut self, node: NodeIx, anchor: Vec2, font_px: f32) {
        let text = self.input.store.caption(node);
        if text.is_empty() {
            return;
        }
        self.labels.push(PendingLabel {
            text: SharedString::from(text.to_string()),
            anchor: [anchor[0] + self.origin[0], anchor[1] + self.origin[1]],
            font_px,
        });
    }
}

impl GraphRenderer for GpuiPrimitives {
    fn paint(
        &mut self,
        input: &FrameInput<'_>,
        window: &mut Window,
        cx: &App,
    ) -> (SceneStats, Vec<PendingLabel>) {
        let viewport = [
            f32::from(input.bounds.size.width),
            f32::from(input.bounds.size.height),
        ];
        let origin = [
            f32::from(input.bounds.origin.x),
            f32::from(input.bounds.origin.y),
        ];
        let lod = Lod::for_radius(input.params.node_radius * input.camera.zoom, input.params);
        self.node_quads.clear();
        self.edges.clear();
        let mut sink = Sink {
            input,
            origin,
            node_quads: &mut self.node_quads,
            edges: &mut self.edges,
            labels: Vec::new(),
        };
        let stats = build_scene(input.store, input.camera, viewport, input.params, &mut sink);
        let labels = std::mem::take(&mut sink.labels);

        let normal = if lod == Lod::Far {
            input.style.edge_far
        } else {
            input.style.edge
        };
        let sf = window.scale_factor();
        // Request tiles at the canvas origin so the untransformed sprite bounds lie inside the content
        // mask (GPUI culls on untransformed bounds); the transform moves them into place.
        let tile_bounds = Bounds::new(input.bounds.origin, size(px(TILE), px(TILE)));
        let tile_o = [origin[0] * sf, origin[1] * sf];
        let side = TILE * sf;
        if !self.edges.is_empty() {
            window.paint_layer(input.bounds, |window| {
                for e in &self.edges {
                    // Unpack the logical values stashed by the sink.
                    let dir = [
                        e.transform.rotation_scale[0][0],
                        e.transform.rotation_scale[0][1],
                    ];
                    let (len, height) = (
                        e.transform.rotation_scale[1][0],
                        e.transform.rotation_scale[1][1],
                    );
                    let start = [
                        e.transform.translation[0] * sf,
                        e.transform.translation[1] * sf,
                    ];
                    let t = rect_transform(tile_o, side, start, dir, len * sf, height * sf);
                    let color = if e.highlight {
                        input.style.edge_highlight
                    } else {
                        normal
                    };
                    let (key, data) = if e.arrow {
                        ("graph-canvas/arrow.svg", ARROW_SVG)
                    } else {
                        ("graph-canvas/stripe.svg", STRIPE_SVG)
                    };
                    let _ = window.paint_svg(
                        tile_bounds,
                        SharedString::new_static(key),
                        Some(data),
                        t,
                        color,
                        cx,
                    );
                }
            });
        }
        if !self.node_quads.is_empty() {
            window.paint_layer(input.bounds, |window| {
                for q in self.node_quads.drain(..) {
                    window.paint_quad(q);
                }
            });
        }
        (stats, labels)
    }
}

/// Paints pending labels centred under their nodes.
pub fn paint_labels(labels: &[PendingLabel], color: Hsla, window: &mut Window, cx: &mut App) {
    if labels.is_empty() {
        return;
    }
    let font = window.text_style().font();
    for l in labels {
        let run = TextRun {
            len: l.text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(l.text.clone(), px(l.font_px), &[run], None);
        let origin = point(px(l.anchor[0]) - line.width / 2.0, px(l.anchor[1]));
        let _ = line.paint(
            origin,
            px(l.font_px * 1.2),
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}
