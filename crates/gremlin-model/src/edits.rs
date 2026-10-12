//! Data edits as Gremlin statements: each [`EditCommand`] becomes one statement whose values (ids, keys,
//! property values, labels) travel as typed bindings (ADR-0030), so ids keep their exact type (Neptune
//! strings, TinkerGraph longs or ints). Commands know their inverse when the pre-edit state is known.
//!
//! Binding names avoid `id`, `key`, `value` and `label`: Gremlin Server statically imports `T.*`, and
//! those names would resolve to the tokens rather than the bindings.

use std::fmt::Write as _;

use crate::{GValue, console};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Vertex,
    Edge,
}

/// A vertex or edge, by id.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementRef {
    pub kind: ElementKind,
    pub id: GValue,
}

impl ElementRef {
    pub fn vertex(id: GValue) -> Self {
        Self {
            kind: ElementKind::Vertex,
            id,
        }
    }

    pub fn edge(id: GValue) -> Self {
        Self {
            kind: ElementKind::Edge,
            id,
        }
    }
}

/// Vertex property cardinality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinality {
    Single,
    List,
    Set,
}

impl Cardinality {
    pub fn token(self) -> &'static str {
        match self {
            Cardinality::Single => "single",
            Cardinality::List => "list",
            Cardinality::Set => "set",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditCommand {
    /// Set a property (vertex: with cardinality, `single` replacing existing values).
    SetProperty {
        element: ElementRef,
        key: String,
        value: GValue,
        cardinality: Cardinality,
    },
    RemoveProperty {
        element: ElementRef,
        key: String,
    },
    AddVertex {
        label: String,
        props: Vec<(String, GValue)>,
    },
    AddEdge {
        from: GValue,
        to: GValue,
        label: String,
        props: Vec<(String, GValue)>,
    },
    /// Drop vertices (with their incident edges) and edges.
    DropElements {
        vertices: Vec<GValue>,
        edges: Vec<GValue>,
    },
}

/// A statement ready to send: the script plus its bindings, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Statement {
    pub script: String,
    pub bindings: Vec<(String, GValue)>,
}

impl Statement {
    /// The script followed by one `name = value (Type)` line per binding, for previews.
    pub fn preview(&self) -> String {
        let mut s = self.script.clone();
        if !self.bindings.is_empty() {
            s.push('\n');
            for (k, v) in &self.bindings {
                let _ = write!(s, "\n{k} = {} ({})", console::format(v), v.type_name());
            }
        }
        s
    }
}

fn source(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Vertex => "V",
        ElementKind::Edge => "E",
    }
}

fn props_steps(props: &[(String, GValue)], bindings: &mut Vec<(String, GValue)>) -> String {
    let mut s = String::new();
    for (i, (k, v)) in props.iter().enumerate() {
        let _ = write!(s, ".property(propKey{i}, propValue{i})");
        bindings.push((format!("propKey{i}"), GValue::String(k.clone())));
        bindings.push((format!("propValue{i}"), v.clone()));
    }
    s
}

