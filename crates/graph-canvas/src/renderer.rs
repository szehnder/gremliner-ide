//! Renderer boundary (ADR-0006 / ADR-0007). [`GpuiPrimitives`] draws the scene with GPUI quads and
//! batched paths; a wgpu-surface renderer could implement [`GraphRenderer`] instead without touching
//! the rest of the canvas.

use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, PaintQuad, Pixels, SharedString, TextAlign,
    TextRun, TransformationMatrix, Window, point, px, quad, size,
};
use graph_store::style::{ColorRef, Looks, Shape};
use graph_store::view::{Camera, Lod, SceneParams, SceneSink, SceneStats, Vec2, build_scene};
use graph_store::{EdgeIx, GraphStore, LabelIx, NodeIx, flags};

use crate::theme::CanvasStyle;

pub struct FrameInput<'a> {
    pub store: &'a GraphStore,
    pub camera: &'a Camera,
    pub params: &'a SceneParams,
    pub style: &'a CanvasStyle,
    /// Stylesheet looks per label (colours, shapes, icons).
    pub looks: &'a Looks,
    pub hovered: Option<NodeIx>,
    pub bounds: Bounds<Pixels>,
}

/// The fill for vertex label `label`: the stylesheet colour or the label's palette slot.
pub fn vertex_color(style: &CanvasStyle, looks: &Looks, label: LabelIx) -> Hsla {
    match looks.vertex(label).map(|l| l.color) {
        Some(ColorRef::Rgb(c)) => crate::theme::color_from_bytes(c.0),
        Some(ColorRef::Palette(i)) => style.palette[i as usize % style.palette.len()],
        None => style.palette[label.0 as usize % style.palette.len()],
    }
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
    sprites: Vec<NodeSprite>,
}

/// One edge or arrowhead drawn as a transformed sprite.
#[derive(Clone, Copy)]
struct EdgeSprite {
    /// Maps the unit square (in device pixels, relative to the tile origin) onto the target shape.
    transform: TransformationMatrix,
    highlight: bool,
    arrow: bool,
    /// Stylesheet colour for the edge label (`None`: theme colour).
    color: Option<Hsla>,
}

/// A node shape or icon drawn from an SVG tile scaled to the node (logical centre and radius).
struct NodeSprite {
    center: Vec2,
    radius: f32,
    color: Hsla,
    kind: SpriteKind,
}

enum SpriteKind {
    Shape(Shape),
    Icon(SharedString),
}

struct Sink<'a> {
    input: &'a FrameInput<'a>,
    origin: Vec2,
    node_quads: &'a mut Vec<PaintQuad>,
    edges: &'a mut Vec<EdgeSprite>,
    sprites: &'a mut Vec<NodeSprite>,
    labels: Vec<PendingLabel>,
    /// Icons are drawn only when nodes are at least this large on screen.
    icons: bool,
}

/// Sprite tiles: a horizontal stripe covering the middle half of the tile height (soft top/bottom
/// edges from bilinear sampling give cheap anti-aliasing), and an arrowhead pointing +x.
const STRIPE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="0" y="4" width="16" height="8" fill="white"/></svg>"#;
const ARROW_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><polygon points="0,1 16,8 0,15" fill="white"/></svg>"#;
const DIAMOND_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><polygon points="16,0 32,16 16,32 0,16" fill="white"/></svg>"#;
const HEXAGON_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><polygon points="8,2 24,2 32,16 24,30 8,30 0,16" fill="white"/></svg>"#;
/// Logical size of the tile request; the tile is rasterised once and reused for every edge.
const TILE: f32 = 16.0;
/// Logical size of node shape and icon tiles (scaled per node by a transform, so the atlas holds one
/// tile per shape or icon rather than one per size).
const NODE_TILE: f32 = 32.0;
/// Icons need room: only from this on-screen node radius.
const ICON_MIN_RADIUS: f32 = 7.0;
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

impl Sink<'_> {
    fn edge_color(&self, label: LabelIx) -> Option<Hsla> {
        self.input
            .looks
            .edge(label)
            .and_then(|l| l.color)
            .map(|c| crate::theme::color_from_bytes(c.0))
    }
}

