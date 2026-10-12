//! The graph elements in a result: every distinct vertex and edge, wherever it appears (top level, paths,
//! lists, map keys and values, trees, traversers), plus `elementMap()` rows recognised as elements.

use std::collections::HashSet;

use crate::{
    Direction, Edge, GKey, GValue, IndexMap, Property, T, Vertex, VertexProperty, new_map,
};

/// Distinct vertices and edges of a result, in first-seen order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ElementSet {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

impl ElementSet {
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.edges.is_empty()
    }
}

/// Collect the elements of `items`. A vertex seen twice keeps the copy with more properties.
pub fn elements(items: &[GValue]) -> ElementSet {
    let mut c = Collector::default();
    for item in items {
        c.visit(item);
    }
    c.out
}

/// True when the result contains at least one vertex or edge (the Graph view applies).
pub fn has_graph_elements(items: &[GValue]) -> bool {
    items.iter().any(contains_element)
}

fn contains_element(v: &GValue) -> bool {
    match v {
        GValue::Vertex(_) | GValue::Edge(_) => true,
        GValue::Path(p) => p.objects.iter().any(contains_element),
        GValue::List(items) | GValue::Set(items) => items.iter().any(contains_element),
        GValue::BulkSet(items) => items.iter().any(|(v, _)| contains_element(v)),
        GValue::Map(map) => {
            element_map(map).is_some()
                || map
                    .iter()
                    .any(|(k, v)| matches!(k, GKey::Vertex { .. }) || contains_element(v))
        }
        GValue::Tree(entries) => entries
            .iter()
            .any(|(k, v)| contains_element(k) || contains_element(v)),
        GValue::Traverser { value, .. } => contains_element(value),
        _ => false,
    }
}

/// Identity key for an element id (type-tagged so `1` and `"1"` differ).
fn id_key(id: &GValue) -> String {
    format!("{}:{}", id.type_name(), crate::console::format(id))
}

#[derive(Default)]
struct Collector {
    out: ElementSet,
    vertex_ix: std::collections::HashMap<String, usize>,
    edge_seen: HashSet<String>,
}

impl Collector {
    fn vertex(&mut self, v: &Vertex) {
        match self.vertex_ix.get(&id_key(&v.id)) {
            Some(&i) => {
                if self.out.vertices[i].properties.len() < v.properties.len() {
                    self.out.vertices[i] = v.clone();
                }
            }
            None => {
                self.vertex_ix
                    .insert(id_key(&v.id), self.out.vertices.len());
                self.out.vertices.push(v.clone());
            }
        }
    }

    fn edge(&mut self, e: &Edge) {
        if self.edge_seen.insert(id_key(&e.id)) {
            self.out.edges.push(e.clone());
        }
    }

    fn visit(&mut self, v: &GValue) {
        match v {
            GValue::Vertex(v) => self.vertex(v),
            GValue::Edge(e) => self.edge(e),
            GValue::Path(p) => p.objects.iter().for_each(|o| self.visit(o)),
            GValue::List(items) | GValue::Set(items) => items.iter().for_each(|i| self.visit(i)),
            GValue::BulkSet(items) => items.iter().for_each(|(i, _)| self.visit(i)),
            GValue::Map(map) => match element_map(map) {
                Some(Element::Vertex(v)) => self.vertex(&v),
                Some(Element::Edge(e)) => self.edge(&e),
                None => {
                    for (k, v) in map {
                        if let GKey::Vertex { id, label } = k {
                            self.vertex(&Vertex {
                                id: Box::new(key_value(id)),
                                label: label.clone(),
                                properties: new_map(),
                            });
                        }
                        self.visit(v);
                    }
                }
            },
            GValue::Tree(entries) => {
                for (k, v) in entries {
                    self.visit(k);
                    self.visit(v);
                }
            }
            GValue::Traverser { value, .. } => self.visit(value),
            _ => {}
        }
    }
}

enum Element {
    Vertex(Vertex),
    Edge(Edge),
}

fn key_value(k: &GKey) -> GValue {
    match k {
        GKey::Int(n) => GValue::Int64(*n),
        GKey::String(s) | GKey::Other(s) => GValue::String(s.clone()),
        GKey::Uuid(u) => GValue::Uuid(*u),
        GKey::Bool(b) => GValue::Bool(*b),
        _ => GValue::Null,
    }
}