impl EditCommand {
    pub fn statement(&self) -> Statement {
        let mut bindings = Vec::new();
        let script = match self {
            EditCommand::SetProperty {
                element,
                key,
                value,
                cardinality,
            } => {
                bindings.push(("elementId".into(), element.id.clone()));
                bindings.push(("propKey".into(), GValue::String(key.clone())));
                bindings.push(("propValue".into(), value.clone()));
                match element.kind {
                    ElementKind::Vertex => format!(
                        "g.V(elementId).property({}, propKey, propValue)",
                        cardinality.token()
                    ),
                    ElementKind::Edge => "g.E(elementId).property(propKey, propValue)".into(),
                }
            }
            EditCommand::RemoveProperty { element, key } => {
                bindings.push(("elementId".into(), element.id.clone()));
                bindings.push(("propKey".into(), GValue::String(key.clone())));
                format!(
                    "g.{}(elementId).properties(propKey).drop()",
                    source(element.kind)
                )
            }
            EditCommand::AddVertex { label, props } => {
                bindings.push(("vertexLabel".into(), GValue::String(label.clone())));
                format!("g.addV(vertexLabel){}", props_steps(props, &mut bindings))
            }
            EditCommand::AddEdge {
                from,
                to,
                label,
                props,
            } => {
                bindings.push(("fromId".into(), from.clone()));
                bindings.push(("toId".into(), to.clone()));
                bindings.push(("edgeLabel".into(), GValue::String(label.clone())));
                format!(
                    "g.V(fromId).addE(edgeLabel).to(__.V(toId)){}",
                    props_steps(props, &mut bindings)
                )
            }
            EditCommand::DropElements { vertices, edges } => {
                let one_or_list = |ids: &[GValue]| match ids {
                    [one] => one.clone(),
                    many => GValue::List(many.to_vec()),
                };
                let mut parts = Vec::new();
                if !edges.is_empty() {
                    bindings.push(("eids".into(), one_or_list(edges)));
                    parts.push("g.E(eids).drop()");
                }
                if !vertices.is_empty() {
                    bindings.push(("vids".into(), one_or_list(vertices)));
                    parts.push("g.V(vids).drop()");
                }
                match parts.as_slice() {
                    [a, b] => format!("{a}.iterate(); {b}"),
                    [a] => (*a).to_owned(),
                    _ => "g.inject(0).limit(0)".to_owned(),
                }
            }
        };
        Statement { script, bindings }
    }

    /// A one-line description for previews and the undo list.
    pub fn describe(&self) -> String {
        let el = |e: &ElementRef| {
            let kind = match e.kind {
                ElementKind::Vertex => "vertex",
                ElementKind::Edge => "edge",
            };
            format!("{kind} {}", console::format(&e.id))
        };
        match self {
            EditCommand::SetProperty {
                element,
                key,
                value,
                ..
            } => format!("Set {key} = {} on {}", console::format(value), el(element)),
            EditCommand::RemoveProperty { element, key } => {
                format!("Remove {key} from {}", el(element))
            }
            EditCommand::AddVertex { label, .. } => format!("Create a {label} vertex"),
            EditCommand::AddEdge { label, .. } => format!("Create a {label} edge"),
            EditCommand::DropElements { vertices, edges } => {
                format!("Delete {}", counts(vertices.len(), edges.len()))
            }
        }
    }

    /// True for statements that remove data.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            EditCommand::RemoveProperty { .. } | EditCommand::DropElements { .. }
        )
    }

    /// The command that undoes this one, given the property's values before the edit (`None`: it was
    /// absent). Creations are undone with [`EditCommand::undo_creation`] once the new id is known;
    /// deletions cannot be undone.
    pub fn inverse(&self, before: Option<&[GValue]>) -> Option<EditCommand> {
        match self {
            EditCommand::SetProperty { element, key, .. }
            | EditCommand::RemoveProperty { element, key } => {
                match before.and_then(<[GValue]>::first) {
                    Some(old) => Some(EditCommand::SetProperty {
                        element: element.clone(),
                        key: key.clone(),
                        value: old.clone(),
                        cardinality: Cardinality::Single,
                    }),
                    None => Some(EditCommand::RemoveProperty {
                        element: element.clone(),
                        key: key.clone(),
                    }),
                }
            }
            EditCommand::AddVertex { .. }
            | EditCommand::AddEdge { .. }
            | EditCommand::DropElements { .. } => None,
        }
    }

    /// The inverse of a creation, once the server returned the new element's id.
    pub fn undo_creation(&self, created: &GValue) -> Option<EditCommand> {
        match self {
            EditCommand::AddVertex { .. } => Some(EditCommand::DropElements {
                vertices: vec![created.clone()],
                edges: vec![],
            }),
            EditCommand::AddEdge { .. } => Some(EditCommand::DropElements {
                vertices: vec![],
                edges: vec![created.clone()],
            }),
            _ => None,
        }
    }
}

