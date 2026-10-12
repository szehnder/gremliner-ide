//! Reading the model's answer: the structured `{query, explanation, intent, expected_shape}`, or — from
//! models without JSON output — JSON in prose, a fenced Gremlin block, or a bare `g.` line.

use llm::ChatResponse;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::ANSWER_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    #[default]
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Graph,
    #[default]
    Table,
    Scalar,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Answer {
    pub query: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub intent: Intent,
    #[serde(default)]
    pub expected_shape: Shape,
}

impl Answer {
    /// The answer as the assistant turn kept in the conversation.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn from_value(v: &Value) -> Option<Answer> {
    let query = v.get("query")?.as_str()?.trim().to_owned();
    if query.is_empty() {
        return None;
    }
    let text = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    Some(Answer {
        query,
        explanation: v
            .get("explanation")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned(),
        intent: if text("intent") == "write" {
            Intent::Write
        } else {
            Intent::Read
        },
        expected_shape: match text("expected_shape").as_str() {
            "graph" => Shape::Graph,
            "scalar" => Shape::Scalar,
            _ => Shape::Table,
        },
    })
}

/// A query in ```gremlin fences or on a line starting with `g.`.
fn query_in_text(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let nl = after.find('\n')?;
        let lang = after[..nl].trim().to_lowercase();
        let body = &after[nl + 1..];
        let end = body.find("```").unwrap_or(body.len());
        let code = body[..end].trim();
        if (lang.is_empty() || lang == "gremlin" || lang == "groovy") && code.starts_with("g.") {
            return Some(code.to_owned());
        }
        rest = &body[end.min(body.len())..];
        if end == body.len() {
            break;
        }
        rest = &rest[3.min(rest.len())..];
    }
    // Inline code: `g.V()…`
    if let Some(start) = text.find("`g.") {
        let rest = &text[start + 1..];
        if let Some(end) = rest.find('`') {
            return Some(rest[..end].trim().to_owned());
        }
    }
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("g."))
        .map(str::to_owned)
}

/// Parse the model's reply.
pub fn parse(resp: &ChatResponse) -> Option<Answer> {
    if let Some(v) = resp.output_json(ANSWER_SCHEMA)
        && let Some(a) = from_value(&v)
    {
        return Some(a);
    }
    let query = query_in_text(&resp.text)?;
    Some(Answer {
        query,
        ..Answer::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> ChatResponse {
        ChatResponse {
            text: t.into(),
            ..ChatResponse::default()
        }
    }

    #[test]
    fn structured_json_schema_reply() {
        let a = parse(&text(
            r#"{"query":"g.V().hasLabel('airport').count()","explanation":"Counts airports.","intent":"read","expected_shape":"scalar"}"#,
        ))
        .unwrap();
        assert_eq!(a.query, "g.V().hasLabel('airport').count()");
        assert_eq!(a.expected_shape, Shape::Scalar);
    }

    #[test]
    fn forced_tool_reply() {
        let resp = ChatResponse {
            tool_calls: vec![("t1".into(), ANSWER_SCHEMA.into(), r#"{"query":"g.E().limit(3)","explanation":"x","intent":"read","expected_shape":"graph"}"#.into())],
            ..ChatResponse::default()
        };
        assert_eq!(parse(&resp).unwrap().expected_shape, Shape::Graph);
    }

    #[test]
    fn json_object_in_prose_and_fences() {
        let a = parse(&text(
            "Sure! ```json\n{\"query\": \"g.V().limit(1)\", \"intent\": \"WRITE\"}\n```",
        ))
        .unwrap();
        assert_eq!(
            (a.query.as_str(), a.intent),
            ("g.V().limit(1)", Intent::Write)
        );
    }

    #[test]
    fn free_text_fallbacks() {
        let a = parse(&text(
            "Here it is:\n```gremlin\ng.V().has('code','AUS').out('route')\n```\nIt finds routes.",
        ))
        .unwrap();
        assert_eq!(a.query, "g.V().has('code','AUS').out('route')");
        let a = parse(&text("Try `g.V().count()` for that.")).unwrap();
        assert_eq!(a.query, "g.V().count()");
        assert!(parse(&text("I cannot help with that.")).is_none());
    }
}
