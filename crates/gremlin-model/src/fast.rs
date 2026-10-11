//! Streaming GraphSON v3 decoder: deserialises bytes straight into [`GValue`] with serde, without an
//! intermediate `serde_json::Value` tree (~4× faster than `graphson::decode` on large responses).
//! `graphson::decode` remains the reference implementation; tests assert both agree on real responses.

use std::fmt;

use base64::Engine as _;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

use crate::graphson::to_key;
use crate::{Direction, Edge, GValue, IndexMap, Path, Property, T, Vertex, VertexProperty};

/// Decodes GraphSON v3 bytes into a value.
pub fn decode_slice(bytes: &[u8]) -> Result<GValue, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let v = AnyValue.deserialize(&mut de)?;
    de.end()?;
    Ok(v)
}

/// Decodes GraphSON v3 bytes with SIMD-accelerated parsing. Parses in place, so it takes ownership of
/// the buffer (response frames are owned anyway).
pub fn decode_owned(mut bytes: Vec<u8>) -> Result<GValue, String> {
    let mut de = simd_json::Deserializer::from_slice(&mut bytes).map_err(|e| e.to_string())?;
    AnyValue.deserialize(&mut de).map_err(|e| e.to_string())
}

/// Seed for any GraphSON value (typed or untyped).
#[derive(Clone, Copy)]
pub struct AnyValue;

impl<'de> DeserializeSeed<'de> for AnyValue {
    type Value = GValue;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<GValue, D::Error> {
        d.deserialize_any(AnyVisitor)
    }
}

impl<'de> serde::Deserialize<'de> for GValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(AnyVisitor)
    }
}

struct AnyVisitor;

