//! Gremlin value model and GraphSON v3 codec (ADR-0012).
//!
//! [`GValue`] represents every value a TinkerPop server can return: primitives with their exact numeric
//! types, collections (maps may have non-string keys), graph elements, paths, traversers and metrics.
//! [`graphson::decode`] turns a GraphSON v3 `serde_json::Value` into a `GValue`; [`graphson::encode`]
//! does the reverse for request bindings.

pub mod fast;
pub mod graphson;
pub mod plain;

use std::fmt;

/// Insertion-ordered map with a fast non-cryptographic hasher (keys are short strings and ids).
pub type IndexMap<K, V> = indexmap::IndexMap<K, V, rustc_hash::FxBuildHasher>;

/// Creates an empty [`IndexMap`].
pub fn new_map<K, V>() -> IndexMap<K, V> {
    IndexMap::with_hasher(rustc_hash::FxBuildHasher)
}

/// A Gremlin value.
#[derive(Clone, Debug, PartialEq)]
pub enum GValue {
    Null,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float(f32),
    Double(f64),
    /// `gx:BigDecimal`, kept as its exact decimal text.
    BigDecimal(String),
    /// `gx:BigInteger`, kept as its exact decimal text.
    BigInteger(String),
    Byte(i8),
    Short(i16),
    Char(char),
    String(String),
    Uuid(uuid::Uuid),
    /// `g:Date`: milliseconds since the Unix epoch.
    Date(i64),
    /// `g:Timestamp`: milliseconds since the Unix epoch.
    Timestamp(i64),
    /// `gx:Duration` and similar ISO-8601 text types.
    Duration(String),
    Bytes(Vec<u8>),
    List(Vec<GValue>),
    Set(Vec<GValue>),
    BulkSet(Vec<(GValue, i64)>),
    Map(IndexMap<GKey, GValue>),
    Vertex(Vertex),
    Edge(Edge),
    VertexProperty(VertexProperty),
    Property(Property),
    Path(Path),
    Tree(Vec<(GValue, GValue)>),
    Traverser {
        bulk: i64,
        value: Box<GValue>,
    },
    T(T),
    Direction(Direction),
    /// Other enum-like tokens (`g:Order`, `g:Scope`, `g:Column`, `g:Pop`, `g:Cardinality`, `g:Merge`, …).
    Token {
        kind: String,
        value: String,
    },
    Class(String),
    TraversalMetrics(Metrics),
    Metrics(Metrics),
    /// A typed value this client does not model; preserved verbatim.
    Unknown {
        type_name: String,
        value: serde_json::Value,
    },
}

/// Map keys: any hashable Gremlin value. Floats and maps are not valid keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GKey {
    Null,
    Bool(bool),
    Int(i64),
    String(String),
    Uuid(uuid::Uuid),
    T(T),
    Direction(Direction),
    Token {
        kind: String,
        value: String,
    },
    /// Element keys (e.g. `group().by()` on vertices) — identified by element id and label.
    Vertex {
        id: Box<GKey>,
        label: String,
    },
    Edge {
        id: Box<GKey>,
        label: String,
    },
    /// Fallback for keys that are not hashable as-is (lists, decimals, …): their GraphSON text.
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum T {
    Id,
    Label,
    Key,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Out,
    In,
    Both,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vertex {
    pub id: Box<GValue>,
    pub label: String,
    /// Present when the server includes properties (e.g. `materializeProperties=all`).
    pub properties: IndexMap<String, Vec<VertexProperty>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub id: Box<GValue>,
    pub label: String,
    pub out_v: Box<GValue>,
    pub out_v_label: String,
    pub in_v: Box<GValue>,
    pub in_v_label: String,
    pub properties: IndexMap<String, Property>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexProperty {
    pub id: Box<GValue>,
    pub label: String,
    pub value: Box<GValue>,
    /// Meta-properties.
    pub properties: IndexMap<String, GValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub key: String,
    pub value: Box<GValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub labels: Vec<Vec<String>>,
    pub objects: Vec<GValue>,
}

/// Profile output (`g:TraversalMetrics` / `g:Metrics`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Metrics {
    pub id: Option<String>,
    pub name: Option<String>,
    /// Duration in nanoseconds when reported in nanos (TinkerPop reports `dur` in ms as a double).
    pub dur_ms: f64,
    pub counts: IndexMap<String, i64>,
    pub annotations: IndexMap<String, GValue>,
    pub metrics: Vec<Metrics>,
}

impl GValue {
    /// The element id if this value is a vertex, edge or vertex property.
    pub fn element_id(&self) -> Option<&GValue> {
        match self {
            GValue::Vertex(v) => Some(&v.id),
            GValue::Edge(e) => Some(&e.id),
            GValue::VertexProperty(p) => Some(&p.id),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            GValue::Int32(v) => Some(i64::from(*v)),
            GValue::Int64(v) => Some(*v),
            GValue::Byte(v) => Some(i64::from(*v)),
            GValue::Short(v) => Some(i64::from(*v)),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            GValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Short GraphSON-style type name, e.g. `Int32`, `Vertex`, `List`.
    pub fn type_name(&self) -> &str {
        match self {
            GValue::Null => "Null",
            GValue::Bool(_) => "Boolean",
            GValue::Int32(_) => "Int32",
            GValue::Int64(_) => "Int64",
            GValue::Float(_) => "Float",
            GValue::Double(_) => "Double",
            GValue::BigDecimal(_) => "BigDecimal",
            GValue::BigInteger(_) => "BigInteger",
            GValue::Byte(_) => "Byte",
            GValue::Short(_) => "Int16",
            GValue::Char(_) => "Char",
            GValue::String(_) => "String",
            GValue::Uuid(_) => "UUID",
            GValue::Date(_) => "Date",
            GValue::Timestamp(_) => "Timestamp",
            GValue::Duration(_) => "Duration",
            GValue::Bytes(_) => "ByteBuffer",
            GValue::List(_) => "List",
            GValue::Set(_) => "Set",
            GValue::BulkSet(_) => "BulkSet",
            GValue::Map(_) => "Map",
            GValue::Vertex(_) => "Vertex",
            GValue::Edge(_) => "Edge",
            GValue::VertexProperty(_) => "VertexProperty",
            GValue::Property(_) => "Property",
            GValue::Path(_) => "Path",
            GValue::Tree(_) => "Tree",
            GValue::Traverser { .. } => "Traverser",
            GValue::T(_) => "T",
            GValue::Direction(_) => "Direction",
            GValue::Token { kind, .. } => kind,
            GValue::Class(_) => "Class",
            GValue::TraversalMetrics(_) => "TraversalMetrics",
            GValue::Metrics(_) => "Metrics",
            GValue::Unknown { type_name, .. } => type_name,
        }
    }
}

impl fmt::Display for T {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            T::Id => "id",
            T::Label => "label",
            T::Key => "key",
            T::Value => "value",
        })
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Direction::Out => "OUT",
            Direction::In => "IN",
            Direction::Both => "BOTH",
        })
    }
}
