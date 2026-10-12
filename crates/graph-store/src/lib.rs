//! In-memory graph store for visualisation, in struct-of-arrays form with dense `u32` indices (ADR-0008),
//! plus the pure view geometry ([`view`]) the canvas and the headless render benchmark share.

pub mod merge;
pub mod style;
pub mod synth;
pub mod view;
pub mod visibility;

pub use merge::{EdgeIn, GraphDelta, MergeReport, NodeIn};
pub use visibility::{Filter, FilterOp};

use std::collections::HashMap;
use std::sync::Arc;

/// External (database) element id. Gremlin ids are commonly longs or strings.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExtId {
    Int(i64),
    Str(Arc<str>),
}

impl From<i64> for ExtId {
    fn from(v: i64) -> Self {
        ExtId::Int(v)
    }
}

impl From<&str> for ExtId {
    fn from(v: &str) -> Self {
        ExtId::Str(v.into())
    }
}

impl std::fmt::Display for ExtId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtId::Int(n) => write!(f, "{n}"),
            ExtId::Str(s) => f.write_str(s),
        }
    }
}

/// A property value as the view needs it: for captions, search and filters.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Bool(bool),
    Num(f64),
    Text(Arc<str>),
}

impl std::fmt::Display for PropValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropValue::Bool(b) => write!(f, "{b}"),
            PropValue::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            PropValue::Num(n) => write!(f, "{n}"),
            PropValue::Text(t) => f.write_str(t),
        }
    }
}