impl<'de> Visitor<'de> for AnyVisitor {
    type Value = GValue;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a GraphSON value")
    }
    fn visit_unit<E>(self) -> Result<GValue, E> {
        Ok(GValue::Null)
    }
    fn visit_none<E>(self) -> Result<GValue, E> {
        Ok(GValue::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<GValue, E> {
        Ok(GValue::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<GValue, E> {
        Ok(GValue::Int64(v))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<GValue, E> {
        Ok(i64::try_from(v).map_or(GValue::Double(v as f64), GValue::Int64))
    }
    fn visit_f64<E>(self, v: f64) -> Result<GValue, E> {
        Ok(GValue::Double(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<GValue, E> {
        Ok(GValue::String(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<GValue, E> {
        Ok(GValue::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GValue, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(v) = seq.next_element_seed(AnyValue)? {
            items.push(v);
        }
        Ok(GValue::List(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GValue, A::Error> {
        let Some(first) = map.next_key::<std::borrow::Cow<'de, str>>()? else {
            return Ok(GValue::Map(crate::new_map()));
        };
        if first == "@type" {
            let type_name: std::borrow::Cow<'de, str> = map.next_value()?;
            let value = match map.next_key::<std::borrow::Cow<'de, str>>()? {
                Some(k) if k == "@value" => map.next_value_seed(Typed(&type_name))?,
                Some(_) => return Err(de::Error::custom("expected @value after @type")),
                None => GValue::Unknown {
                    type_name: type_name.into_owned(),
                    value: serde_json::Value::Null,
                },
            };
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            return Ok(value);
        }
        // Untyped object: a string-keyed map.
        let mut m = crate::new_map();
        m.insert(
            crate::GKey::String(first.into_owned()),
            map.next_value_seed(AnyValue)?,
        );
        while let Some(k) = map.next_key::<String>()? {
            m.insert(crate::GKey::String(k), map.next_value_seed(AnyValue)?);
        }
        Ok(GValue::Map(m))
    }
}

/// Seed for the `@value` of a known `@type`.
struct Typed<'a>(&'a str);

fn float_from<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    struct F;
    impl<'de> Visitor<'de> for F {
        type Value = f64;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a number or NaN/Infinity")
        }
        fn visit_f64<E>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            match v {
                "NaN" => Ok(f64::NAN),
                "Infinity" => Ok(f64::INFINITY),
                "-Infinity" => Ok(f64::NEG_INFINITY),
                other => other
                    .parse()
                    .map_err(|_| E::custom(format!("bad float {other:?}"))),
            }
        }
    }
    d.deserialize_any(F)
}

fn int_from<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let v = float_or_int(d)?;
    v.ok_or_else(|| de::Error::custom("expected an integer"))
}

fn float_or_int<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    struct I;
    impl<'de> Visitor<'de> for I {
        type Value = Option<i64>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an integer")
        }
        fn visit_i64<E>(self, v: i64) -> Result<Option<i64>, E> {
            Ok(Some(v))
        }
        fn visit_u64<E>(self, v: u64) -> Result<Option<i64>, E> {
            Ok(i64::try_from(v).ok())
        }
        fn visit_f64<E>(self, _: f64) -> Result<Option<i64>, E> {
            Ok(None)
        }
    }
    d.deserialize_any(I)
}

/// Number-or-string text (BigDecimal/BigInteger).
fn number_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct N;
    impl<'de> Visitor<'de> for N {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_i64<E>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_u64<E>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E>(self, v: f64) -> Result<String, E> {
            Ok(serde_json::Number::from_f64(v).map_or_else(|| v.to_string(), |n| n.to_string()))
        }
        fn visit_str<E>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
    }
    d.deserialize_any(N)
}

struct Seq<S>(S);

impl<'de, S: DeserializeSeed<'de> + Copy> DeserializeSeed<'de> for Seq<S> {
    type Value = Vec<S::Value>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        struct V<S>(S);
        impl<'de, S: DeserializeSeed<'de> + Copy> Visitor<'de> for V<S> {
            type Value = Vec<S::Value>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(v) = seq.next_element_seed(self.0)? {
                    out.push(v);
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V(self.0))
    }
}

fn string_of(v: GValue) -> String {
    match v {
        GValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

impl<'de> DeserializeSeed<'de> for Typed<'_> {
    type Value = GValue;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<GValue, D::Error> {
        let t = self.0;
        let range = |v: i64, what: &str| de::Error::custom(format!("{what} out of range: {v}"));
        Ok(match t {
            "g:Int32" => {
                let v = int_from(d)?;
                GValue::Int32(i32::try_from(v).map_err(|_| range(v, t))?)
            }
            "g:Int64" => GValue::Int64(int_from(d)?),
            "g:Float" => GValue::Float(float_from(d)? as f32),
            "g:Double" => GValue::Double(float_from(d)?),
            "gx:Byte" => {
                let v = int_from(d)?;
                GValue::Byte(i8::try_from(v).map_err(|_| range(v, t))?)
            }
            "gx:Int16" => {
                let v = int_from(d)?;
                GValue::Short(i16::try_from(v).map_err(|_| range(v, t))?)
            }
            "gx:BigDecimal" => GValue::BigDecimal(number_text(d)?),
            "gx:BigInteger" => GValue::BigInteger(number_text(d)?),
            "gx:Char" => {
                let s: String = serde::Deserialize::deserialize(d)?;
                GValue::Char(
                    s.chars()
                        .next()
                        .ok_or_else(|| de::Error::custom("empty char"))?,
                )
            }
            "g:UUID" => {
                let s: std::borrow::Cow<'de, str> = serde::Deserialize::deserialize(d)?;
                GValue::Uuid(uuid::Uuid::parse_str(&s).map_err(de::Error::custom)?)
            }
            "g:Date" => GValue::Date(int_from(d)?),
            "g:Timestamp" => GValue::Timestamp(int_from(d)?),
            "gx:Duration" | "gx:Instant" | "gx:LocalDate" | "gx:LocalDateTime" | "gx:LocalTime"
            | "gx:OffsetDateTime" | "gx:ZonedDateTime" | "gx:Period" => {
                GValue::Duration(serde::Deserialize::deserialize(d)?)
            }
            "gx:ByteBuffer" => {
                let s: String = serde::Deserialize::deserialize(d)?;
                GValue::Bytes(
                    base64::engine::general_purpose::STANDARD
                        .decode(s)
                        .map_err(de::Error::custom)?,
                )
            }
            "g:List" => GValue::List(Seq(AnyValue).deserialize(d)?),
            "g:Set" => GValue::Set(Seq(AnyValue).deserialize(d)?),
            "g:Map" => {
                let flat = Seq(AnyValue).deserialize(d)?;
                if flat.len() % 2 != 0 {
                    return Err(de::Error::custom("odd number of map entries"));
                }
                let mut m = crate::IndexMap::with_capacity_and_hasher(
                    flat.len() / 2,
                    rustc_hash::FxBuildHasher,
                );
                let mut it = flat.into_iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    m.insert(to_key(&k), v);
                }
                GValue::Map(m)
            }
            "g:BulkSet" => {
                let flat = Seq(AnyValue).deserialize(d)?;
                let mut out = Vec::with_capacity(flat.len() / 2);
                let mut it = flat.into_iter();
                while let Some(v) = it.next() {
                    let bulk = it.next().and_then(|b| b.as_i64()).unwrap_or(1);
                    out.push((v, bulk));
                }
                GValue::BulkSet(out)
            }
            "g:Vertex" => d.deserialize_map(VertexVisitor)?,
            "g:Edge" => d.deserialize_map(EdgeVisitor)?,
            "g:VertexProperty" => GValue::VertexProperty(d.deserialize_map(VertexPropertyVisitor)?),
            "g:Property" => d.deserialize_map(PropertyVisitor)?,
            "g:Path" | "g:Traverser" | "g:Tree" | "g:TraversalMetrics" | "g:Metrics" | "g:T"
            | "g:Direction" | "g:Class" | "g:Order" | "g:Scope" | "g:Column" | "g:Pop"
            | "g:Cardinality" | "g:Merge" | "g:Operator" | "g:Barrier" | "g:Pick" => {
                // Rare or small: reuse the reference decoder on a JSON value.
                let raw: serde_json::Value = serde::Deserialize::deserialize(d)?;
                crate::graphson::decode(&serde_json::json!({ "@type": t, "@value": raw }))
                    .map_err(de::Error::custom)?
            }
            other => GValue::Unknown {
                type_name: other.to_string(),
                value: serde::Deserialize::deserialize(d)?,
            },
        })
    }
}

struct VertexVisitor;

impl<'de> Visitor<'de> for VertexVisitor {
    type Value = GValue;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a g:Vertex object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GValue, A::Error> {
        let (mut id, mut label, mut properties) =
            (GValue::Null, "vertex".to_string(), crate::new_map());
        while let Some(k) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            match &*k {
                "id" => id = map.next_value_seed(AnyValue)?,
                "label" => label = string_of(map.next_value_seed(AnyValue)?),
                "properties" => properties = map.next_value_seed(VertexPropsSeed)?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(GValue::Vertex(Vertex {
            id: Box::new(id),
            label,
            properties,
        }))
    }
}

struct VertexPropsSeed;

impl<'de> DeserializeSeed<'de> for VertexPropsSeed {
    type Value = IndexMap<String, Vec<VertexProperty>>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = IndexMap<String, Vec<VertexProperty>>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("vertex properties")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = crate::new_map();
                while let Some(k) = map.next_key::<String>()? {
                    let vals: Vec<GValue> = map.next_value_seed(Seq(AnyValue))?;
                    let vps = vals
                        .into_iter()
                        .filter_map(|v| match v {
                            GValue::VertexProperty(p) => Some(p),
                            _ => None,
                        })
                        .collect();
                    out.insert(k, vps);
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}

struct VertexPropertyVisitor;

impl<'de> Visitor<'de> for VertexPropertyVisitor {
    type Value = VertexProperty;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a g:VertexProperty object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<VertexProperty, A::Error> {
        let mut vp = VertexProperty {
            id: Box::new(GValue::Null),
            label: String::new(),
            value: Box::new(GValue::Null),
            properties: crate::new_map(),
        };
        while let Some(k) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            match &*k {
                "id" => vp.id = Box::new(map.next_value_seed(AnyValue)?),
                "label" => vp.label = string_of(map.next_value_seed(AnyValue)?),
                "value" => vp.value = Box::new(map.next_value_seed(AnyValue)?),
                "properties" => {
                    if let GValue::Map(m) = map.next_value_seed(AnyValue)? {
                        for (k, v) in m {
                            if let crate::GKey::String(k) = k {
                                vp.properties.insert(k, v);
                            }
                        }
                    }
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(vp)
    }
}

struct PropertyVisitor;

impl<'de> Visitor<'de> for PropertyVisitor {
    type Value = GValue;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a g:Property object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GValue, A::Error> {
        let (mut key, mut value) = (String::new(), GValue::Null);
        while let Some(k) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            match &*k {
                "key" => key = string_of(map.next_value_seed(AnyValue)?),
                "value" => value = map.next_value_seed(AnyValue)?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(GValue::Property(Property {
            key,
            value: Box::new(value),
        }))
    }
}

struct EdgeVisitor;

impl<'de> Visitor<'de> for EdgeVisitor {
    type Value = GValue;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a g:Edge object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GValue, A::Error> {
        let mut e = Edge {
            id: Box::new(GValue::Null),
            label: String::new(),
            out_v: Box::new(GValue::Null),
            out_v_label: "vertex".into(),
            in_v: Box::new(GValue::Null),
            in_v_label: "vertex".into(),
            properties: crate::new_map(),
        };
        while let Some(k) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            match &*k {
                "id" => e.id = Box::new(map.next_value_seed(AnyValue)?),
                "label" => e.label = string_of(map.next_value_seed(AnyValue)?),
                "outV" => e.out_v = Box::new(map.next_value_seed(AnyValue)?),
                "inV" => e.in_v = Box::new(map.next_value_seed(AnyValue)?),
                "outVLabel" => e.out_v_label = string_of(map.next_value_seed(AnyValue)?),
                "inVLabel" => e.in_v_label = string_of(map.next_value_seed(AnyValue)?),
                "properties" => {
                    if let GValue::Map(m) = map.next_value_seed(AnyValue)? {
                        for (k, v) in m {
                            if let crate::GKey::String(k) = k {
                                let value = match v {
                                    GValue::Property(p) => *p.value,
                                    other => other,
                                };
                                e.properties.insert(
                                    k.clone(),
                                    Property {
                                        key: k,
                                        value: Box::new(value),
                                    },
                                );
                            }
                        }
                    }
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(GValue::Edge(e))
    }
}

// Silence unused-import warnings for types only referenced in match arms above.
#[allow(dead_code)]
fn _types(_: Path, _: T, _: Direction) {}
