//! GraphSON v3 decoding and encoding (ADR-0012).

use base64::Engine as _;
use serde_json::{Value, json};

use crate::{
    Direction, Edge, GKey, GValue, IndexMap, Metrics, Path, Property, T, Vertex, VertexProperty,
};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("malformed {type_name}: {reason}")]
    Malformed { type_name: String, reason: String },
}

fn malformed(type_name: &str, reason: impl Into<String>) -> DecodeError {
    DecodeError::Malformed {
        type_name: type_name.to_string(),
        reason: reason.into(),
    }
}

/// Decodes a GraphSON v3 value. Untyped JSON is accepted too (strings, booleans, null, bare numbers).
pub fn decode(v: &Value) -> Result<GValue, DecodeError> {
    match v {
        Value::Null => Ok(GValue::Null),
        Value::Bool(b) => Ok(GValue::Bool(*b)),
        Value::String(s) => Ok(GValue::String(s.clone())),
        Value::Number(n) => Ok(match n.as_i64() {
            Some(i) => GValue::Int64(i),
            None => GValue::Double(n.as_f64().unwrap_or(f64::NAN)),
        }),
        Value::Array(items) => Ok(GValue::List(
            items.iter().map(decode).collect::<Result<_, _>>()?,
        )),
        Value::Object(obj) => match (obj.get("@type"), obj.get("@value")) {
            (Some(Value::String(t)), Some(value)) => decode_typed(t, value),
            (Some(Value::String(t)), None) => decode_typed(t, &Value::Null),
            _ => {
                // An untyped object (GraphSON v1 style or a raw JSON payload): treat as a string-keyed map.
                let mut m = crate::new_map();
                for (k, val) in obj {
                    m.insert(GKey::String(k.clone()), decode(val)?);
                }
                Ok(GValue::Map(m))
            }
        },
    }
}

fn as_f64(t: &str, v: &Value) -> Result<f64, DecodeError> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| malformed(t, "not a number")),
        Value::String(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            other => other
                .parse()
                .map_err(|_| malformed(t, format!("bad float {other:?}"))),
        },
        _ => Err(malformed(t, "expected number")),
    }
}

fn as_i64(t: &str, v: &Value) -> Result<i64, DecodeError> {
    v.as_i64()
        .ok_or_else(|| malformed(t, format!("expected integer, got {v}")))
}

fn as_array<'a>(t: &str, v: &'a Value) -> Result<&'a Vec<Value>, DecodeError> {
    v.as_array().ok_or_else(|| malformed(t, "expected array"))
}

fn field<'a>(t: &str, v: &'a Value, name: &str) -> Result<&'a Value, DecodeError> {
    v.get(name)
        .ok_or_else(|| malformed(t, format!("missing field {name}")))
}

fn str_field(t: &str, v: &Value, name: &str) -> Result<String, DecodeError> {
    match field(t, v, name)? {
        Value::String(s) => Ok(s.clone()),
        other => match decode(other)? {
            GValue::String(s) => Ok(s),
            g => Err(malformed(t, format!("field {name} is {}", g.type_name()))),
        },
    }
}

/// Flat `[k1, v1, k2, v2, …]` → ordered map.
fn decode_map(t: &str, v: &Value) -> Result<IndexMap<GKey, GValue>, DecodeError> {
    let items = as_array(t, v)?;
    if items.len() % 2 != 0 {
        return Err(malformed(t, "odd number of map entries"));
    }
    let mut m =
        crate::IndexMap::with_capacity_and_hasher(items.len() / 2, rustc_hash::FxBuildHasher);
    for pair in items.chunks(2) {
        let k = to_key(&decode(&pair[0])?);
        m.insert(k, decode(&pair[1])?);
    }
    Ok(m)
}

/// Converts a value to a map key.
pub fn to_key(v: &GValue) -> GKey {
    match v {
        GValue::Null => GKey::Null,
        GValue::Bool(b) => GKey::Bool(*b),
        GValue::Int32(_) | GValue::Int64(_) | GValue::Byte(_) | GValue::Short(_) => {
            GKey::Int(v.as_i64().unwrap_or_default())
        }
        GValue::String(s) => GKey::String(s.clone()),
        GValue::Char(c) => GKey::String(c.to_string()),
        GValue::Uuid(u) => GKey::Uuid(*u),
        GValue::T(t) => GKey::T(*t),
        GValue::Direction(d) => GKey::Direction(*d),
        GValue::Token { kind, value } => GKey::Token {
            kind: kind.clone(),
            value: value.clone(),
        },
        GValue::Vertex(vx) => GKey::Vertex {
            id: Box::new(to_key(&vx.id)),
            label: vx.label.clone(),
        },
        GValue::Edge(e) => GKey::Edge {
            id: Box::new(to_key(&e.id)),
            label: e.label.clone(),
        },
        other => GKey::Other(encode(other).to_string()),
    }
}

