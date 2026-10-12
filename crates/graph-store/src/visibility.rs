//! What is shown: user-hidden elements, hidden labels and a property filter, folded into the effective
//! [`flags::HIDDEN`] bit that renderers and hit-testing read.

use std::collections::HashSet;
use std::sync::Arc;

use crate::{EdgeIx, GraphStore, LabelIx, NodeIx, PropValue, flags};

/// Comparison in a property filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
}

impl FilterOp {
    pub const ALL: [FilterOp; 7] = [
        FilterOp::Eq,
        FilterOp::Ne,
        FilterOp::Gt,
        FilterOp::Ge,
        FilterOp::Lt,
        FilterOp::Le,
        FilterOp::Contains,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            FilterOp::Eq => "=",
            FilterOp::Ne => "≠",
            FilterOp::Gt => ">",
            FilterOp::Ge => "≥",
            FilterOp::Lt => "<",
            FilterOp::Le => "≤",
            FilterOp::Contains => "contains",
        }
    }
}

/// Show only vertices with `label` (when set) whose property `key` (when not empty) satisfies `op value`.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub label: Option<String>,
    pub key: String,
    pub op: FilterOp,
    pub value: String,
}

impl Filter {
    fn matches_value(&self, v: &PropValue) -> bool {
        let number = self.value.trim().parse::<f64>().ok();
        match (v, number) {
            (PropValue::Num(a), Some(b)) => match self.op {
                FilterOp::Eq => *a == b,
                FilterOp::Ne => *a != b,
                FilterOp::Gt => *a > b,
                FilterOp::Ge => *a >= b,
                FilterOp::Lt => *a < b,
                FilterOp::Le => *a <= b,
                FilterOp::Contains => a.to_string().contains(self.value.trim()),
            },
            _ => {
                let a = v.to_string().to_lowercase();
                let b = self.value.trim().to_lowercase();
                match self.op {
                    FilterOp::Eq => a == b,
                    FilterOp::Ne => a != b,
                    FilterOp::Gt => a > b,
                    FilterOp::Ge => a >= b,
                    FilterOp::Lt => a < b,
                    FilterOp::Le => a <= b,
                    FilterOp::Contains => a.contains(&b),
                }
            }
        }
    }

    fn matches(&self, store: &GraphStore, n: NodeIx) -> bool {
        if let Some(label) = &self.label
            && store.vertex_label_name(store.node_label(n)) != label
        {
            return false;
        }
        let key = self.key.trim();
        if key.is_empty() {
            return true;
        }
        let props = store.node_props(n);
        if self.op == FilterOp::Ne {
            // "not equal" holds when no value equals (a missing property counts as not equal).
            let eq = Filter {
                op: FilterOp::Eq,
                ..self.clone()
            };
            return !props
                .iter()
                .any(|(k, v)| &**k == key && eq.matches_value(v));
        }
        props
            .iter()
            .any(|(k, v)| &**k == key && self.matches_value(v))
    }
}

#[derive(Default, Clone, Debug)]
pub(crate) struct Visibility {
    hidden_vertex_labels: HashSet<LabelIx>,
    hidden_edge_labels: HashSet<LabelIx>,
    filter: Option<Filter>,
}

impl Visibility {
    pub(crate) fn hides_vertex_label(&self, l: LabelIx) -> bool {
        self.hidden_vertex_labels.contains(&l)
    }

    pub(crate) fn hides_edge_label(&self, l: LabelIx) -> bool {
        self.hidden_edge_labels.contains(&l)
    }
}

/// A label with its element count, for the legend.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelCount {
    pub label: LabelIx,
    pub name: Arc<str>,
    pub count: usize,
    pub hidden: bool,
}

impl GraphStore {
    pub fn set_vertex_label_hidden(&mut self, l: LabelIx, hidden: bool) {
        if hidden {
            self.visibility.hidden_vertex_labels.insert(l);
        } else {
            self.visibility.hidden_vertex_labels.remove(&l);
        }
        self.recompute_visibility();
    }

    pub fn set_edge_label_hidden(&mut self, l: LabelIx, hidden: bool) {
        if hidden {
            self.visibility.hidden_edge_labels.insert(l);
        } else {
            self.visibility.hidden_edge_labels.remove(&l);
        }
        self.recompute_visibility();
    }

    pub fn is_vertex_label_hidden(&self, l: LabelIx) -> bool {
        self.visibility.hides_vertex_label(l)
    }

