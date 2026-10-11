//! In-memory graph store for visualisation, in struct-of-arrays form with dense `u32` indices (ADR-0008),
//! plus the pure view geometry ([`view`]) the canvas and the headless render benchmark share.

pub mod synth;
pub mod view;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeIx(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeIx(pub u32);

/// Index into the store's label table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LabelIx(pub u16);

pub mod flags {
    pub const SELECTED: u8 = 1;
    pub const HIDDEN: u8 = 1 << 1;
    pub const PINNED: u8 = 1 << 2;
}

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
    vertex_labels: Labels,

    edge_ext: Vec<ExtId>,
    edge_lookup: HashMap<ExtId, EdgeIx>,
    edge_src: Vec<NodeIx>,
    edge_dst: Vec<NodeIx>,
    edge_label: Vec<LabelIx>,
    edge_labels: Labels,

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
        self.node_flags.push(0);
        self.node_caption.push(caption.into());
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
        self.revision += 1;
        ix
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
