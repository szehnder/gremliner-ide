//! The schema as a graph: one vertex per vertex label and one edge per connectivity triple, so the
//! regular graph view can draw it. Vertex ids are the label names; edge ids are `out-label->in`.

use gremlin_model::{Edge, GValue, Property, Vertex, VertexProperty, new_map};

use crate::model::Schema;

/// Vertices then edges, as Gremlin values.
pub fn elements(schema: &Schema) -> Vec<GValue> {
    let mut out = Vec::with_capacity(schema.vertex_labels.len() + schema.triples.len());
    let mut names: Vec<&str> = schema
        .vertex_labels
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    // Triples can mention labels discovery has not listed yet (partial label scans).
    for t in &schema.triples {
        for l in [&t.out_label, &t.in_label] {
            if !names.contains(&l.as_str()) {
                names.push(l);
            }
        }
    }
    for name in names {
        let info = schema.vertex_label(name);
        let mut props = new_map();
        let mut add = |key: &str, value: GValue| {
            props.insert(
                key.to_owned(),
                vec![VertexProperty {
                    id: Box::new(GValue::String(format!("{name}.{key}"))),
                    label: key.to_owned(),
                    value: Box::new(value),
                    properties: new_map(),
                }],
            );
        };
        add("name", GValue::String(name.to_owned()));
        if let Some(count) = info.and_then(|i| i.count) {
            add("count", GValue::String(count.text()));
        }
        if let Some(info) = info.filter(|i| !i.properties.is_empty()) {
            let keys: Vec<String> = info
                .properties
                .iter()
                .map(|p| format!("{}: {}", p.key, p.type_text()))
                .collect();
            add("properties", GValue::String(keys.join(", ")));
        }
        out.push(GValue::Vertex(Vertex {
            id: Box::new(GValue::String(name.to_owned())),
            label: name.to_owned(),
            properties: props,
        }));
    }
    for t in &schema.triples {
        let mut props = new_map();
        if let Some(count) = schema.edge_label(&t.label).and_then(|l| l.count) {
            props.insert(
                "count".to_owned(),
                Property {
                    key: "count".into(),
                    value: Box::new(GValue::String(count.text())),
                },
            );
        }
        out.push(GValue::Edge(Edge {
            id: Box::new(GValue::String(t.text())),
            label: t.label.clone(),
            out_v: Box::new(GValue::String(t.out_label.clone())),
            out_v_label: t.out_label.clone(),
            in_v: Box::new(GValue::String(t.in_label.clone())),
            in_v_label: t.in_label.clone(),
            properties: props,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Count, Triple};

    #[test]
    fn labels_become_vertices_and_triples_edges() {
        let mut s = Schema::new(0);
        s.vertex_label_mut("person").count = Some(Count::exact(4));
        s.vertex_label_mut("software");
        s.triples.push(Triple::new("person", "created", "software"));
        s.triples.push(Triple::new("person", "knows", "unlisted"));
        let items = elements(&s);
        let vertices: Vec<_> = items
            .iter()
            .filter_map(|v| match v {
                GValue::Vertex(v) => Some(v.label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(vertices, ["person", "software", "unlisted"]);
        let edges = items
            .iter()
            .filter(|v| matches!(v, GValue::Edge(_)))
            .count();
        assert_eq!(edges, 2);
    }
}