    pub fn is_edge_label_hidden(&self, l: LabelIx) -> bool {
        self.visibility.hides_edge_label(l)
    }

    pub fn filter(&self) -> Option<&Filter> {
        self.visibility.filter.as_ref()
    }

    pub fn set_filter(&mut self, filter: Option<Filter>) {
        self.visibility.filter = filter;
        self.recompute_visibility();
    }

    /// Hides the selected nodes (and so their edges) and clears the selection.
    pub fn hide_selected(&mut self) {
        for f in &mut self.node_flags {
            if *f & flags::SELECTED != 0 {
                *f = (*f | flags::USER_HIDDEN) & !flags::SELECTED;
            }
        }
        self.recompute_visibility();
    }

    /// Hides one edge.
    pub fn hide_edge(&mut self, e: EdgeIx) {
        self.edge_flags[e.0 as usize] |= flags::USER_HIDDEN;
        self.recompute_visibility();
    }

    /// Shows everything the user hid (label toggles and the filter stay).
    pub fn unhide_all(&mut self) {
        for f in self.node_flags.iter_mut().chain(self.edge_flags.iter_mut()) {
            *f &= !flags::USER_HIDDEN;
        }
        self.recompute_visibility();
    }

    /// Re-derives the effective `HIDDEN` bit of every node and edge.
    pub fn recompute_visibility(&mut self) {
        let mut hidden = Vec::with_capacity(self.node_count());
        for i in 0..self.node_count() {
            let n = NodeIx(i as u32);
            let f = self.node_flags[i];
            hidden.push(
                f & flags::USER_HIDDEN != 0
                    || self.visibility.hides_vertex_label(self.node_label[i])
                    || self
                        .visibility
                        .filter
                        .as_ref()
                        .is_some_and(|flt| !flt.matches(self, n)),
            );
        }
        for (f, h) in self.node_flags.iter_mut().zip(&hidden) {
            if *h {
                // Hidden nodes cannot stay selected.
                *f = (*f | flags::HIDDEN) & !flags::SELECTED;
            } else {
                *f &= !flags::HIDDEN;
            }
        }
        for i in 0..self.edge_flags.len() {
            let (s, d) = (self.edge_src[i].0 as usize, self.edge_dst[i].0 as usize);
            let h = self.edge_flags[i] & flags::USER_HIDDEN != 0
                || self.visibility.hides_edge_label(self.edge_label[i])
                || hidden[s]
                || hidden[d];
            if h {
                self.edge_flags[i] |= flags::HIDDEN;
            } else {
                self.edge_flags[i] &= !flags::HIDDEN;
            }
        }
        self.revision += 1;
    }

    pub fn visible_node_count(&self) -> usize {
        self.node_flags
            .iter()
            .filter(|f| **f & flags::HIDDEN == 0)
            .count()
    }

    pub fn visible_edge_count(&self) -> usize {
        self.edge_flags
            .iter()
            .filter(|f| **f & flags::HIDDEN == 0)
            .count()
    }

    /// Vertex labels with node counts, in first-seen order (labels with no nodes are skipped).
    pub fn vertex_label_counts(&self) -> Vec<LabelCount> {
        let mut counts = vec![0usize; self.vertex_labels.names.len()];
        for l in &self.node_label {
            counts[l.0 as usize] += 1;
        }
        counts
            .into_iter()
            .enumerate()
            .filter(|(_, c)| *c > 0)
            .map(|(i, count)| {
                let label = LabelIx(i as u16);
                LabelCount {
                    label,
                    name: self.vertex_labels.names[i].clone(),
                    count,
                    hidden: self.visibility.hides_vertex_label(label),
                }
            })
            .collect()
    }

    /// Edge labels with edge counts, in first-seen order.
    pub fn edge_label_counts(&self) -> Vec<LabelCount> {
        let mut counts = vec![0usize; self.edge_labels.names.len()];
        for l in &self.edge_label {
            counts[l.0 as usize] += 1;
        }
        counts
            .into_iter()
            .enumerate()
            .filter(|(_, c)| *c > 0)
            .map(|(i, count)| {
                let label = LabelIx(i as u16);
                LabelCount {
                    label,
                    name: self.edge_labels.names[i].clone(),
                    count,
                    hidden: self.visibility.hides_edge_label(label),
                }
            })
            .collect()
    }

