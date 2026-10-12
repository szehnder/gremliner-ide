//! Stylesheets: per-label vertex and edge styles plus label-density settings, serialised as JSON
//! (`schema_version` 1), and their resolution against a [`GraphStore`].
//!
//! Resolution is O(labels) for colours, shapes and icons (looked up per label by the renderer) and O(n) for
//! what varies per element: property-bound sizes and widths, captions from templates, dashes.

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{EdgeIx, GraphStore, NodeIx, PropValue, Props, flags};

pub const SCHEMA_VERSION: u32 = 1;
/// A rule under this label applies to every label without its own.
pub const WILDCARD: &str = "*";
/// Number of colours in the theme's categorical graph palette (ADR-0023).
pub const PALETTE_SIZE: u8 = 12;
/// Default node diameter in pixels at 100% zoom.
pub const DEFAULT_NODE_PX: f32 = 12.0;
/// Default edge width in pixels.
pub const DEFAULT_EDGE_PX: f32 = 1.5;
/// Property keys tried, in order, for a caption when no template is set.
pub const CAPTION_KEYS: [&str; 6] = ["name", "code", "title", "label", "username", "id"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    Circle,
    Square,
    Diamond,
    Hexagon,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Circle, Shape::Square, Shape::Diamond, Shape::Hexagon];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Circle => "Circle",
            Shape::Square => "Square",
            Shape::Diamond => "Diamond",
            Shape::Hexagon => "Hexagon",
        }
    }
}

/// An sRGB colour, serialised as `"#rrggbb"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    /// Parses `#rgb` or `#rrggbb` (the `#` is optional).
    pub fn parse(s: &str) -> Option<Rgb> {
        let h = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match h.len() {
            6 => Some(Rgb([byte(0)?, byte(2)?, byte(4)?])),
            3 => {
                let n = |i: usize| u8::from_str_radix(h.get(i..=i)?, 16).ok().map(|v| v * 17);
                Some(Rgb([n(0)?, n(1)?, n(2)?]))
            }
            _ => None,
        }
    }

    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("bad colour {s:?}")))
    }
}