impl SceneSink for Sink<'_> {
    fn edge_triangle(&mut self, _label: LabelIx, _highlight: bool, _tri: [Vec2; 3]) {
        // Unused: edges arrive as segments/arrows and are drawn as sprites.
    }

    fn edge_segment(
        &mut self,
        label: LabelIx,
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
            color: self.edge_color(label),
        });
    }

    fn arrow(
        &mut self,
        label: LabelIx,
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
            color: self.edge_color(label),
        });
    }

    fn node(&mut self, node: NodeIx, label: LabelIx, center: Vec2, radius: f32, node_flags: u8) {
        let style = self.input.style;
        let mut fill: Hsla = vertex_color(style, self.input.looks, label);
        if node_flags & flags::PLACEHOLDER != 0 {
            // Not fetched yet: drawn faded until its properties arrive.
            fill = fill.opacity(0.4);
        }
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
        let look = self.input.looks.vertex(label);
        let shape = look.map_or(Shape::Circle, |l| l.shape);
        let c = [center[0] + o[0], center[1] + o[1]];
        match shape {
            Shape::Circle | Shape::Square => {
                let bounds = Bounds::new(
                    point(px(c[0] - radius), px(c[1] - radius)),
                    gpui_kit::size(px(radius * 2.0), px(radius * 2.0)),
                );
                let corner = if shape == Shape::Circle {
                    radius
                } else {
                    (radius * 0.25).min(4.0)
                };
                self.node_quads.push(quad(
                    bounds,
                    Corners::all(px(corner)),
                    fill,
                    Edges::all(px(border_w)),
                    border_color,
                    BorderStyle::Solid,
                ));
            }
            Shape::Diamond | Shape::Hexagon => {
                if border_w > 0.0 {
                    // The border is the same shape, slightly larger, underneath.
                    self.sprites.push(NodeSprite {
                        center: c,
                        radius: radius + border_w,
                        color: border_color,
                        kind: SpriteKind::Shape(shape),
                    });
                }
                self.sprites.push(NodeSprite {
                    center: c,
                    radius,
                    color: fill,
                    kind: SpriteKind::Shape(shape),
                });
            }
        }
        if self.icons
            && radius >= ICON_MIN_RADIUS
            && let Some(icon) = look.and_then(|l| l.icon.clone())
        {
            // Light icons on dark fills and vice versa.
            let on = if fill.l > 0.6 {
                style.icon_on_light
            } else {
                style.icon_on_dark
            };
            self.sprites.push(NodeSprite {
                center: c,
                radius: radius * 0.6,
                color: on,
                kind: SpriteKind::Icon(SharedString::from(format!("icons/{icon}.svg"))),
            });
        }
    }

    fn edge_label(&mut self, edge: EdgeIx, anchor: Vec2, font_px: f32) {
        let text = self.input.store.edge_caption(edge);
        if text.is_empty() {
            return;
        }
        self.labels.push(PendingLabel {
            text: SharedString::from(text.to_string()),
            anchor: [
                anchor[0] + self.origin[0],
                anchor[1] + self.origin[1] - font_px * 0.6,
            ],
            font_px,
        });
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
        self.sprites.clear();
        let mut sink = Sink {
            input,
            origin,
            node_quads: &mut self.node_quads,
            edges: &mut self.edges,
            sprites: &mut self.sprites,
            labels: Vec::new(),
            icons: lod == Lod::Full,
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
                    } else if let Some(c) = e.color {
                        // Stylesheet colours keep the theme's edge translucency.
                        c.opacity(if lod == Lod::Far { 0.35 } else { 0.75 })
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
        if !self.node_quads.is_empty() || !self.sprites.is_empty() {
            let node_tile = Bounds::new(input.bounds.origin, size(px(NODE_TILE), px(NODE_TILE)));
            let node_side = NODE_TILE * sf;
            window.paint_layer(input.bounds, |window| {
                for q in self.node_quads.drain(..) {
                    window.paint_quad(q);
                }
                for s in &self.sprites {
                    // Scale the tile (at the canvas origin) onto the node's square.
                    let k = s.radius * 2.0 * sf / node_side;
                    let corner = [(s.center[0] - s.radius) * sf, (s.center[1] - s.radius) * sf];
                    let t = TransformationMatrix {
                        rotation_scale: [[k, 0.0], [0.0, k]],
                        translation: [corner[0] - k * tile_o[0], corner[1] - k * tile_o[1]],
                    };
                    let (path, data): (SharedString, Option<&[u8]>) = match &s.kind {
                        SpriteKind::Shape(Shape::Diamond) => (
                            SharedString::new_static("graph-canvas/diamond.svg"),
                            Some(DIAMOND_SVG),
                        ),
                        SpriteKind::Shape(_) => (
                            SharedString::new_static("graph-canvas/hexagon.svg"),
                            Some(HEXAGON_SVG),
                        ),
                        SpriteKind::Icon(path) => (path.clone(), None),
                    };
                    let _ = window.paint_svg(node_tile, path, data, t, s.color, cx);
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