    /// Nodes matching `query` by id or property value, best first: exact (case-insensitive) matches
    /// of the id, caption or a property, then prefix matches, then substring matches.
    pub fn find(&self, query: &str, limit: usize) -> Vec<NodeIx> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(u8, NodeIx)> = Vec::new();
        for i in 0..self.node_count() {
            let n = NodeIx(i as u32);
            let mut best = u8::MAX;
            let mut consider = |text: &str| {
                let t = text.to_lowercase();
                let score = if t == q {
                    0
                } else if t.starts_with(&q) {
                    1
                } else if t.contains(&q) {
                    2
                } else {
                    return;
                };
                best = best.min(score);
            };
            consider(&self.node_ext[i].to_string());
            consider(&self.node_caption[i]);
            for (_, v) in &self.node_props[i] {
                if let PropValue::Text(t) = v {
                    consider(t);
                } else {
                    consider(&v.to_string());
                }
            }
            if best != u8::MAX {
                scored.push((best, n));
            }
        }
        scored.sort_by_key(|(s, n)| (*s, self.has_flag(*n, flags::HIDDEN), n.0));
        scored.into_iter().take(limit).map(|(_, n)| n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExtId, GraphDelta, NodeIn};

    fn airports() -> GraphStore {
        let mut g = GraphStore::new();
        let nodes = [("AUS", 2.0), ("LHR", 2.0), ("DFW", 7.0), ("ORD", 8.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (code, runways))| NodeIn {
                id: ExtId::Int(i as i64),
                label: "airport".into(),
                caption: code.into(),
                props: vec![
                    ("code".into(), PropValue::Text(code.into())),
                    ("runways".into(), PropValue::Num(runways)),
                ],
            })
            .chain(std::iter::once(NodeIn {
                id: ExtId::Int(99),
                label: "country".into(),
                caption: "US".into(),
                props: vec![("code".into(), PropValue::Text("US".into()))],
            }))
            .collect();
        g.merge(GraphDelta {
            nodes,
            edges: vec![],
        });
        g
    }

    #[test]
    fn scenario_graph_view_property_filter() {
        let mut g = airports();
        g.set_filter(Some(Filter {
            label: Some("airport".into()),
            key: "runways".into(),
            op: FilterOp::Gt,
            value: "3".into(),
        }));
        let visible: Vec<&str> = (0..g.node_count())
            .map(|i| NodeIx(i as u32))
            .filter(|n| !g.has_flag(*n, flags::HIDDEN))
            .map(|n| &**g.caption(n))
            .collect();
        assert_eq!(visible, ["DFW", "ORD"]);
        g.set_filter(None);
        assert_eq!(g.visible_node_count(), 5);
    }

    #[test]
    fn text_filters_ignore_case() {
        let mut g = airports();
        g.set_filter(Some(Filter {
            label: None,
            key: "code".into(),
            op: FilterOp::Eq,
            value: "lhr".into(),
        }));
        assert_eq!(g.visible_node_count(), 1);
        g.set_filter(Some(Filter {
            label: None,
            key: "code".into(),
            op: FilterOp::Ne,
            value: "LHR".into(),
        }));
        assert_eq!(g.visible_node_count(), 4);
    }

    #[test]
    fn find_ranks_exact_matches_first() {
        let g = airports();
        let found = g.find("lhr", 5);
        assert_eq!(g.caption(found[0]).as_ref(), "LHR");
        let us = g.find("US", 5);
        assert_eq!(
            g.caption(us[0]).as_ref(),
            "US",
            "exact beats substring (AUS)"
        );
        assert_eq!(us.len(), 2);
        assert!(g.find("  ", 5).is_empty());
    }

    #[test]
    fn hidden_label_hides_new_nodes_and_edges() {
        let mut g = airports();
        let airport = g.vertex_label_ix("airport").unwrap_or(LabelIx(0));
        g.set_vertex_label_hidden(airport, true);
        let counts = g.vertex_label_counts();
        assert_eq!(counts[0].count, 4);
        assert!(counts[0].hidden);
        let late = g.add_node(ExtId::Int(50), "airport", "SEA", [0.0, 0.0]);
        assert!(g.has_flag(late, flags::HIDDEN));
        let us = g.node_by_ext(&ExtId::Int(99)).unwrap_or(NodeIx(0));
        let e = g.add_edge(ExtId::Int(1000), late, us, "contains");
        assert!(g.edge_has_flag(e, flags::HIDDEN));
    }
}