/// A size (node diameter or edge width, in pixels at 100% zoom): fixed, or proportional to a numeric
/// property between `min` and `max`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Size {
    Fixed { px: f32 },
    Property { key: String, min: f32, max: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VertexStyle {
    /// `None`: the label's automatic palette colour.
    pub color: Option<Rgb>,
    pub shape: Shape,
    /// A Lucide icon name (e.g. `plane`), drawn inside the node.
    pub icon: Option<String>,
    /// Caption template such as `{code} – {city}`; `None` uses the default caption.
    pub caption: Option<String>,
    pub size: Size,
}

impl Default for VertexStyle {
    fn default() -> Self {
        Self {
            color: None,
            shape: Shape::Circle,
            icon: None,
            caption: None,
            size: Size::Fixed {
                px: DEFAULT_NODE_PX,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeStyle {
    /// `None`: the theme's edge colour.
    pub color: Option<Rgb>,
    pub width: Size,
    /// Caption template; `None` captions edges with their label (when edge captions are on).
    pub caption: Option<String>,
    pub dashed: bool,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self {
            color: None,
            width: Size::Fixed {
                px: DEFAULT_EDGE_PX,
            },
            caption: None,
            dashed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Globals {
    /// 0 (few labels) to 1 (as many as fit).
    pub label_density: f32,
    /// Captions appear once their font would be at least this many pixels.
    pub min_label_px: f32,
    /// Longer captions are cut with "…"; 0 means no truncation.
    pub max_caption_len: usize,
    pub show_edge_captions: bool,
}

impl Default for Globals {
    fn default() -> Self {
        Self {
            label_density: 0.5,
            min_label_px: 9.0,
            max_caption_len: 32,
            show_edge_captions: false,
        }
    }
}

impl Globals {
    /// Labels per frame for the density setting.
    pub fn max_labels(&self) -> usize {
        (50.0 + self.label_density.clamp(0.0, 1.0) * 950.0) as usize
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stylesheet {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vertex: IndexMap<String, VertexStyle>,
    #[serde(default)]
    pub edge: IndexMap<String, EdgeStyle>,
    #[serde(default)]
    pub globals: Globals,
}

impl Stylesheet {
    /// The rule for vertex label `label`: its own, else the `*` wildcard.
    pub fn vertex_rule(&self, label: &str) -> Option<&VertexStyle> {
        self.vertex.get(label).or_else(|| self.vertex.get(WILDCARD))
    }

    /// The rule for edge label `label`: its own, else the `*` wildcard.
    pub fn edge_rule(&self, label: &str) -> Option<&EdgeStyle> {
        self.edge.get(label).or_else(|| self.edge.get(WILDCARD))
    }

    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            vertex: IndexMap::new(),
            edge: IndexMap::new(),
            globals: Globals::default(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The rules only (what import/export must preserve): everything but id and name.
    pub fn same_rules(&self, other: &Stylesheet) -> bool {
        self.vertex == other.vertex && self.edge == other.edge && self.globals == other.globals
    }
}

// ---- captions ----------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Segment {
    Lit(String),
    Prop(String),
}

/// A caption template: literal text with `{property}` placeholders (`{id}` and `{label}` are the
/// element's id and label). An unmatched `{` is literal.
#[derive(Clone, Debug, PartialEq)]
pub struct Template(Vec<Segment>);

impl Template {
    pub fn parse(text: &str) -> Template {
        let mut out = Vec::new();
        let mut lit = String::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            match rest[open + 1..].find('}') {
                Some(close) => {
                    lit.push_str(&rest[..open]);
                    if !lit.is_empty() {
                        out.push(Segment::Lit(std::mem::take(&mut lit)));
                    }
                    out.push(Segment::Prop(
                        rest[open + 1..open + 1 + close].trim().to_owned(),
                    ));
                    rest = &rest[open + 2 + close..];
                }
                None => break,
            }
        }
        lit.push_str(rest);
        if !lit.is_empty() {
            out.push(Segment::Lit(lit));
        }
        Template(out)
    }

    /// Renders with `lookup(key)` for placeholders (missing values render empty).
    pub fn render(&self, lookup: impl Fn(&str) -> Option<String>) -> String {
        let mut s = String::new();
        for seg in &self.0 {
            match seg {
                Segment::Lit(l) => s.push_str(l),
                Segment::Prop(k) => s.push_str(&lookup(k).unwrap_or_default()),
            }
        }
        s
    }
}

/// Cuts `text` to `max` characters plus "…" (0: no limit).
pub fn truncate(text: &str, max: usize) -> String {
    if max == 0 || text.chars().count() <= max {
        return text.to_owned();
    }
    let mut s: String = text.chars().take(max).collect();
    s.push('…');
    s
}

/// The caption when no template is set: the first of [`CAPTION_KEYS`], else the id.
pub fn default_caption(props: &Props, id: &str) -> String {
    CAPTION_KEYS
        .iter()
        .find_map(|key| {
            props
                .iter()
                .find(|(k, _)| &**k == *key)
                .map(|(_, v)| v.to_string())
        })
        .unwrap_or_else(|| id.to_owned())
}

fn prop_text(props: &Props, key: &str) -> Option<String> {
    props
        .iter()
        .find(|(k, _)| &**k == key)
        .map(|(_, v)| v.to_string())
}

fn prop_num(props: &Props, key: &str) -> Option<f64> {
    props
        .iter()
        .find(|(k, _)| &**k == key)
        .and_then(|(_, v)| match v {
            PropValue::Num(n) => Some(*n),
            PropValue::Text(t) => t.trim().parse().ok(),
            PropValue::Bool(_) => None,
        })
}

// ---- colours -----------------------------------------------------------------------------------

/// Session-wide automatic colours: a label's palette slot is its name's hash, linearly probed past slots
/// other labels already took, and never changes once assigned.
#[derive(Clone, Debug, Default)]
pub struct AutoPalette {
    assigned: HashMap<String, u8>,
}

impl AutoPalette {
    pub fn index(&mut self, label: &str) -> u8 {
        if let Some(i) = self.assigned.get(label) {
            return *i;
        }
        let start = (fnv1a(label) % u64::from(PALETTE_SIZE)) as u8;
        let taken: Vec<u8> = self.assigned.values().copied().collect();
        let slot = (0..PALETTE_SIZE)
            .map(|k| (start + k) % PALETTE_SIZE)
            .find(|s| !taken.contains(s))
            .unwrap_or(start);
        self.assigned.insert(label.to_owned(), slot);
        slot
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A colour to draw with: a palette slot (themed) or an explicit colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorRef {
    Palette(u8),
    Rgb(Rgb),
}

/// What the renderer needs per vertex label.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexLook {
    pub color: ColorRef,
    pub shape: Shape,
    pub icon: Option<Arc<str>>,
}

/// What the renderer needs per edge label (`None`: theme colour).
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeLook {
    pub color: Option<Rgb>,
}

/// The per-label looks and global settings that resolution produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Looks {
    /// By vertex label index.
    pub vertex: Vec<VertexLook>,
    /// By edge label index.
    pub edge: Vec<EdgeLook>,
    pub globals: Globals,
}

impl Looks {
    pub fn vertex(&self, l: crate::LabelIx) -> Option<&VertexLook> {
        self.vertex.get(l.0 as usize)
    }

    pub fn edge(&self, l: crate::LabelIx) -> Option<&EdgeLook> {
        self.edge.get(l.0 as usize)
    }
}

/// Maps `v` in `[lo, hi]` to `[min, max]` (the midpoint when the range is empty).
fn scale(v: f64, lo: f64, hi: f64, min: f32, max: f32) -> f32 {
    if hi > lo {
        min + ((v - lo) / (hi - lo)) as f32 * (max - min)
    } else {
        (min + max) * 0.5
    }
}

/// Applies `sheet` to `store` (sizes, widths, captions, dashes) and returns the per-label looks.
pub fn resolve(sheet: &Stylesheet, store: &mut GraphStore, auto: &mut AutoPalette) -> Looks {
    let default_v = VertexStyle::default();
    let default_e = EdgeStyle::default();
    let vertex: Vec<VertexLook> = (0..store.vertex_label_count())
        .map(|i| {
            let name = store.vertex_label_name(crate::LabelIx(i as u16)).to_owned();
            let style = sheet.vertex_rule(&name).unwrap_or(&default_v);
            VertexLook {
                color: match style.color {
                    Some(c) => ColorRef::Rgb(c),
                    None => ColorRef::Palette(auto.index(&name)),
                },
                shape: style.shape,
                icon: style
                    .icon
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(Arc::from),
            }
        })
        .collect();
    let edge: Vec<EdgeLook> = (0..store.edge_label_count())
        .map(|i| {
            let name = store.edge_label_name(crate::LabelIx(i as u16));
            EdgeLook {
                color: sheet.edge_rule(name).and_then(|s| s.color),
            }
        })
        .collect();

    // Node sizes: property-bound sizes scale between the label's smallest and largest value among
    // visible nodes.
    let n = store.node_count();
    let label_style = |store: &GraphStore, node: NodeIx| {
        let name = store.vertex_label_name(store.node_label(node));
        sheet.vertex_rule(name).unwrap_or(&default_v).clone()
    };
    let mut ranges: HashMap<(crate::LabelIx, String), (f64, f64)> = HashMap::new();
    for i in 0..n {
        let node = NodeIx(i as u32);
        if store.has_flag(node, flags::HIDDEN) {
            continue;
        }
        if let Size::Property { key, .. } = &label_style(store, node).size
            && let Some(v) = prop_num(store.node_props(node), key)
        {
            let r = ranges
                .entry((store.node_label(node), key.clone()))
                .or_insert((v, v));
            *r = (r.0.min(v), r.1.max(v));
        }
    }
    let mut radii = Vec::with_capacity(n);
    for i in 0..n {
        let node = NodeIx(i as u32);
        let style = label_style(store, node);
        let px = match &style.size {
            Size::Fixed { px } => *px,
            Size::Property { key, min, max } => {
                match (
                    prop_num(store.node_props(node), key),
                    ranges.get(&(store.node_label(node), key.clone())),
                ) {
                    (Some(v), Some((lo, hi))) => scale(v, *lo, *hi, *min, *max),
                    _ => *min,
                }
            }
        };
        radii.push((px * 0.5).max(0.5));
        // Caption.
        let id = store.node_ext(node).to_string();
        let props = store.node_props(node);
        let text = match &style.caption {
            Some(t) if !t.trim().is_empty() => {
                let label = store.vertex_label_name(store.node_label(node)).to_owned();
                Template::parse(t).render(|k| match k {
                    "id" => Some(id.clone()),
                    "label" => Some(label.clone()),
                    _ => prop_text(props, k),
                })
            }
            _ => default_caption(props, &id),
        };
        let text = truncate(&text, sheet.globals.max_caption_len);
        store.set_caption(node, &text);
    }
    store.set_node_radii(&radii);

    // Edge widths, dashes and captions.
    let m = store.edge_count();
    let edge_style = |store: &GraphStore, e: EdgeIx| {
        let name = store.edge_label_name(store.edge_label(e));
        sheet.edge_rule(name).unwrap_or(&default_e).clone()
    };
    let mut wranges: HashMap<(crate::LabelIx, String), (f64, f64)> = HashMap::new();
    for i in 0..m {
        let e = EdgeIx(i as u32);
        if store.edge_has_flag(e, flags::HIDDEN) {
            continue;
        }
        if let Size::Property { key, .. } = &edge_style(store, e).width
            && let Some(v) = prop_num(store.edge_props(e), key)
        {
            let r = wranges
                .entry((store.edge_label(e), key.clone()))
                .or_insert((v, v));
            *r = (r.0.min(v), r.1.max(v));
        }
    }
    let mut widths = Vec::with_capacity(m);
    for i in 0..m {
        let e = EdgeIx(i as u32);
        let style = edge_style(store, e);
        let px = match &style.width {
            Size::Fixed { px } => *px,
            Size::Property { key, min, max } => match (
                prop_num(store.edge_props(e), key),
                wranges.get(&(store.edge_label(e), key.clone())),
            ) {
                (Some(v), Some((lo, hi))) => scale(v, *lo, *hi, *min, *max),
                _ => *min,
            },
        };
        widths.push((px / DEFAULT_EDGE_PX).max(0.1));
        store.set_edge_flag(e, flags::DASHED, style.dashed);
        let label = store.edge_label_name(store.edge_label(e)).to_owned();
        let caption = match &style.caption {
            Some(t) if !t.trim().is_empty() => {
                let id = store.edge_ext(e).to_string();
                let props = store.edge_props(e);
                Template::parse(t).render(|k| match k {
                    "id" => Some(id.clone()),
                    "label" => Some(label.clone()),
                    _ => prop_text(props, k),
                })
            }
            _ => label,
        };
        let caption = truncate(&caption, sheet.globals.max_caption_len);
        store.set_edge_caption(e, &caption);
    }
    store.set_edge_widths(&widths);

    Looks {
        vertex,
        edge,
        globals: sheet.globals.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EdgeIn, ExtId, GraphDelta, NodeIn};

    fn airports() -> GraphStore {
        let mut g = GraphStore::new();
        let a = |id: i64, code: &str, city: &str, runways: f64| NodeIn {
            id: ExtId::Int(id),
            label: "airport".into(),
            caption: code.into(),
            props: vec![
                ("code".into(), PropValue::Text(code.into())),
                ("city".into(), PropValue::Text(city.into())),
                ("runways".into(), PropValue::Num(runways)),
            ],
        };
        let route = |id: i64, from: i64, to: i64, dist: f64| EdgeIn {
            id: ExtId::Int(id),
            label: "route".into(),
            out_v: ExtId::Int(from),
            out_label: "airport".into(),
            in_v: ExtId::Int(to),
            in_label: "airport".into(),
            props: vec![("dist".into(), PropValue::Num(dist))],
        };
        g.merge(GraphDelta {
            nodes: vec![
                a(1, "AUS", "Austin", 2.0),
                a(2, "DFW", "Dallas", 7.0),
                a(3, "LHR", "London", 2.0),
                a(4, "ORD", "Chicago", 8.0),
            ],
            edges: vec![
                route(10, 1, 2, 190.0),
                route(11, 1, 3, 4900.0),
                route(12, 2, 4, 800.0),
            ],
        });
        g
    }

    fn node(g: &GraphStore, id: i64) -> NodeIx {
        g.node_by_ext(&ExtId::Int(id)).unwrap()
    }

    #[test]
    fn scenario_graph_stylesheets_round_trip() {
        let mut sheet = Stylesheet::new("s1", "Air");
        sheet.vertex.insert(
            "airport".into(),
            VertexStyle {
                color: Some(Rgb([0x12, 0xab, 0xef])),
                shape: Shape::Hexagon,
                icon: Some("plane".into()),
                caption: Some("{code} – {city}".into()),
                size: Size::Property {
                    key: "runways".into(),
                    min: 8.0,
                    max: 32.0,
                },
            },
        );
        sheet.edge.insert(
            "route".into(),
            EdgeStyle {
                color: None,
                width: Size::Property {
                    key: "dist".into(),
                    min: 1.0,
                    max: 6.0,
                },
                caption: Some("{dist} mi".into()),
                dashed: true,
            },
        );
        sheet.globals.max_caption_len = 5;
        let json = sheet.to_json();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"#12abef\""));
        let mut back = Stylesheet::from_json(&json).unwrap();
        assert_eq!(back, sheet);
        back.id = "other".into();
        back.name = "Imported".into();
        assert!(back.same_rules(&sheet));
        // Missing fields take defaults.
        let minimal = Stylesheet::from_json(
            r#"{"schema_version":1,"id":"m","name":"M","vertex":{"person":{"shape":"square"}}}"#,
        )
        .unwrap();
        assert_eq!(minimal.vertex["person"].shape, Shape::Square);
        assert_eq!(minimal.vertex["person"].size, VertexStyle::default().size);
    }

    #[test]
    fn scenario_graph_stylesheets_stable_auto_colour() {
        let mut auto = AutoPalette::default();
        let first = auto.index("airport");
        auto.index("country");
        auto.index("continent");
        assert_eq!(auto.index("airport"), first, "same label, same colour");
        // Two different results that both contain airports resolve to the same colour.
        let sheet = Stylesheet::new("d", "Default");
        let mut g1 = airports();
        let looks1 = resolve(&sheet, &mut g1, &mut auto);
        let mut g2 = GraphStore::new();
        g2.add_node(ExtId::Int(9), "country", "US", [0.0, 0.0]);
        g2.add_node(ExtId::Int(1), "airport", "AUS", [1.0, 0.0]);
        let looks2 = resolve(&sheet, &mut g2, &mut auto);
        let colour = |g: &GraphStore, looks: &Looks| {
            looks
                .vertex(g.vertex_label_ix("airport").unwrap())
                .unwrap()
                .color
        };
        assert_eq!(colour(&g1, &looks1), colour(&g2, &looks2));
        // Distinct labels get distinct slots while the palette lasts.
        let mut fresh = AutoPalette::default();
        let slots: std::collections::HashSet<u8> =
            (0..12).map(|i| fresh.index(&format!("label{i}"))).collect();
        assert_eq!(slots.len(), 12);
    }

    #[test]
    fn scenario_graph_stylesheets_caption_template() {
        let mut sheet = Stylesheet::new("s", "S");
        sheet.vertex.insert(
            "airport".into(),
            VertexStyle {
                caption: Some("{code} – {city}".into()),
                ..Default::default()
            },
        );
        let mut g = airports();
        resolve(&sheet, &mut g, &mut AutoPalette::default());
        assert_eq!(g.caption(node(&g, 1)).as_ref(), "AUS – Austin");
        let t = Template::parse("{label}#{id} {missing}{ unclosed");
        assert_eq!(
            t.render(|k| match k {
                "label" => Some("airport".into()),
                "id" => Some("3".into()),
                _ => None,
            }),
            "airport#3 { unclosed"
        );
    }

    #[test]
    fn scenario_graph_stylesheets_truncate_captions() {
        assert_eq!(truncate("Austin Bergstrom", 5), "Austi…");
        assert_eq!(truncate("Austin", 0), "Austin", "0 means no truncation");
        assert_eq!(truncate("Ünïcode", 3), "Ünï…");
        let mut sheet = Stylesheet::new("s", "S");
        sheet.globals.max_caption_len = 2;
        let mut g = airports();
        resolve(&sheet, &mut g, &mut AutoPalette::default());
        assert_eq!(g.caption(node(&g, 1)).as_ref(), "AU…");
    }

    #[test]
    fn scenario_graph_stylesheets_size_by_property() {
        let mut sheet = Stylesheet::new("s", "S");
        sheet.vertex.insert(
            "airport".into(),
            VertexStyle {
                size: Size::Property {
                    key: "runways".into(),
                    min: 8.0,
                    max: 32.0,
                },
                ..Default::default()
            },
        );
        let mut g = airports();
        resolve(&sheet, &mut g, &mut AutoPalette::default());
        let diameter = |id| g.node_radius(node(&g, id)) * 2.0;
        assert_eq!(diameter(4), 32.0, "ORD has the most runways");
        assert_eq!(diameter(1), 8.0, "AUS has the fewest");
        assert!(diameter(2) > 8.0 && diameter(2) < 32.0);
        assert_eq!(g.max_node_radius(), 16.0);
    }

    #[test]
    fn scenario_graph_stylesheets_edge_width_by_distance() {
        let mut sheet = Stylesheet::new("s", "S");
        sheet.edge.insert(
            "route".into(),
            EdgeStyle {
                width: Size::Property {
                    key: "dist".into(),
                    min: 1.0,
                    max: 6.0,
                },
                dashed: true,
                ..Default::default()
            },
        );
        let mut g = airports();
        resolve(&sheet, &mut g, &mut AutoPalette::default());
        let w = |id| g.edge_width(g.edge_by_ext(&ExtId::Int(id)).unwrap()) * DEFAULT_EDGE_PX;
        assert!(w(11) > w(12) && w(12) > w(10), "longer routes are thicker");
        assert!((w(11) - 6.0).abs() < 1e-4 && (w(10) - 1.0).abs() < 1e-4);
        assert!(g.edge_has_flag(EdgeIx(0), flags::DASHED));
    }

    #[test]
    fn colours_parse() {
        assert_eq!(Rgb::parse("#fff"), Some(Rgb([255, 255, 255])));
        assert_eq!(Rgb::parse("12AbEf"), Some(Rgb([0x12, 0xab, 0xef])));
        assert_eq!(Rgb::parse("#12"), None);
        assert_eq!(Rgb([1, 2, 3]).hex(), "#010203");
    }
}