/// Recognise an `elementMap()` / `valueMap(true)` row: `T.id` and `T.label` keys, plus `Direction.OUT` /
/// `Direction.IN` endpoint maps for edges.
fn element_map(map: &IndexMap<GKey, GValue>) -> Option<Element> {
    let id = map.get(&GKey::T(T::Id))?;
    let label = match map.get(&GKey::T(T::Label))? {
        GValue::String(s) => s.clone(),
        _ => return None,
    };
    let endpoint = |d: Direction| -> Option<(GValue, String)> {
        let GValue::Map(m) = map.get(&GKey::Direction(d))? else {
            return None;
        };
        let id = m.get(&GKey::T(T::Id))?.clone();
        let label = m
            .get(&GKey::T(T::Label))
            .and_then(GValue::as_str)
            .unwrap_or_default()
            .to_owned();
        Some((id, label))
    };
    // valueMap(true) wraps values in lists; elementMap() does not.
    let first = |v: &GValue| match v {
        GValue::List(items) if items.len() == 1 => items[0].clone(),
        other => other.clone(),
    };
    let props = map.iter().filter_map(|(k, v)| match k {
        GKey::String(s) => Some((s.clone(), v)),
        _ => None,
    });
    if let (Some((out_v, out_v_label)), Some((in_v, in_v_label))) =
        (endpoint(Direction::Out), endpoint(Direction::In))
    {
        let mut properties = new_map();
        for (k, v) in props {
            properties.insert(
                k.clone(),
                Property {
                    key: k,
                    value: Box::new(first(v)),
                },
            );
        }
        return Some(Element::Edge(Edge {
            id: Box::new(id.clone()),
            label,
            out_v: Box::new(out_v),
            out_v_label,
            in_v: Box::new(in_v),
            in_v_label,
            properties,
        }));
    }
    let mut properties = new_map();
    for (k, v) in props {
        let values = match v {
            GValue::List(items) => items.clone(),
            other => vec![other.clone()],
        };
        properties.insert(
            k.clone(),
            values
                .into_iter()
                .map(|value| VertexProperty {
                    id: Box::new(GValue::Null),
                    label: k.clone(),
                    value: Box::new(value),
                    properties: new_map(),
                })
                .collect(),
        );
    }
    Some(Element::Vertex(Vertex {
        id: Box::new(id.clone()),
        label,
        properties,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Path;

    fn vertex(id: i64, name: Option<&str>) -> GValue {
        let mut properties = new_map();
        if let Some(name) = name {
            properties.insert(
                "name".to_owned(),
                vec![VertexProperty {
                    id: Box::new(GValue::Int64(100 + id)),
                    label: "name".into(),
                    value: Box::new(GValue::String(name.into())),
                    properties: new_map(),
                }],
            );
        }
        GValue::Vertex(Vertex {
            id: Box::new(GValue::Int64(id)),
            label: "person".into(),
            properties,
        })
    }

    fn edge(id: i64, out: i64, inv: i64) -> GValue {
        GValue::Edge(Edge {
            id: Box::new(GValue::Int64(id)),
            label: "knows".into(),
            out_v: Box::new(GValue::Int64(out)),
            out_v_label: "person".into(),
            in_v: Box::new(GValue::Int64(inv)),
            in_v_label: "person".into(),
            properties: new_map(),
        })
    }

    #[test]
    fn paths_dedupe_shared_vertices() {
        let path = |e: i64, to: i64| {
            GValue::Path(Path {
                labels: vec![vec![]; 3],
                objects: vec![vertex(1, None), edge(e, 1, to), vertex(to, None)],
            })
        };
        let set = elements(&[path(7, 2), path(8, 3)]);
        assert_eq!(
            set.vertices.len(),
            3,
            "the shared start vertex appears once"
        );
        assert_eq!(set.edges.len(), 2);
        assert!(has_graph_elements(&[path(7, 2)]));
        assert!(!has_graph_elements(&[GValue::Int32(1)]));
    }

    #[test]
    fn richer_copy_wins_and_maps_are_searched() {
        let mut row = new_map();
        row.insert(GKey::String("e".into()), edge(7, 1, 2));
        row.insert(GKey::String("v".into()), vertex(2, Some("vadas")));
        let set = elements(&[vertex(2, None), GValue::Map(row)]);
        assert_eq!(set.vertices.len(), 1);
        assert_eq!(
            set.vertices[0].properties.len(),
            1,
            "kept the copy with properties"
        );
        assert_eq!(set.edges.len(), 1);
    }

    #[test]
    fn element_maps_are_elements() {
        let mut v = new_map();
        v.insert(GKey::T(T::Id), GValue::Int32(1));
        v.insert(GKey::T(T::Label), GValue::String("person".into()));
        v.insert(GKey::String("name".into()), GValue::String("marko".into()));
        let endpoint = |id: i32| {
            let mut m = new_map();
            m.insert(GKey::T(T::Id), GValue::Int32(id));
            m.insert(GKey::T(T::Label), GValue::String("person".into()));
            GValue::Map(m)
        };
        let mut e = new_map();
        e.insert(GKey::T(T::Id), GValue::Int32(7));
        e.insert(GKey::T(T::Label), GValue::String("knows".into()));
        e.insert(GKey::Direction(Direction::In), endpoint(2));
        e.insert(GKey::Direction(Direction::Out), endpoint(1));
        e.insert(GKey::String("weight".into()), GValue::Double(0.5));
        let set = elements(&[GValue::Map(v), GValue::Map(e)]);
        assert_eq!(set.vertices.len(), 1);
        assert_eq!(set.vertices[0].properties["name"].len(), 1);
        assert_eq!(set.edges.len(), 1);
        assert_eq!(*set.edges[0].out_v, GValue::Int32(1));
        assert_eq!(
            *set.edges[0].properties["weight"].value,
            GValue::Double(0.5)
        );
    }
}