/// "1 vertex and 3 edges", "2 vertices", "1 edge".
pub fn counts(vertices: usize, edges: usize) -> String {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (vertices, edges) {
        (0, e) => plural(e, "edge", "edges"),
        (v, 0) => plural(v, "vertex", "vertices"),
        (v, e) => format!(
            "{} and {}",
            plural(v, "vertex", "vertices"),
            plural(e, "edge", "edges")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(c: &EditCommand) -> String {
        c.statement().preview()
    }

    #[test]
    fn statements_use_bindings() {
        let marko = ElementRef::vertex(GValue::Int32(1));
        insta::assert_snapshot!(snapshot(&EditCommand::SetProperty {
            element: marko.clone(),
            key: "age".into(),
            value: GValue::Int32(30),
            cardinality: Cardinality::Single,
        }), @r"
        g.V(elementId).property(single, propKey, propValue)

        elementId = 1 (Int32)
        propKey = age (String)
        propValue = 30 (Int32)
        ");
        insta::assert_snapshot!(snapshot(&EditCommand::SetProperty {
            element: ElementRef::edge(GValue::String("e-7".into())),
            key: "weight".into(),
            value: GValue::Double(0.5),
            cardinality: Cardinality::Single,
        }), @r"
        g.E(elementId).property(propKey, propValue)

        elementId = e-7 (String)
        propKey = weight (String)
        propValue = 0.5 (Double)
        ");
        insta::assert_snapshot!(snapshot(&EditCommand::RemoveProperty {
            element: marko,
            key: "age".into(),
        }), @r"
        g.V(elementId).properties(propKey).drop()

        elementId = 1 (Int32)
        propKey = age (String)
        ");
        insta::assert_snapshot!(snapshot(&EditCommand::AddVertex {
            label: "person".into(),
            props: vec![("name".into(), GValue::String("ann".into()))],
        }), @r"
        g.addV(vertexLabel).property(propKey0, propValue0)

        vertexLabel = person (String)
        propKey0 = name (String)
        propValue0 = ann (String)
        ");
        insta::assert_snapshot!(snapshot(&EditCommand::AddEdge {
            from: GValue::Int64(1),
            to: GValue::Int64(3),
            label: "created".into(),
            props: vec![],
        }), @r"
        g.V(fromId).addE(edgeLabel).to(__.V(toId))

        fromId = 1 (Int64)
        toId = 3 (Int64)
        edgeLabel = created (String)
        ");
        insta::assert_snapshot!(snapshot(&EditCommand::DropElements {
            vertices: vec![GValue::Int32(4), GValue::Int32(6)],
            edges: vec![GValue::Int32(9)],
        }), @r"
        g.E(eids).drop().iterate(); g.V(vids).drop()

        eids = 9 (Int32)
        vids = [4,6] (List)
        ");
    }

    #[test]
    fn inverses() {
        let el = ElementRef::vertex(GValue::Int32(1));
        let set = EditCommand::SetProperty {
            element: el.clone(),
            key: "age".into(),
            value: GValue::Int32(30),
            cardinality: Cardinality::Single,
        };
        assert_eq!(
            set.inverse(Some(&[GValue::Int32(29)])),
            Some(EditCommand::SetProperty {
                element: el.clone(),
                key: "age".into(),
                value: GValue::Int32(29),
                cardinality: Cardinality::Single,
            })
        );
        assert_eq!(
            set.inverse(None),
            Some(EditCommand::RemoveProperty {
                element: el.clone(),
                key: "age".into()
            }),
            "a new property is removed again"
        );
        let add = EditCommand::AddVertex {
            label: "x".into(),
            props: vec![],
        };
        assert_eq!(add.inverse(None), None);
        assert_eq!(
            add.undo_creation(&GValue::Int64(99)),
            Some(EditCommand::DropElements {
                vertices: vec![GValue::Int64(99)],
                edges: vec![]
            })
        );
        assert_eq!(counts(1, 3), "1 vertex and 3 edges");
        assert_eq!(counts(2, 0), "2 vertices");
        assert_eq!(counts(0, 1), "1 edge");
    }
}
