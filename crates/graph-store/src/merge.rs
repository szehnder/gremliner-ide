//! Merging query results into the store: dedupe by external id, placeholders for edge endpoints that were
//! not returned, and stable placement of new nodes (existing nodes never move; new ones start next to the
//! neighbours that brought them in, on rings that face away from the rest of the graph).

use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::TAU;

use crate::view::Vec2;
use crate::{EdgeIx, ExtId, GraphStore, NodeIx, Props, flags};

/// A vertex to merge.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeIn {
    pub id: ExtId,
    pub label: String,
    pub caption: String,
    pub props: Props,
}

/// An edge to merge; endpoints are referenced by external id.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeIn {
    pub id: ExtId,
    pub label: String,
    pub out_v: ExtId,
    pub out_label: String,
    pub in_v: ExtId,
    pub in_label: String,
    pub props: Props,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphDelta {
    pub nodes: Vec<NodeIn>,
    pub edges: Vec<EdgeIn>,
}

impl GraphDelta {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }
}

/// What a merge changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergeReport {
    pub added_nodes: Vec<NodeIx>,
    pub added_edges: Vec<EdgeIx>,
    /// Placeholder nodes created for edge endpoints that were not in the delta; fetch them. (Vertices
    /// that arrived without properties are placeholders too, but not listed here.)
    pub missing_endpoints: Vec<NodeIx>,
}

/// Distance between siblings on a ring, in world units (nodes have radius 6).
const SPACING: f32 = 28.0;
/// Radius of the first ring around a parent.
const RING: f32 = 90.0;
/// Gap between rings.
const RING_GAP: f32 = 42.0;

impl GraphStore {
    /// Merges `delta`. Existing elements keep their positions; a vertex seen again updates its caption
    /// and properties when it brings any (or when it was a placeholder).
    pub fn merge(&mut self, delta: GraphDelta) -> MergeReport {
        let mut report = MergeReport::default();
        let unplaced = [f32::NAN, f32::NAN];
        for node in delta.nodes {
            match self.node_by_ext(&node.id) {
                Some(n) => {
                    let i = n.0 as usize;
                    let placeholder = self.node_flags[i] & flags::PLACEHOLDER != 0;
                    if placeholder || !node.props.is_empty() {
                        if !node.caption.is_empty() {
                            self.node_caption[i] = node.caption.as_str().into();
                        }
                        self.node_props[i] = node.props;
                        self.node_label[i] = self.vertex_labels.intern(&node.label);
                        self.node_flags[i] &= !flags::PLACEHOLDER;
                        self.revision += 1;
                    }
                }
                None => {
                    let n = self.add_node(node.id, &node.label, &node.caption, unplaced);
                    if node.props.is_empty() {
                        self.node_flags[n.0 as usize] |= flags::PLACEHOLDER;
                    }
                    self.node_props[n.0 as usize] = node.props;
                    report.added_nodes.push(n);
                }
            }
        }
        for edge in delta.edges {
            if self.edge_by_ext(&edge.id).is_some() {
                continue;
            }
            let mut endpoint =
                |store: &mut GraphStore, id: ExtId, label: &str| match store.node_by_ext(&id) {
                    Some(n) => n,
                    None => {
                        let caption = id.to_string();
                        let n = store.add_node(id, label, &caption, unplaced);
                        store.node_flags[n.0 as usize] |= flags::PLACEHOLDER;
                        report.added_nodes.push(n);
                        report.missing_endpoints.push(n);
                        n
                    }
                };
            let src = endpoint(self, edge.out_v, &edge.out_label);
            let dst = endpoint(self, edge.in_v, &edge.in_label);
            let e = self.add_edge(edge.id, src, dst, &edge.label);
            self.edge_props[e.0 as usize] = edge.props;
            report.added_edges.push(e);
        }
        self.place(&report.added_nodes, &report.added_edges);
        self.recompute_visibility();
        report
    }

