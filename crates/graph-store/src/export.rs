//! Image export (export): the graph as SVG (vector shapes, arrowheads, captions, current styles and
//! positions) and as PNG rendered from that SVG with resvg (ADR-0038).

use std::fmt::Write as _;

use crate::style::{ColorRef, Looks, Shape};
use crate::{EdgeIx, GraphStore, NodeIx, flags};

/// Colours for the export, resolved from the app theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportStyle {
    /// The categorical palette `ColorRef::Palette` indexes into.
    pub palette: Vec<[u8; 3]>,
    pub edge: [u8; 3],
    pub text: [u8; 3],
    /// `None`: transparent.
    pub background: Option<[u8; 3]>,
    pub font_family: String,
}

/// What to export, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Area {
    /// Every visible node, with a margin.
    Whole,
    /// The visible viewport: top-left and bottom-right corners.
    Rect { min: [f32; 2], max: [f32; 2] },
}

const MARGIN: f32 = 24.0;
const ARROW: f32 = 5.0;

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The bounds of the visible nodes (with their radii), or `None` for an empty graph.
pub fn bounds(store: &GraphStore) -> Option<([f32; 2], [f32; 2])> {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let mut any = false;
    for i in 0..store.node_count() {
        let n = NodeIx(i as u32);
        if store.has_flag(n, flags::HIDDEN) {
            continue;
        }
        let [x, y] = store.positions()[i];
        let r = store.node_radius(n);
        min = [min[0].min(x - r), min[1].min(y - r)];
        max = [max[0].max(x + r), max[1].max(y + r)];
        any = true;
    }
    any.then_some((min, max))
}

fn node_color(store: &GraphStore, looks: &Looks, style: &ExportStyle, n: NodeIx) -> [u8; 3] {
    match looks.vertex(store.node_label(n)).map(|l| &l.color) {
        Some(ColorRef::Rgb(rgb)) => rgb.0,
        Some(ColorRef::Palette(i)) => style
            .palette
            .get(*i as usize % style.palette.len().max(1))
            .copied()
            .unwrap_or(style.edge),
        None => style.palette.first().copied().unwrap_or(style.edge),
    }
}