/// Element properties; a key may repeat (multi-properties).
pub type Props = Vec<(Arc<str>, PropValue)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeIx(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeIx(pub u32);

/// Index into the store's label table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LabelIx(pub u16);

pub mod flags {
    pub const SELECTED: u8 = 1;
    /// Effectively hidden (by the user, a hidden label or the filter); what renderers and hit-testing
    /// check. Maintained by [`GraphStore::recompute_visibility`](crate::GraphStore::recompute_visibility).
    pub const HIDDEN: u8 = 1 << 1;
    pub const PINNED: u8 = 1 << 2;
    /// Hidden explicitly by the user (Delete).
    pub const USER_HIDDEN: u8 = 1 << 3;
    /// Details not fetched yet: only the id and label are known (an edge endpoint that was not
    /// returned, or a vertex returned without properties, as in `path()` results).
    pub const PLACEHOLDER: u8 = 1 << 4;
    /// Edges only: drawn dashed (from the stylesheet).
    pub const DASHED: u8 = 1 << 5;
}

/// Node radius in world units when no stylesheet sets a size (12 px across at 100% zoom).
pub const DEFAULT_NODE_RADIUS: f32 = 6.0;

#[derive(Default, Clone)]
struct Labels {
    names: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, LabelIx>,
}

impl Labels {
    fn intern(&mut self, name: &str) -> LabelIx {
        if let Some(ix) = self.lookup.get(name) {
            return *ix;
        }
        let ix = LabelIx(u16::try_from(self.names.len()).unwrap_or(u16::MAX));
        let name: Arc<str> = name.into();
        self.names.push(name.clone());
        self.lookup.insert(name, ix);
        ix
    }
}

/// Graph data for one view. Positions are world coordinates.
#[derive(Default, Clone)]
pub struct GraphStore {
    node_ext: Vec<ExtId>,
    node_lookup: HashMap<ExtId, NodeIx>,
    positions: Vec<[f32; 2]>,
    node_label: Vec<LabelIx>,
    node_flags: Vec<u8>,
    node_caption: Vec<Arc<str>>,
    node_props: Vec<Props>,
    /// World-unit radius per node (stylesheet sizes).
    node_radius: Vec<f32>,
    max_node_radius: f32,
    vertex_labels: Labels,

    edge_ext: Vec<ExtId>,
    edge_lookup: HashMap<ExtId, EdgeIx>,
    edge_src: Vec<NodeIx>,
    edge_dst: Vec<NodeIx>,
    edge_label: Vec<LabelIx>,
    edge_flags: Vec<u8>,
    edge_props: Vec<Props>,
    /// Width multiplier per edge (stylesheet widths; 1 is the default width).
    edge_width: Vec<f32>,
    edge_caption: Vec<Arc<str>>,
    edge_labels: Labels,

    visibility: visibility::Visibility,
    revision: u64,
}

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Monotonic counter bumped on every change, so caches can detect staleness cheaply.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn node_count(&self) -> usize {
        self.node_ext.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edge_ext.len()
    }

    /// Adds a node, or returns the existing one with the same external id.
    pub fn add_node(&mut self, ext: ExtId, label: &str, caption: &str, pos: [f32; 2]) -> NodeIx {
        if let Some(ix) = self.node_lookup.get(&ext) {
            return *ix;
        }
        let ix = NodeIx(self.node_ext.len() as u32);
        self.node_lookup.insert(ext.clone(), ix);
        self.node_ext.push(ext);
        self.positions.push(pos);
        let label = self.vertex_labels.intern(label);
        self.node_label.push(label);
        let hidden = self.visibility.hides_vertex_label(label);
        self.node_flags.push(if hidden { flags::HIDDEN } else { 0 });
        self.node_caption.push(caption.into());
        self.node_props.push(Props::new());
        self.node_radius.push(DEFAULT_NODE_RADIUS);
        self.max_node_radius = self.max_node_radius.max(DEFAULT_NODE_RADIUS);
        self.revision += 1;
        ix
    }

    /// Adds an edge, or returns the existing one with the same external id.
    pub fn add_edge(&mut self, ext: ExtId, src: NodeIx, dst: NodeIx, label: &str) -> EdgeIx {
        if let Some(ix) = self.edge_lookup.get(&ext) {
            return *ix;
        }
        let ix = EdgeIx(self.edge_ext.len() as u32);
        self.edge_lookup.insert(ext.clone(), ix);
        self.edge_ext.push(ext);
        self.edge_src.push(src);
        self.edge_dst.push(dst);
        let label = self.edge_labels.intern(label);
        self.edge_label.push(label);
        self.edge_props.push(Props::new());
        self.edge_width.push(1.0);
        self.edge_caption.push(Arc::from(""));
        let hidden = self.visibility.hides_edge_label(label)
            || (self.node_flags[src.0 as usize] | self.node_flags[dst.0 as usize]) & flags::HIDDEN
                != 0;
        self.edge_flags.push(if hidden { flags::HIDDEN } else { 0 });
        self.revision += 1;
        ix
    }

    /// Removes everything (labels and visibility settings stay).
    pub fn clear(&mut self) {
        let revision = self.revision + 1;
        let visibility = std::mem::take(&mut self.visibility);
        *self = Self {
            visibility,
            revision,
            ..Self::default()
        };
    }

    pub fn edge_by_ext(&self, ext: &ExtId) -> Option<EdgeIx> {
        self.edge_lookup.get(ext).copied()
    }

    pub fn edge_ext(&self, e: EdgeIx) -> &ExtId {
        &self.edge_ext[e.0 as usize]
    }

    pub fn node_props(&self, n: NodeIx) -> &Props {
        &self.node_props[n.0 as usize]
    }

    pub fn edge_props(&self, e: EdgeIx) -> &Props {
        &self.edge_props[e.0 as usize]
    }

    /// The first value of property `key` on node `n`.
    pub fn node_prop(&self, n: NodeIx, key: &str) -> Option<&PropValue> {
        self.node_props[n.0 as usize]
            .iter()
            .find(|(k, _)| &**k == key)
            .map(|(_, v)| v)
    }

    pub fn node_label(&self, n: NodeIx) -> LabelIx {
        self.node_label[n.0 as usize]
    }

    pub fn edge_label(&self, e: EdgeIx) -> LabelIx {
        self.edge_label[e.0 as usize]
    }

    pub fn edge_label_count(&self) -> usize {
        self.edge_labels.names.len()
    }

    pub fn vertex_label_ix(&self, name: &str) -> Option<LabelIx> {
        self.vertex_labels.lookup.get(name).copied()
    }

    pub fn edge_label_ix(&self, name: &str) -> Option<LabelIx> {
        self.edge_labels.lookup.get(name).copied()
    }

    pub fn edge_flags(&self) -> &[u8] {
        &self.edge_flags
    }

    pub fn edge_has_flag(&self, e: EdgeIx, flag: u8) -> bool {
        self.edge_flags[e.0 as usize] & flag != 0
    }

    pub fn set_caption(&mut self, n: NodeIx, caption: &str) {
        if &*self.node_caption[n.0 as usize] != caption {
            self.node_caption[n.0 as usize] = caption.into();
            self.revision += 1;
        }
    }

    pub fn node_radius(&self, n: NodeIx) -> f32 {
        self.node_radius[n.0 as usize]
    }

    pub fn node_radii(&self) -> &[f32] {
        &self.node_radius
    }

    /// The largest node radius (culling and hit-testing reach).
    pub fn max_node_radius(&self) -> f32 {
        self.max_node_radius.max(DEFAULT_NODE_RADIUS)
    }

    /// Sets every node's radius at once. Length must match the node count.
    pub fn set_node_radii(&mut self, radii: &[f32]) {
        self.node_radius.copy_from_slice(radii);
        self.max_node_radius = radii.iter().copied().fold(DEFAULT_NODE_RADIUS, f32::max);
        self.revision += 1;
    }

    pub fn edge_width(&self, e: EdgeIx) -> f32 {
        self.edge_width[e.0 as usize]
    }

    pub fn edge_widths(&self) -> &[f32] {
        &self.edge_width
    }

    /// Sets every edge's width multiplier at once. Length must match the edge count.
    pub fn set_edge_widths(&mut self, widths: &[f32]) {
        self.edge_width.copy_from_slice(widths);
        self.revision += 1;
    }

    pub fn edge_caption(&self, e: EdgeIx) -> &Arc<str> {
        &self.edge_caption[e.0 as usize]
    }

    pub fn set_edge_caption(&mut self, e: EdgeIx, caption: &str) {
        if &*self.edge_caption[e.0 as usize] != caption {
            self.edge_caption[e.0 as usize] = caption.into();
            self.revision += 1;
        }
    }

    /// Sets or clears a flag on one edge (e.g. dashed).
    pub fn set_edge_flag(&mut self, e: EdgeIx, flag: u8, on: bool) {
        let f = &mut self.edge_flags[e.0 as usize];
        let before = *f;
        if on {
            *f |= flag;
        } else {
            *f &= !flag;
        }
        if *f != before {
            self.revision += 1;
        }
    }

    pub fn edge_endpoints_of(&self, e: EdgeIx) -> (NodeIx, NodeIx) {
        (self.edge_src[e.0 as usize], self.edge_dst[e.0 as usize])
    }

    pub fn node_by_ext(&self, ext: &ExtId) -> Option<NodeIx> {
        self.node_lookup.get(ext).copied()
    }

    pub fn node_ext(&self, n: NodeIx) -> &ExtId {
        &self.node_ext[n.0 as usize]
    }

    pub fn positions(&self) -> &[[f32; 2]] {
        &self.positions
    }

    pub fn position(&self, n: NodeIx) -> [f32; 2] {
        self.positions[n.0 as usize]
    }

    pub fn set_position(&mut self, n: NodeIx, pos: [f32; 2]) {
        self.positions[n.0 as usize] = pos;
        self.revision += 1;
    }

    /// Replaces all positions at once (layout frames). Length must match the node count.
    pub fn set_positions(&mut self, positions: &[[f32; 2]]) {
        self.positions.copy_from_slice(positions);
        self.revision += 1;
    }

    pub fn node_labels(&self) -> &[LabelIx] {
        &self.node_label
    }

    pub fn vertex_label_name(&self, l: LabelIx) -> &str {
        &self.vertex_labels.names[l.0 as usize]
    }

    pub fn vertex_label_count(&self) -> usize {
        self.vertex_labels.names.len()
    }

    pub fn edge_label_name(&self, l: LabelIx) -> &str {
        &self.edge_labels.names[l.0 as usize]
    }

    pub fn caption(&self, n: NodeIx) -> &Arc<str> {
        &self.node_caption[n.0 as usize]
    }

    pub fn edge_endpoints(&self) -> (&[NodeIx], &[NodeIx]) {
        (&self.edge_src, &self.edge_dst)
    }

    pub fn edge_labels(&self) -> &[LabelIx] {
        &self.edge_label
    }

    pub fn node_flags(&self) -> &[u8] {
        &self.node_flags
    }

    pub fn has_flag(&self, n: NodeIx, flag: u8) -> bool {
        self.node_flags[n.0 as usize] & flag != 0
    }

    pub fn set_flag(&mut self, n: NodeIx, flag: u8, on: bool) {
        let f = &mut self.node_flags[n.0 as usize];
        if on {
            *f |= flag;
        } else {
            *f &= !flag;
        }
        self.revision += 1;
    }

    /// Clears a flag on every node (e.g. deselect all).
    pub fn clear_flag(&mut self, flag: u8) {
        for f in &mut self.node_flags {
            *f &= !flag;
        }
        self.revision += 1;
    }

    pub fn selected_count(&self) -> usize {
        self.node_flags
            .iter()
            .filter(|f| **f & flags::SELECTED != 0)
            .count()
    }

    /// Selects every visible node.
    pub fn select_all(&mut self) {
        for f in &mut self.node_flags {
            if *f & flags::HIDDEN == 0 {
                *f |= flags::SELECTED;
            }
        }
        self.revision += 1;
    }

    /// Visible nodes adjacent (through visible edges) to any node in `nodes`, excluding `nodes`.
    pub fn neighbours(&self, nodes: &[NodeIx]) -> Vec<NodeIx> {
        let set: std::collections::HashSet<NodeIx> = nodes.iter().copied().collect();
        let mut out = std::collections::BTreeSet::new();
        for (i, (s, d)) in self.edge_src.iter().zip(&self.edge_dst).enumerate() {
            if self.edge_flags[i] & flags::HIDDEN != 0 {
                continue;
            }
            if set.contains(s) && !set.contains(d) {
                out.insert(*d);
            } else if set.contains(d) && !set.contains(s) {
                out.insert(*s);
            }
        }
        out.into_iter().collect()
    }

    /// Adds the neighbours of the selection to the selection.
    pub fn select_neighbours(&mut self) {
        let selected: Vec<NodeIx> = self.selected().collect();
        for n in self.neighbours(&selected) {
            self.node_flags[n.0 as usize] |= flags::SELECTED;
        }
        self.revision += 1;
    }

    /// Sets or clears `flag` on every selected node (pin/unpin).
    pub fn set_flag_on_selected(&mut self, flag: u8, on: bool) {
        for f in &mut self.node_flags {
            if *f & flags::SELECTED != 0 {
                if on {
                    *f |= flag;
                } else {
                    *f &= !flag;
                }
            }
        }
        self.revision += 1;
    }

    pub fn selected(&self) -> impl Iterator<Item = NodeIx> + '_ {
        self.node_flags
            .iter()
            .enumerate()
            .filter(|(_, f)| **f & flags::SELECTED != 0)
            .map(|(i, _)| NodeIx(i as u32))
    }

    /// Bounding box `[min_x, min_y, max_x, max_y]` of visible nodes.
    pub fn bounds(&self) -> Option<[f32; 4]> {
        let mut it = self
            .positions
            .iter()
            .zip(&self.node_flags)
            .filter(|(_, f)| **f & flags::HIDDEN == 0)
            .map(|(p, _)| p);
        let first = it.next()?;
        Some(it.fold([first[0], first[1], first[0], first[1]], |b, p| {
            [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interns_ids_and_dedupes() {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "person", "marko", [0.0, 0.0]);
        let b = g.add_node("v2".into(), "person", "vadas", [1.0, 0.0]);
        let a2 = g.add_node(1.into(), "person", "marko", [5.0, 5.0]);
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert_eq!(g.node_count(), 2);
        assert_eq!(
            g.position(a),
            [0.0, 0.0],
            "existing node keeps its position"
        );
        assert_eq!(g.vertex_label_count(), 1);
        let e = g.add_edge(7.into(), a, b, "knows");
        assert_eq!(g.add_edge(7.into(), a, b, "knows"), e);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn revision_bumps_on_change() {
        let mut g = GraphStore::new();
        let r0 = g.revision();
        let a = g.add_node(1.into(), "x", "", [0.0, 0.0]);
        let r1 = g.revision();
        assert!(r1 > r0);
        g.set_position(a, [1.0, 1.0]);
        assert!(g.revision() > r1);
        let r2 = g.revision();
        g.add_node(1.into(), "x", "", [0.0, 0.0]);
        assert_eq!(g.revision(), r2, "dedupe does not bump");
    }

    #[test]
    fn flags_and_bounds() {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "x", "", [-1.0, 2.0]);
        let b = g.add_node(2.into(), "x", "", [3.0, -4.0]);
        assert_eq!(g.bounds(), Some([-1.0, -4.0, 3.0, 2.0]));
        g.set_flag(b, flags::HIDDEN, true);
        assert_eq!(g.bounds(), Some([-1.0, 2.0, -1.0, 2.0]));
        g.set_flag(a, flags::SELECTED, true);
        assert_eq!(g.selected().collect::<Vec<_>>(), vec![a]);
        g.clear_flag(flags::SELECTED);
        assert_eq!(g.selected().count(), 0);
    }
}