    /// Positions `new` nodes (currently NaN). Nodes adjacent to existing ones go on rings around them,
    /// breadth-first; the remaining components are laid out as trees around their best-connected node
    /// and packed on a spiral around the existing graph.
    fn place(&mut self, new: &[NodeIx], new_edges: &[EdgeIx]) {
        if new.is_empty() {
            return;
        }
        let is_new: HashSet<NodeIx> = new.iter().copied().collect();
        // Adjacency among the new edges (every edge touching a new node is new).
        let mut adj: HashMap<NodeIx, Vec<NodeIx>> = HashMap::new();
        for e in new_edges {
            let (s, d) = self.edge_endpoints_of(*e);
            if s == d {
                continue;
            }
            adj.entry(s).or_default().push(d);
            adj.entry(d).or_default().push(s);
        }
        for list in adj.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        let mut placed: HashSet<NodeIx> = HashSet::new();
        // Direction each placed node "grows" in (away from where it came from).
        let mut heading: HashMap<NodeIx, Vec2> = HashMap::new();

        // 1. Grow outwards from existing nodes that gained new neighbours.
        let mut roots: Vec<NodeIx> = adj
            .keys()
            .copied()
            .filter(|n| !is_new.contains(n))
            .collect();
        roots.sort_unstable();
        heading.extend(self.away_from_neighbours(&roots, &is_new));
        self.grow(roots, &adj, &is_new, &mut placed, &mut heading);

        // 2. Components with no existing node: tree around the best-connected node, packed on a spiral.
        let mut occupied: Vec<(Vec2, f32)> = Vec::new();
        let old_bounds = self.bounds_where(|i| {
            !is_new.contains(&NodeIx(i as u32)) || placed.contains(&NodeIx(i as u32))
        });
        let centre = old_bounds
            .map(|b| [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5])
            .unwrap_or([0.0, 0.0]);
        if let Some(b) = old_bounds {
            let r = ((b[2] - b[0]).powi(2) + (b[3] - b[1]).powi(2)).sqrt() * 0.5 + SPACING;
            occupied.push((centre, r));
        }
        let mut singles = Vec::new();
        let mut components: Vec<Vec<NodeIx>> = Vec::new();
        let mut seen: HashSet<NodeIx> = placed.clone();
        for &n in new {
            if seen.contains(&n) {
                continue;
            }
            // Collect the component.
            let mut comp = vec![n];
            seen.insert(n);
            let mut q = VecDeque::from([n]);
            while let Some(x) = q.pop_front() {
                for &y in adj.get(&x).into_iter().flatten() {
                    if is_new.contains(&y) && seen.insert(y) {
                        comp.push(y);
                        q.push_back(y);
                    }
                }
            }
            if comp.len() == 1 {
                singles.push(n);
            } else {
                components.push(comp);
            }
        }
        components.sort_by_key(|c| std::cmp::Reverse(c.len()));
        for comp in components {
            let root = *comp
                .iter()
                .max_by_key(|n| (adj.get(n).map_or(0, Vec::len), std::cmp::Reverse(n.0)))
                .unwrap_or(&comp[0]);
            // Lay out around the origin, then move the whole component into a free spot.
            self.positions[root.0 as usize] = [0.0, 0.0];
            placed.insert(root);
            self.grow(vec![root], &adj, &is_new, &mut placed, &mut heading);
            let radius = comp
                .iter()
                .map(|n| crate::view::len(self.positions[n.0 as usize]))
                .fold(0.0f32, f32::max)
                + SPACING;
            let at = free_spot(centre, radius, &occupied);
            for n in &comp {
                let p = &mut self.positions[n.0 as usize];
                *p = [p[0] + at[0], p[1] + at[1]];
            }
            occupied.push((at, radius));
        }
        if !singles.is_empty() {
            // A sunflower disc for unconnected nodes.
            let golden = TAU * (1.0 - 1.0 / std::f32::consts::GOLDEN_RATIO);
            let disc = SPACING * 0.62 * (singles.len() as f32).sqrt() + SPACING;
            let at = free_spot(centre, disc, &occupied);
            for (k, n) in singles.iter().enumerate() {
                let r = SPACING * 0.62 * ((k as f32) + 0.5).sqrt();
                let a = k as f32 * golden;
                self.positions[n.0 as usize] = [at[0] + r * a.cos(), at[1] + r * a.sin()];
            }
        }
        self.revision += 1;
    }