/// Converts a map key back to a value (for display and re-encoding).
pub fn key_to_value(k: &GKey) -> GValue {
    match k {
        GKey::Null => GValue::Null,
        GKey::Bool(b) => GValue::Bool(*b),
        GKey::Int(i) => GValue::Int64(*i),
        GKey::String(s) => GValue::String(s.clone()),
        GKey::Uuid(u) => GValue::Uuid(*u),
        GKey::T(t) => GValue::T(*t),
        GKey::Direction(d) => GValue::Direction(*d),
        GKey::Token { kind, value } => GValue::Token {
            kind: kind.clone(),
            value: value.clone(),
        },
        GKey::Vertex { id, label } => GValue::Vertex(Vertex {
            id: Box::new(key_to_value(id)),
            label: label.clone(),
            properties: crate::new_map(),
        }),
        GKey::Edge { id, label } => GValue::Edge(Edge {
            id: Box::new(key_to_value(id)),
            label: label.clone(),
            out_v: Box::new(GValue::Null),
            out_v_label: String::new(),
            in_v: Box::new(GValue::Null),
            in_v_label: String::new(),
            properties: crate::new_map(),
        }),
        GKey::Other(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|v| decode(&v).ok())
            .unwrap_or_else(|| GValue::String(text.clone())),
    }
}

fn decode_vertex_property(t: &str, v: &Value) -> Result<VertexProperty, DecodeError> {
    let inner = if v.get("@type").is_some() {
        field(t, v, "@value")?
    } else {
        v
    };
    let mut properties = crate::new_map();
    if let Some(Value::Object(props)) = inner.get("properties") {
        for (k, val) in props {
            properties.insert(k.clone(), decode(val)?);
        }
    }
    Ok(VertexProperty {
        id: Box::new(
            inner
                .get("id")
                .map(decode)
                .transpose()?
                .unwrap_or(GValue::Null),
        ),
        label: str_field(t, inner, "label")?,
        value: Box::new(decode(field(t, inner, "value")?)?),
        properties,
    })
}