fn shape_element(shape: Shape, x: f32, y: f32, r: f32, fill: &str) -> String {
    match shape {
        Shape::Circle => {
            format!(r#"<circle class="node" cx="{x:.2}" cy="{y:.2}" r="{r:.2}" fill="{fill}"/>"#)
        }
        Shape::Square => format!(
            r#"<rect class="node" x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="{:.2}" fill="{fill}"/>"#,
            x - r,
            y - r,
            2.0 * r,
            2.0 * r,
            r * 0.2
        ),
        Shape::Diamond => format!(
            r#"<polygon class="node" points="{x:.2},{:.2} {:.2},{y:.2} {x:.2},{:.2} {:.2},{y:.2}" fill="{fill}"/>"#,
            y - r,
            x + r,
            y + r,
            x - r
        ),
        Shape::Hexagon => {
            let h = r * 0.87;
            format!(
                r#"<polygon class="node" points="{:.2},{:.2} {:.2},{:.2} {:.2},{y:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{y:.2}" fill="{fill}"/>"#,
                x - r * 0.5,
                y - h,
                x + r * 0.5,
                y - h,
                x + r,
                x + r * 0.5,
                y + h,
                x - r * 0.5,
                y + h,
                x - r
            )
        }
    }
}

/// The graph as SVG: one `class="node"` shape per visible node, edges with arrowheads, and a `<text>`
/// per non-empty caption, in the current positions and styles.
pub fn svg(store: &GraphStore, looks: &Looks, style: &ExportStyle, area: Area) -> String {
    let (min, max) = match area {
        Area::Rect { min, max } => (min, max),
        Area::Whole => bounds(store)
            .map(|(a, b)| {
                (
                    [a[0] - MARGIN, a[1] - MARGIN * 2.0],
                    [b[0] + MARGIN, b[1] + MARGIN * 2.0],
                )
            })
            .unwrap_or(([0.0, 0.0], [100.0, 100.0])),
    };
    let (w, h) = ((max[0] - min[0]).max(1.0), (max[1] - min[1]).max(1.0));
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="{:.2} {:.2} {w:.2} {h:.2}" font-family="{}">"#,
        min[0],
        min[1],
        escape(&style.font_family)
    );
    out.push('\n');
    let edge_hex = hex(style.edge);
    // One arrowhead marker per edge colour in use.
    let mut colors: Vec<String> = vec![edge_hex.clone()];
    for e in 0..store.edge_count() {
        let c = looks
            .edge(store.edge_label(EdgeIx(e as u32)))
            .and_then(|l| l.color)
            .map(|c| hex(c.0));
        if let Some(c) = c
            && !colors.contains(&c)
        {
            colors.push(c);
        }
    }
    out.push_str("<defs>");
    for (i, c) in colors.iter().enumerate() {
        let _ = write!(
            out,
            r#"<marker id="arrow{i}" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="{ARROW}" markerHeight="{ARROW}" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="{c}"/></marker>"#
        );
    }
    out.push_str("</defs>\n");
    if let Some(bg) = style.background {
        let _ = writeln!(
            out,
            r#"<rect x="{:.2}" y="{:.2}" width="{w:.2}" height="{h:.2}" fill="{}"/>"#,
            min[0],
            min[1],
            hex(bg)
        );
    }
    let positions = store.positions();
    let (sources, targets) = store.edge_endpoints();
    out.push_str("<g class=\"edges\">\n");
    for e in 0..store.edge_count() {
        let (a, b) = (sources[e], targets[e]);
        if a == b || store.has_flag(a, flags::HIDDEN) || store.has_flag(b, flags::HIDDEN) {
            continue;
        }
        let ix = EdgeIx(e as u32);
        let color = looks
            .edge(store.edge_label(ix))
            .and_then(|l| l.color)
            .map(|c| hex(c.0))
            .unwrap_or_else(|| edge_hex.clone());
        let marker = colors.iter().position(|c| *c == color).unwrap_or(0);
        let [x1, y1] = positions[a.0 as usize];
        let [x2, y2] = positions[b.0 as usize];
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
        let (ux, uy) = (dx / len, dy / len);
        let (ra, rb) = (store.node_radius(a), store.node_radius(b));
        let dash = if store.edge_has_flag(ix, flags::DASHED) {
            r#" stroke-dasharray="4 3""#
        } else {
            ""
        };
        let _ = writeln!(
            out,
            r#"<line class="edge" x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{color}" stroke-width="{:.2}"{dash} marker-end="url(#arrow{marker})"/>"#,
            x1 + ux * ra,
            y1 + uy * ra,
            x2 - ux * rb,
            y2 - uy * rb,
            store.edge_width(ix).max(0.5)
        );
        if looks.globals.show_edge_captions {
            let caption = store.edge_caption(ix);
            if !caption.is_empty() {
                let _ = writeln!(
                    out,
                    r#"<text class="edge-caption" x="{:.2}" y="{:.2}" font-size="6" text-anchor="middle" fill="{}">{}</text>"#,
                    (x1 + x2) / 2.0,
                    (y1 + y2) / 2.0 - 2.0,
                    hex(style.text),
                    escape(caption)
                );
            }
        }
    }
    out.push_str("</g>\n<g class=\"nodes\">\n");
    let text = hex(style.text);
    for (i, &[x, y]) in positions.iter().enumerate().take(store.node_count()) {
        let n = NodeIx(i as u32);
        if store.has_flag(n, flags::HIDDEN) {
            continue;
        }
        let r = store.node_radius(n);
        let shape = looks
            .vertex(store.node_label(n))
            .map(|l| l.shape)
            .unwrap_or_default();
        let fill = hex(node_color(store, looks, style, n));
        out.push_str(&shape_element(shape, x, y, r, &fill));
        out.push('\n');
        let caption = store.caption(n);
        if !caption.is_empty() {
            let _ = writeln!(
                out,
                r#"<text class="caption" x="{x:.2}" y="{:.2}" font-size="{:.2}" text-anchor="middle" fill="{text}">{}</text>"#,
                y + r + 8.0,
                (r * 1.2).clamp(6.0, 14.0),
                escape(caption)
            );
        }
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// Render an SVG to PNG at `scale` (1, 2, 4...). Captions use the system fonts.
pub fn png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    use resvg::{tiny_skia, usvg};
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1))
        .ok_or_else(|| format!("cannot allocate a {w}×{h} image"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}
