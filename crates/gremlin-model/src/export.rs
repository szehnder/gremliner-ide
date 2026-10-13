//! Data exports (export): GraphML and GraphSON for graph elements, and JSON Lines for results.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::GValue;
use crate::elements::ElementSet;

/// GraphML `attr.type` for a value.
fn graphml_type(v: &GValue) -> &'static str {
    match v {
        GValue::Bool(_) => "boolean",
        GValue::Int32(_) | GValue::Short(_) | GValue::Byte(_) => "int",
        GValue::Int64(_) | GValue::Date(_) | GValue::Timestamp(_) => "long",
        GValue::Float(_) => "float",
        GValue::Double(_) => "double",
        _ => "string",
    }
}

fn scalar_text(v: &GValue) -> String {
    match v {
        GValue::String(s) => s.clone(),
        GValue::Date(ms) | GValue::Timestamp(ms) => ms.to_string(),
        other => crate::console::format(other),
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// The elements as GraphML, readable by TinkerPop's GraphMLReader (`labelV`/`labelE` keys). Multi-valued
/// vertex properties keep their first value; meta-properties are not written (GraphML has no place
/// for them).
pub fn graphml(set: &ElementSet) -> String {
    // Keys: (for, name) → type, the first value's type winning.
    let mut keys: BTreeMap<(&'static str, String), &'static str> = BTreeMap::new();
    for v in &set.vertices {
        for (k, props) in &v.properties {
            if let Some(p) = props.first() {
                keys.entry(("node", k.clone()))
                    .or_insert_with(|| graphml_type(&p.value));
            }
        }
    }
    for e in &set.edges {
        for (k, p) in &e.properties {
            keys.entry(("edge", k.clone()))
                .or_insert_with(|| graphml_type(&p.value));
        }
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n",
    );
    out.push_str("  <key id=\"labelV\" for=\"node\" attr.name=\"labelV\" attr.type=\"string\"/>\n");
    out.push_str("  <key id=\"labelE\" for=\"edge\" attr.name=\"labelE\" attr.type=\"string\"/>\n");
    for ((target, name), ty) in &keys {
        let id = escape(name);
        // Node and edge keys of the same name need distinct ids.
        let key_id = if *target == "edge" && keys.contains_key(&("node", name.clone())) {
            format!("{id}_e")
        } else {
            id.clone()
        };
        let _ = writeln!(
            out,
            "  <key id=\"{key_id}\" for=\"{target}\" attr.name=\"{id}\" attr.type=\"{ty}\"/>"
        );
    }
    out.push_str("  <graph id=\"G\" edgedefault=\"directed\">\n");
    for v in &set.vertices {
        let _ = writeln!(out, "    <node id=\"{}\">", escape(&scalar_text(&v.id)));
        let _ = writeln!(
            out,
            "      <data key=\"labelV\">{}</data>",
            escape(&v.label)
        );
        for (k, props) in &v.properties {
            if let Some(p) = props.first() {
                let _ = writeln!(
                    out,
                    "      <data key=\"{}\">{}</data>",
                    escape(k),
                    escape(&scalar_text(&p.value))
                );
            }
        }
        out.push_str("    </node>\n");
    }
    for e in &set.edges {
        let _ = writeln!(
            out,
            "    <edge id=\"{}\" source=\"{}\" target=\"{}\">",
            escape(&scalar_text(&e.id)),
            escape(&scalar_text(&e.out_v)),
            escape(&scalar_text(&e.in_v))
        );
        let _ = writeln!(
            out,
            "      <data key=\"labelE\">{}</data>",
            escape(&e.label)
        );
        for (k, p) in &e.properties {
            let key_id = if keys.contains_key(&("node", k.clone())) {
                format!("{}_e", escape(k))
            } else {
                escape(k)
            };
            let _ = writeln!(
                out,
                "      <data key=\"{key_id}\">{}</data>",
                escape(&scalar_text(&p.value))
            );
        }
        out.push_str("    </edge>\n");
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

/// The elements as typed GraphSON 3: a JSON object with the vertex and edge lists.
pub fn graphson(set: &ElementSet) -> String {
    let vertices: Vec<serde_json::Value> = set
        .vertices
        .iter()
        .map(|v| crate::graphson::encode(&GValue::Vertex(v.clone())))
        .collect();
    let edges: Vec<serde_json::Value> = set
        .edges
        .iter()
        .map(|e| crate::graphson::encode(&GValue::Edge(e.clone())))
        .collect();
    let doc = serde_json::json!({ "vertices": vertices, "edges": edges });
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

/// One plain-JSON value per line.
pub fn json_lines(items: &[GValue]) -> String {
    let mut out = String::new();
    for item in items {
        out.push_str(&crate::plain::to_json(item).to_string());
        out.push('\n');
    }
    out
}

/// Elements as a JSON array of plain objects (`id`, `label`, `properties`; edges also `outV`/`inV`),
/// for "Copy as JSON".
pub fn elements_json(set: &ElementSet) -> String {
    let mut arr = Vec::new();
    for v in &set.vertices {
        arr.push(crate::plain::to_json(&GValue::Vertex(v.clone())));
    }
    for e in &set.edges {
        arr.push(crate::plain::to_json(&GValue::Edge(e.clone())));
    }
    serde_json::to_string_pretty(&serde_json::Value::Array(arr)).unwrap_or_default()
}