    /// Breadth-first: places the unplaced new neighbours of each node in `frontier` on rings around it.
    fn grow(
        &mut self,
        frontier: Vec<NodeIx>,
        adj: &HashMap<NodeIx, Vec<NodeIx>>,
        is_new: &HashSet<NodeIx>,
        placed: &mut HashSet<NodeIx>,
        heading: &mut HashMap<NodeIx, Vec2>,
    ) {
        let mut queue: VecDeque<NodeIx> = frontier.into();
        while let Some(p) = queue.pop_front() {
            let children: Vec<NodeIx> = adj
                .get(&p)
                .into_iter()
                .flatten()
                .copied()
                .filter(|c| is_new.contains(c) && !placed.contains(c))
                .collect();
            if children.is_empty() {
                continue;
            }
            let centre = self.positions[p.0 as usize];
            let dir = heading.get(&p).copied();
            let slots = ring_slots(children.len(), dir);
            for (c, offset) in children.into_iter().zip(slots) {
                let pos = [centre[0] + offset[0], centre[1] + offset[1]];
                self.positions[c.0 as usize] = pos;
                let l = crate::view::len(offset).max(1e-3);
                heading.insert(c, [offset[0] / l, offset[1] / l]);
                placed.insert(c);
                queue.push_back(c);
            }
        }
    }

    /// For each root, the unit vector from the centroid of its existing (non-new) neighbours towards
    /// it. One pass over the edges.
    fn away_from_neighbours(
        &self,
        roots: &[NodeIx],
        is_new: &HashSet<NodeIx>,
    ) -> HashMap<NodeIx, Vec2> {
        let mut sums: HashMap<NodeIx, (f32, f32, usize)> =
            roots.iter().map(|r| (*r, (0.0, 0.0, 0))).collect();
        for (s, d) in self.edge_src.iter().zip(&self.edge_dst) {
            if s == d || is_new.contains(s) || is_new.contains(d) {
                continue;
            }
            for (n, other) in [(s, d), (d, s)] {
                if let Some(acc) = sums.get_mut(n) {
                    let q = self.positions[other.0 as usize];
                    acc.0 += q[0];
                    acc.1 += q[1];
                    acc.2 += 1;
                }
            }
        }
        sums.into_iter()
            .filter(|(_, (_, _, k))| *k > 0)
            .filter_map(|(n, (sx, sy, k))| {
                let p = self.positions[n.0 as usize];
                let v = [p[0] - sx / k as f32, p[1] - sy / k as f32];
                let l = crate::view::len(v);
                (l > 1e-3).then(|| (n, [v[0] / l, v[1] / l]))
            })
            .collect()
    }

