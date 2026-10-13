//! Query parameters (query-parameters, ADR-0030): which free identifiers a statement uses, how a typed
//! value is read from the parameters panel, and the type the schema suggests for each.

use gremlin_model::{GKey, GValue, IndexMap};
use schema::Schema;

use crate::Document;

/// Parameters in `text`, across statements, in order of first appearance.
pub fn parameters(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for analysis in Document::parse(text).analyze_all() {
        for p in analysis.parameters {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// What the schema says a parameter holds, when it is compared with a property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Int32,
    Int64,
    Double,
    Text,
    Boolean,
}

impl Hint {
    pub fn label(self) -> &'static str {
        match self {
            Hint::Int32 | Hint::Int64 => "integer",
            Hint::Double => "number",
            Hint::Text => "text",
            Hint::Boolean => "true/false",
        }
    }
}

/// Split call arguments at top-level commas: `'airport','code',code` → three parts.
fn split_args(args: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut quote) = (0i32, 0usize, None::<char>);
    let mut chars = args.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(' | '[' | '{') => depth += 1,
            (None, ')' | ']' | '}') => depth -= 1,
            (None, ',') if depth == 0 => {
                out.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(args[start..].trim());
    out
}

fn unquote(s: &str) -> Option<&str> {
    let s = s.trim();
    s.strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
}

/// The type of property `key` (on `label` when given), from the schema's samples.
fn property_hint(schema: &Schema, label: Option<&str>, key: &str) -> Option<Hint> {
    let ty = schema
        .vertex_labels
        .iter()
        .chain(&schema.edge_labels)
        .filter(|l| label.is_none_or(|want| l.name == want))
        .find_map(|l| l.properties.iter().find(|p| p.key == key))?
        .types
        .first()?
        .clone();
    Some(match ty.as_str() {
        "Int32" | "Short" | "Byte" => Hint::Int32,
        "Int64" | "BigInteger" => Hint::Int64,
        "Double" | "Float" | "BigDecimal" => Hint::Double,
        "Boolean" => Hint::Boolean,
        _ => Hint::Text,
    })
}

/// The type the schema suggests for `param`: from `has(key, param)`, `has(label, key, param)` and
/// predicates wrapping it (`has('runways', gt(n))`).
pub fn hint(text: &str, param: &str, schema: Option<&Schema>) -> Option<Hint> {
    let schema = schema?;
    let doc = Document::parse(text);
    for statement in doc.statements() {
        for step in doc.steps(&statement).iter().flat_map(|s| s.walk()) {
            if step.name != "has" {
                continue;
            }
            let call = &text[step.span.clone()];
            let Some(open) = call.find('(') else { continue };
            let args = split_args(
                call[open + 1..]
                    .strip_suffix(')')
                    .unwrap_or(&call[open + 1..]),
            );
            let (label, key, value) = match args.as_slice() {
                [key, value] => (None, *key, *value),
                [label, key, value] => (unquote(label), *key, *value),
                _ => continue,
            };
            let mentions = value
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| w == param);
            if mentions && let Some(key) = unquote(key) {
                return property_hint(schema, label, key);
            }
        }
    }
    None
}

fn from_json(v: serde_json::Value, hint: Option<Hint>) -> GValue {
    match v {
        serde_json::Value::Null => GValue::Null,
        serde_json::Value::Bool(b) => GValue::Bool(b),
        serde_json::Value::Number(n) => match (n.as_i64(), hint) {
            (Some(i), Some(Hint::Int32)) if i32::try_from(i).is_ok() => GValue::Int32(i as i32),
            (Some(i), Some(Hint::Double)) => GValue::Double(i as f64),
            (Some(i), _) => GValue::Int64(i),
            (None, _) => GValue::Double(n.as_f64().unwrap_or(f64::NAN)),
        },
        serde_json::Value::String(s) => GValue::String(s),
        serde_json::Value::Array(items) => {
            GValue::List(items.into_iter().map(|v| from_json(v, hint)).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = IndexMap::default();
            for (k, v) in map {
                out.insert(GKey::String(k), from_json(v, hint));
            }
            GValue::Map(out)
        }
    }
}

/// Read a value as typed in the parameters panel (ADR-0030): JSON first (`10`, `true`, `null`,
/// `"AUS"`, `[1, 2]`), otherwise the text itself as a string. Integers become `Int64` unless the
/// schema says the compared property is `Int32`.
pub fn parse_value(input: &str, hint: Option<Hint>) -> GValue {
    let trimmed = input.trim();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => from_json(v, hint),
        Err(_) => GValue::String(trimmed.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn detection_in_order() {
        assert_eq!(
            parameters("g.V().has('airport','code',code).out().limit(n)"),
            ["code", "n"]
        );
        assert!(parameters("g.V().has('code','AUS')").is_empty());
    }

    #[test]
    fn args_split_at_top_level() {
        assert_eq!(
            split_args("'a', within('x','y'), n"),
            ["'a'", "within('x','y')", "n"]
        );
        assert_eq!(split_args("'a,b'"), ["'a,b'"]);
    }
}
