//! Plain JSON rendering of Gremlin values (no GraphSON `@type` wrappers), for CLI output and exports.

use serde_json::{Value, json};

use crate::graphson::key_to_value;
use crate::{GKey, GValue};

/// Converts a value to idiomatic JSON. Elements become objects with `id`, `label`, and `properties`;
/// maps with non-string keys use the key's plain JSON text as the object key.
pub fn to_json(v: &GValue) -> Value {
    match v {
        GValue::Null => Value::Null,
        GValue::Bool(b) => json!(b),
        GValue::Int32(i) => json!(i),
        GValue::Int64(i) => json!(i),
        GValue::Float(f) => json!(f),
        GValue::Double(d) => json!(d),
        GValue::BigDecimal(s) | GValue::BigInteger(s) | GValue::Duration(s) | GValue::Class(s) => {
            json!(s)
        }
        GValue::Byte(b) => json!(b),
        GValue::Short(s) => json!(s),
        GValue::Char(c) => json!(c.to_string()),
        GValue::String(s) => json!(s),
        GValue::Uuid(u) => json!(u.to_string()),
        GValue::Date(ms) | GValue::Timestamp(ms) => json!(ms),
        GValue::Bytes(b) => json!(b),
        GValue::List(items) | GValue::Set(items) => {
            Value::Array(items.iter().map(to_json).collect())
        }
        GValue::BulkSet(items) => Value::Array(
            items
                .iter()
                .flat_map(|(v, n)| std::iter::repeat_n(to_json(v), *n as usize))
                .collect(),
        ),
        GValue::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in m {
                obj.insert(key_text(k), to_json(v));
            }
            Value::Object(obj)
        }
        GValue::Vertex(vx) => {
            let props: serde_json::Map<String, Value> = vx
                .properties
                .iter()
                .map(|(k, vps)| {
                    let vals: Vec<Value> = vps.iter().map(|p| to_json(&p.value)).collect();
                    (
                        k.clone(),
                        if vals.len() == 1 {
                            vals.into_iter().next().unwrap_or(Value::Null)
                        } else {
                            Value::Array(vals)
                        },
                    )
                })
                .collect();
            let mut o = json!({ "id": to_json(&vx.id), "label": vx.label, "type": "vertex" });
            if !props.is_empty() {
                o["properties"] = Value::Object(props);
            }
            o
        }
        GValue::Edge(e) => {
            let props: serde_json::Map<String, Value> = e
                .properties
                .iter()
                .map(|(k, p)| (k.clone(), to_json(&p.value)))
                .collect();
            let mut o = json!({
                "id": to_json(&e.id), "label": e.label, "type": "edge",
                "outV": to_json(&e.out_v), "outVLabel": e.out_v_label,
                "inV": to_json(&e.in_v), "inVLabel": e.in_v_label,
            });
            if !props.is_empty() {
                o["properties"] = Value::Object(props);
            }
            o
        }
        GValue::VertexProperty(p) => {
            json!({ "id": to_json(&p.id), "key": p.label, "value": to_json(&p.value) })
        }
        GValue::Property(p) => json!({ "key": p.key, "value": to_json(&p.value) }),
        GValue::Path(p) => {
            json!({ "labels": p.labels, "objects": p.objects.iter().map(to_json).collect::<Vec<_>>() })
        }
        GValue::Tree(entries) => Value::Array(
            entries
                .iter()
                .map(|(k, v)| json!({ "key": to_json(k), "value": to_json(v) }))
                .collect(),
        ),
        GValue::Traverser { bulk, value } => json!({ "bulk": bulk, "value": to_json(value) }),
        GValue::T(t) => json!(t.to_string()),
        GValue::Direction(d) => json!(d.to_string()),
        GValue::Token { value, .. } => json!(value),
        GValue::TraversalMetrics(m) | GValue::Metrics(m) => json!({
            "name": m.name, "dur_ms": m.dur_ms, "counts": m.counts,
            "metrics": m.metrics.iter().map(|c| to_json(&GValue::Metrics(c.clone()))).collect::<Vec<_>>(),
        }),
        GValue::Unknown { value, .. } => value.clone(),
    }
}

fn key_text(k: &GKey) -> String {
    match k {
        GKey::String(s) => s.clone(),
        GKey::T(t) => t.to_string(),
        other => match to_json(&key_to_value(other)) {
            Value::String(s) => s,
            v => v.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_with_token_and_int_keys() {
        let mut m = crate::new_map();
        m.insert(GKey::T(crate::T::Label), GValue::String("person".into()));
        m.insert(GKey::Int(3), GValue::Int32(1));
        assert_eq!(
            to_json(&GValue::Map(m)),
            json!({ "label": "person", "3": 1 })
        );
    }
}