fn decode_metrics(t: &str, v: &Value) -> Result<Metrics, DecodeError> {
    // Gremlin Server wraps the metrics map as a typed `g:Map`; accept a bare flat array too.
    let m = match decode(v)? {
        GValue::Map(m) => m,
        _ => decode_map(t, v)?,
    };
    let get = |k: &str| m.get(&GKey::String(k.to_string()));
    let mut out = Metrics {
        id: get("id").and_then(|v| v.as_str().map(String::from)),
        name: get("name").and_then(|v| v.as_str().map(String::from)),
        dur_ms: match get("dur") {
            Some(GValue::Double(d)) => *d,
            Some(GValue::Float(f)) => f64::from(*f),
            Some(other) => other.as_i64().map_or(0.0, |i| i as f64),
            None => 0.0,
        },
        ..Metrics::default()
    };
    if let Some(GValue::Map(counts)) = get("counts") {
        for (k, v) in counts {
            if let (GKey::String(k), Some(n)) = (k, v.as_i64()) {
                out.counts.insert(k.clone(), n);
            }
        }
    }
    if let Some(GValue::Map(ann)) = get("annotations") {
        for (k, v) in ann {
            if let GKey::String(k) = k {
                out.annotations.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(GValue::List(children)) = get("metrics") {
        for c in children {
            if let GValue::Metrics(cm) | GValue::TraversalMetrics(cm) = c {
                out.metrics.push(cm.clone());
            }
        }
    }
    Ok(out)
}

fn decode_typed(t: &str, v: &Value) -> Result<GValue, DecodeError> {
    Ok(match t {
        "g:Int32" => {
            GValue::Int32(i32::try_from(as_i64(t, v)?).map_err(|_| malformed(t, "out of range"))?)
        }
        "g:Int64" => GValue::Int64(as_i64(t, v)?),
        "g:Float" => GValue::Float(as_f64(t, v)? as f32),
        "g:Double" => GValue::Double(as_f64(t, v)?),
        "gx:Byte" => {
            GValue::Byte(i8::try_from(as_i64(t, v)?).map_err(|_| malformed(t, "out of range"))?)
        }
        "gx:Int16" => {
            GValue::Short(i16::try_from(as_i64(t, v)?).map_err(|_| malformed(t, "out of range"))?)
        }
        "gx:BigDecimal" | "gx:BigInteger" => {
            let text = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return Err(malformed(t, "expected number or string")),
            };
            if t == "gx:BigDecimal" {
                GValue::BigDecimal(text)
            } else {
                GValue::BigInteger(text)
            }
        }
        "gx:Char" => GValue::Char(
            v.as_str()
                .and_then(|s| s.chars().next())
                .ok_or_else(|| malformed(t, "expected one character"))?,
        ),
        "g:UUID" => GValue::Uuid(
            v.as_str()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .ok_or_else(|| malformed(t, "bad uuid"))?,
        ),
        "g:Date" => GValue::Date(as_i64(t, v)?),
        "g:Timestamp" => GValue::Timestamp(as_i64(t, v)?),
        "gx:Duration" | "gx:Instant" | "gx:LocalDate" | "gx:LocalDateTime" | "gx:LocalTime"
        | "gx:OffsetDateTime" | "gx:ZonedDateTime" | "gx:Period" => {
            GValue::Duration(v.as_str().unwrap_or_default().to_string())
        }
        "gx:ByteBuffer" => GValue::Bytes(
            base64::engine::general_purpose::STANDARD
                .decode(v.as_str().unwrap_or_default())
                .map_err(|e| malformed(t, e.to_string()))?,
        ),
        "g:List" => GValue::List(
            as_array(t, v)?
                .iter()
                .map(decode)
                .collect::<Result<_, _>>()?,
        ),
        "g:Set" => GValue::Set(
            as_array(t, v)?
                .iter()
                .map(decode)
                .collect::<Result<_, _>>()?,
        ),
        "g:Map" => GValue::Map(decode_map(t, v)?),
        "g:BulkSet" => {
            let items = as_array(t, v)?;
            let mut out = Vec::with_capacity(items.len() / 2);
            for pair in items.chunks(2) {
                let bulk = pair
                    .get(1)
                    .map(decode)
                    .transpose()?
                    .and_then(|b| b.as_i64())
                    .unwrap_or(1);
                out.push((decode(&pair[0])?, bulk));
            }
            GValue::BulkSet(out)
        }
        "g:Vertex" => {
            let mut properties: IndexMap<String, Vec<VertexProperty>> = crate::new_map();
            if let Some(Value::Object(props)) = v.get("properties") {
                for (k, list) in props {
                    let vps = as_array(t, list)?
                        .iter()
                        .map(|p| decode_vertex_property("g:VertexProperty", p))
                        .collect::<Result<_, _>>()?;
                    properties.insert(k.clone(), vps);
                }
            }
            GValue::Vertex(Vertex {
                id: Box::new(decode(field(t, v, "id")?)?),
                label: v
                    .get("label")
                    .map_or(Ok("vertex".to_string()), |_| str_field(t, v, "label"))?,
                properties,
            })
        }
        "g:Edge" => {
            let mut properties = crate::new_map();
            if let Some(Value::Object(props)) = v.get("properties") {
                for (k, p) in props {
                    let value = match decode(p)? {
                        GValue::Property(prop) => *prop.value,
                        other => other,
                    };
                    properties.insert(
                        k.clone(),
                        Property {
                            key: k.clone(),
                            value: Box::new(value),
                        },
                    );
                }
            }
            GValue::Edge(Edge {
                id: Box::new(decode(field(t, v, "id")?)?),
                label: str_field(t, v, "label")?,
                out_v: Box::new(decode(field(t, v, "outV")?)?),
                out_v_label: v
                    .get("outVLabel")
                    .and_then(Value::as_str)
                    .unwrap_or("vertex")
                    .to_string(),
                in_v: Box::new(decode(field(t, v, "inV")?)?),
                in_v_label: v
                    .get("inVLabel")
                    .and_then(Value::as_str)
                    .unwrap_or("vertex")
                    .to_string(),
                properties,
            })
        }
        "g:VertexProperty" => GValue::VertexProperty(decode_vertex_property(t, v)?),
        "g:Property" => GValue::Property(Property {
            key: str_field(t, v, "key")?,
            value: Box::new(decode(field(t, v, "value")?)?),
        }),
        "g:Path" => {
            let labels = match decode(field(t, v, "labels")?)? {
                GValue::List(sets) => sets
                    .into_iter()
                    .map(|s| match s {
                        GValue::Set(items) | GValue::List(items) => items
                            .into_iter()
                            .filter_map(|i| i.as_str().map(String::from))
                            .collect(),
                        _ => Vec::new(),
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let objects = match decode(field(t, v, "objects")?)? {
                GValue::List(o) => o,
                other => vec![other],
            };
            GValue::Path(Path { labels, objects })
        }
        "g:Tree" => {
            let mut out = Vec::new();
            for entry in as_array(t, v)? {
                out.push((
                    decode(field(t, entry, "key")?)?,
                    decode(field(t, entry, "value")?)?,
                ));
            }
            GValue::Tree(out)
        }
        "g:Traverser" => GValue::Traverser {
            bulk: decode(field(t, v, "bulk")?)?.as_i64().unwrap_or(1),
            value: Box::new(decode(field(t, v, "value")?)?),
        },
        "g:T" => GValue::T(match v.as_str().unwrap_or_default() {
            "id" => T::Id,
            "label" => T::Label,
            "key" => T::Key,
            "value" => T::Value,
            other => return Err(malformed(t, format!("unknown T {other}"))),
        }),
        "g:Direction" => GValue::Direction(match v.as_str().unwrap_or_default() {
            "OUT" => Direction::Out,
            "IN" => Direction::In,
            "BOTH" => Direction::Both,
            other => return Err(malformed(t, format!("unknown Direction {other}"))),
        }),
        "g:Class" => GValue::Class(v.as_str().unwrap_or_default().to_string()),
        "g:TraversalMetrics" => GValue::TraversalMetrics(decode_metrics(t, v)?),
        "g:Metrics" => GValue::Metrics(decode_metrics(t, v)?),
        "g:Order" | "g:Scope" | "g:Column" | "g:Pop" | "g:Cardinality" | "g:Merge"
        | "g:Operator" | "g:Barrier" | "g:Pick" => GValue::Token {
            kind: t.trim_start_matches("g:").to_string(),
            value: v.as_str().unwrap_or_default().to_string(),
        },
        other => GValue::Unknown {
            type_name: other.to_string(),
            value: v.clone(),
        },
    })
}

fn typed(t: &str, v: Value) -> Value {
    json!({ "@type": t, "@value": v })
}

/// Encodes a value as GraphSON v3 (used for request bindings).
pub fn encode(v: &GValue) -> Value {
    match v {
        GValue::Null => Value::Null,
        GValue::Bool(b) => Value::Bool(*b),
        GValue::Int32(i) => typed("g:Int32", json!(i)),
        GValue::Int64(i) => typed("g:Int64", json!(i)),
        GValue::Float(f) => typed("g:Float", float_json(f64::from(*f))),
        GValue::Double(d) => typed("g:Double", float_json(*d)),
        GValue::BigDecimal(s) => typed("gx:BigDecimal", json!(s)),
        GValue::BigInteger(s) => typed("gx:BigInteger", json!(s)),
        GValue::Byte(b) => typed("gx:Byte", json!(b)),
        GValue::Short(s) => typed("gx:Int16", json!(s)),
        GValue::Char(c) => typed("gx:Char", json!(c.to_string())),
        GValue::String(s) => Value::String(s.clone()),
        GValue::Uuid(u) => typed("g:UUID", json!(u.to_string())),
        GValue::Date(ms) => typed("g:Date", json!(ms)),
        GValue::Timestamp(ms) => typed("g:Timestamp", json!(ms)),
        GValue::Duration(s) => typed("gx:Duration", json!(s)),
        GValue::Bytes(b) => typed(
            "gx:ByteBuffer",
            json!(base64::engine::general_purpose::STANDARD.encode(b)),
        ),
        GValue::List(items) => typed("g:List", Value::Array(items.iter().map(encode).collect())),
        GValue::Set(items) => typed("g:Set", Value::Array(items.iter().map(encode).collect())),
        GValue::BulkSet(items) => typed(
            "g:BulkSet",
            Value::Array(
                items
                    .iter()
                    .flat_map(|(v, b)| [encode(v), typed("g:Int64", json!(b))])
                    .collect(),
            ),
        ),
        GValue::Map(m) => typed(
            "g:Map",
            Value::Array(
                m.iter()
                    .flat_map(|(k, v)| [encode(&key_to_value(k)), encode(v)])
                    .collect(),
            ),
        ),
        GValue::Vertex(vx) => {
            let mut body = json!({ "id": encode(&vx.id), "label": vx.label });
            if !vx.properties.is_empty() {
                let props: serde_json::Map<String, Value> = vx
                    .properties
                    .iter()
                    .map(|(k, vps)| {
                        (
                            k.clone(),
                            Value::Array(vps.iter().map(encode_vertex_property).collect()),
                        )
                    })
                    .collect();
                body["properties"] = Value::Object(props);
            }
            typed("g:Vertex", body)
        }
        GValue::Edge(e) => {
            let mut body = json!({
                "id": encode(&e.id), "label": e.label,
                "outV": encode(&e.out_v), "outVLabel": e.out_v_label,
                "inV": encode(&e.in_v), "inVLabel": e.in_v_label,
            });
            if !e.properties.is_empty() {
                let props: serde_json::Map<String, Value> = e
                    .properties
                    .iter()
                    .map(|(k, p)| {
                        (
                            k.clone(),
                            typed(
                                "g:Property",
                                json!({ "key": p.key, "value": encode(&p.value) }),
                            ),
                        )
                    })
                    .collect();
                body["properties"] = Value::Object(props);
            }
            typed("g:Edge", body)
        }
        GValue::VertexProperty(p) => encode_vertex_property(p),
        GValue::Property(p) => typed(
            "g:Property",
            json!({ "key": p.key, "value": encode(&p.value) }),
        ),
        GValue::Path(p) => typed(
            "g:Path",
            json!({
                "labels": typed("g:List", Value::Array(p.labels.iter().map(|l| typed("g:Set", json!(l))).collect())),
                "objects": typed("g:List", Value::Array(p.objects.iter().map(encode).collect())),
            }),
        ),
        GValue::Tree(entries) => typed(
            "g:Tree",
            Value::Array(
                entries
                    .iter()
                    .map(|(k, v)| json!({ "key": encode(k), "value": encode(v) }))
                    .collect(),
            ),
        ),
        GValue::Traverser { bulk, value } => typed(
            "g:Traverser",
            json!({ "bulk": typed("g:Int64", json!(bulk)), "value": encode(value) }),
        ),
        GValue::T(t) => typed("g:T", json!(t.to_string())),
        GValue::Direction(d) => typed("g:Direction", json!(d.to_string())),
        GValue::Token { kind, value } => typed(&format!("g:{kind}"), json!(value)),
        GValue::Class(c) => typed("g:Class", json!(c)),
        GValue::TraversalMetrics(m) => {
            typed("g:TraversalMetrics", typed("g:Map", encode_metrics(m)))
        }
        GValue::Metrics(m) => typed("g:Metrics", typed("g:Map", encode_metrics(m))),
        GValue::Unknown { type_name, value } => typed(type_name, value.clone()),
    }
}

fn encode_vertex_property(p: &VertexProperty) -> Value {
    let mut body = json!({ "id": encode(&p.id), "label": p.label, "value": encode(&p.value) });
    if !p.properties.is_empty() {
        let meta: serde_json::Map<String, Value> = p
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), encode(v)))
            .collect();
        body["properties"] = Value::Object(meta);
    }
    typed("g:VertexProperty", body)
}

fn float_json(f: f64) -> Value {
    if f.is_nan() {
        json!("NaN")
    } else if f.is_infinite() {
        json!(if f > 0.0 { "Infinity" } else { "-Infinity" })
    } else {
        json!(f)
    }
}

fn encode_metrics(m: &Metrics) -> Value {
    let mut flat = vec![json!("dur"), typed("g:Double", json!(m.dur_ms))];
    if let Some(id) = &m.id {
        flat.extend([json!("id"), json!(id)]);
    }
    if let Some(name) = &m.name {
        flat.extend([json!("name"), json!(name)]);
    }
    let counts: Vec<Value> = m
        .counts
        .iter()
        .flat_map(|(k, v)| [json!(k), typed("g:Int64", json!(v))])
        .collect();
    flat.extend([json!("counts"), typed("g:Map", Value::Array(counts))]);
    let ann: Vec<Value> = m
        .annotations
        .iter()
        .flat_map(|(k, v)| [json!(k), encode(v)])
        .collect();
    flat.extend([json!("annotations"), typed("g:Map", Value::Array(ann))]);
    if !m.metrics.is_empty() {
        let children: Vec<Value> = m
            .metrics
            .iter()
            .map(|c| typed("g:Metrics", typed("g:Map", encode_metrics(c))))
            .collect();
        flat.extend([json!("metrics"), typed("g:List", Value::Array(children))]);
    }
    Value::Array(flat)
}