    fn bounds_where(&self, keep: impl Fn(usize) -> bool) -> Option<[f32; 4]> {
        let mut it = self
            .positions
            .iter()
            .enumerate()
            .filter(|(i, p)| keep(*i) && p[0].is_finite())
            .map(|(_, p)| p);
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

/// Offsets for `n` children on concentric rings: a full circle, or an arc of 216° centred on `dir`.
fn ring_slots(n: usize, dir: Option<Vec2>) -> Vec<Vec2> {
    let (span, base) = match dir {
        Some(d) => (TAU * 0.6, d[1].atan2(d[0])),
        None => (TAU, 0.0),
    };
    let mut out = Vec::with_capacity(n);
    let mut ring = 0usize;
    while out.len() < n {
        let r = RING + ring as f32 * RING_GAP;
        let capacity = ((span * r / SPACING).floor() as usize).max(1);
        let count = capacity.min(n - out.len());
        for k in 0..count {
            let a = if span >= TAU {
                base + TAU * k as f32 / count as f32 + ring as f32 * 0.37
            } else if count == 1 {
                base
            } else {
                base - span * 0.5 + span * k as f32 / (count - 1) as f32
            };
            out.push([r * a.cos(), r * a.sin()]);
        }
        ring += 1;
    }
    out
}

/// The first point on a spiral around `centre` where a circle of `radius` overlaps none of `occupied`.
fn free_spot(centre: Vec2, radius: f32, occupied: &[(Vec2, f32)]) -> Vec2 {
    if occupied.is_empty() {
        return centre;
    }
    let mut t = 0.0f32;
    loop {
        let r = t * SPACING * 0.5;
        let p = [centre[0] + r * t.cos(), centre[1] + r * t.sin()];
        let free = occupied
            .iter()
            .all(|(c, cr)| crate::view::len([p[0] - c[0], p[1] - c[1]]) >= cr + radius);
        if free || t > 10_000.0 {
            return p;
        }
        t += 0.25;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PropValue;

    fn v(id: i64, label: &str) -> NodeIn {
        NodeIn {
            id: ExtId::Int(id),
            label: label.into(),
            caption: format!("n{id}"),
            props: vec![("name".into(), PropValue::Text(format!("n{id}").into()))],
        }
    }

    fn e(id: i64, out: i64, inv: i64) -> EdgeIn {
        EdgeIn {
            id: ExtId::Int(id),
            label: "route".into(),
            out_v: ExtId::Int(out),
            out_label: "airport".into(),
            in_v: ExtId::Int(inv),
            in_label: "airport".into(),
            props: vec![],
        }
    }

    fn finite(g: &GraphStore) -> bool {
        g.positions()
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite())
    }

    #[test]
    fn scenario_graph_view_paths_become_a_graph() {
        // AUS and ten destinations, as `path()` rows would deliver them (AUS repeated in each row).
        let mut g = GraphStore::new();
        let mut delta = GraphDelta::default();
        for k in 1..=10 {
            delta.nodes.push(v(0, "airport"));
            delta.edges.push(e(100 + k, 0, k));
            delta.nodes.push(v(k, "airport"));
        }
        let report = g.merge(delta);
        assert_eq!(g.node_count(), 11, "no duplicate AUS");
        assert_eq!(g.edge_count(), 10);
        assert!(report.missing_endpoints.is_empty());
        assert!(finite(&g));
        // The hub sits in the middle with its destinations on a ring around it.
        let hub = g.position(g.node_by_ext(&ExtId::Int(0)).unwrap());
        for k in 1..=10 {
            let p = g.position(g.node_by_ext(&ExtId::Int(k)).unwrap());
            let d = crate::view::len([p[0] - hub[0], p[1] - hub[1]]);
            assert!((d - RING).abs() < 1.0, "destination {k} at distance {d}");
        }
    }

    #[test]
    fn scenario_graph_view_accumulate_results() {
        let mut g = GraphStore::new();
        g.merge(GraphDelta {
            nodes: vec![v(1, "person"), v(2, "person")],
            edges: vec![e(10, 1, 2)],
        });
        let before: Vec<Vec2> = g.positions().to_vec();
        let report = g.merge(GraphDelta {
            nodes: vec![v(2, "person"), v(3, "software")],
            edges: vec![e(11, 2, 3)],
        });
        assert_eq!(g.node_count(), 3, "union of both results");
        assert_eq!(report.added_nodes.len(), 1);
        assert_eq!(
            &g.positions()[..2],
            &before[..],
            "earlier nodes keep their positions"
        );
        let p2 = g.position(NodeIx(1));
        let p3 = g.position(NodeIx(2));
        let d = crate::view::len([p3[0] - p2[0], p3[1] - p2[1]]);
        assert!(
            (d - RING).abs() < 1.0,
            "new node placed next to its neighbour"
        );
    }

    #[test]
    fn missing_endpoints_become_placeholders_then_fill_in() {
        let mut g = GraphStore::new();
        let report = g.merge(GraphDelta {
            nodes: vec![],
            edges: vec![e(10, 1, 2), e(11, 1, 3)],
        });
        assert_eq!(report.missing_endpoints.len(), 3);
        assert!(g.has_flag(report.missing_endpoints[0], flags::PLACEHOLDER));
        assert!(finite(&g));
        let pos = g.positions().to_vec();
        let report = g.merge(GraphDelta {
            nodes: vec![v(1, "person"), v(2, "software"), v(3, "software")],
            edges: vec![],
        });
        assert!(report.added_nodes.is_empty());
        let n1 = g.node_by_ext(&ExtId::Int(1)).unwrap();
        assert!(!g.has_flag(n1, flags::PLACEHOLDER));
        assert_eq!(g.caption(n1).as_ref(), "n1");
        assert_eq!(g.vertex_label_name(g.node_label(NodeIx(1))), "software");
        assert_eq!(g.positions(), &pos[..], "filling in does not move nodes");
    }

    #[test]
    fn disconnected_nodes_do_not_overlap_existing_ones() {
        let mut g = GraphStore::new();
        g.merge(GraphDelta {
            nodes: (0..30).map(|i| v(i, "a")).collect(),
            edges: (1..30).map(|i| e(100 + i, 0, i)).collect(),
        });
        g.merge(GraphDelta {
            nodes: (50..80).map(|i| v(i, "b")).collect(),
            edges: vec![],
        });
        assert!(finite(&g));
        let pos = g.positions();
        for i in 30..60 {
            for j in 0..30 {
                let d = crate::view::len([pos[i][0] - pos[j][0], pos[i][1] - pos[j][1]]);
                assert!(d > 8.0, "new node {i} overlaps existing node {j} ({d})");
            }
        }
    }

    #[test]
    fn hundreds_of_children_use_several_rings() {
        let slots = ring_slots(100, None);
        assert_eq!(slots.len(), 100);
        let max_r = slots
            .iter()
            .map(|s| crate::view::len(*s))
            .fold(0.0, f32::max);
        assert!(max_r < 260.0, "compact rings, outermost at {max_r}");
        for (i, a) in slots.iter().enumerate() {
            for b in &slots[i + 1..] {
                assert!(crate::view::len([a[0] - b[0], a[1] - b[1]]) > SPACING * 0.5);
            }
        }
    }
}

#[cfg(test)]
mod perf {
    use super::*;
    use crate::synth::Rng;

    /// Streaming `g.E()` on an air-routes-sized graph: 57k edges among 3.7k vertices, in 64-item batches.
    #[test]
    fn streaming_merge_of_air_routes_sized_result_is_fast() {
        let mut rng = Rng::new(7);
        let mut g = GraphStore::new();
        let started = std::time::Instant::now();
        let mut id = 0i64;
        for _ in 0..(57_000 / 64) {
            let edges = (0..64)
                .map(|_| {
                    id += 1;
                    EdgeIn {
                        id: ExtId::Int(1_000_000 + id),
                        label: "route".into(),
                        out_v: ExtId::Int(rng.below(3_700) as i64),
                        out_label: "airport".into(),
                        in_v: ExtId::Int(rng.below(3_700) as i64),
                        in_label: "airport".into(),
                        props: vec![],
                    }
                })
                .collect();
            g.merge(GraphDelta {
                nodes: vec![],
                edges,
            });
        }
        let elapsed = started.elapsed();
        assert_eq!(g.edge_count(), 57_000 / 64 * 64);
        assert!(g.positions().iter().all(|p| p[0].is_finite()));
        // Generous for debug builds; release is far faster.
        assert!(elapsed.as_secs_f32() < 10.0, "took {elapsed:?}");
        eprintln!("merged 57k edges in {elapsed:?}");
    }
}
