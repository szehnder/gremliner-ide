//! The model's context (ADR-0019, ADR-0021): system prompt with Gremlin rules and the dialect, the schema
//! in a compact typed form (sample values only when sharing is allowed), the editor's query and the
//! conversation. Pure functions, snapshot-tested per privacy mode.

use std::sync::Arc;

use llm::{ChatRequest, Message, OutputSchema};
use schema::{Cardinality, LabelInfo, Schema};
use serde_json::json;

/// What the connected database speaks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialect {
    /// e.g. `3.7.3`.
    pub tinkerpop_version: Option<String>,
    pub neptune: bool,
}

impl Dialect {
    /// Steps the database rejects (beyond lambdas, which are always refused).
    pub fn unsupported_steps(&self) -> &'static [&'static str] {
        if self.neptune {
            &[
                "io",
                "withComputer",
                "subgraph",
                "pageRank",
                "peerPressure",
                "connectedComponent",
                "shortestPath",
                "program",
                "call",
            ]
        } else {
            &[]
        }
    }

    fn describe(&self) -> String {
        let version = self
            .tinkerpop_version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        if self.neptune {
            format!("Amazon Neptune (Apache TinkerPop{version} compatible)")
        } else {
            format!("Apache TinkerPop{version} Gremlin Server")
        }
    }
}

/// Everything known about where the request is made.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub schema: Option<Arc<Schema>>,
    pub dialect: Dialect,
    /// The query in the editor, for refinements that mention "this query".
    pub current_query: Option<String>,
    /// Sample values and dry-run rows may be sent to the model (ADR-0021).
    pub share_samples: bool,
}

/// The name of the structured-output schema.
pub const ANSWER_SCHEMA: &str = "gremlin_answer";

pub fn answer_schema() -> OutputSchema {
    OutputSchema {
        name: ANSWER_SCHEMA.into(),
        schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "One Gremlin traversal starting with g."},
                "explanation": {"type": "string", "description": "One or two sentences on what it returns."},
                "intent": {"type": "string", "enum": ["read", "write"]},
                "expected_shape": {"type": "string", "enum": ["graph", "table", "scalar"]},
            },
            "required": ["query", "explanation", "intent", "expected_shape"],
            "additionalProperties": false,
        }),
    }
}

fn label_line(l: &LabelInfo, share_samples: bool) -> String {
    let count = l
        .count
        .map(|c| format!(" ~{}", c.text()))
        .unwrap_or_default();
    let props: Vec<String> = l
        .properties
        .iter()
        .map(|p| {
            let mut s = format!("{}:{}", p.key, p.type_text());
            if p.cardinality == Cardinality::Multi {
                s.push_str("[multi]");
            }
            if share_samples && !p.samples.is_empty() {
                let samples: Vec<String> = p
                    .samples
                    .iter()
                    .take(3)
                    .map(|v| {
                        if p.primary_type() == Some("String") {
                            format!("'{v}'")
                        } else {
                            v.clone()
                        }
                    })
                    .collect();
                s.push_str(&format!(" e.g. {}", samples.join(", ")));
            }
            s
        })
        .collect();
    if props.is_empty() {
        format!("  {}{count}", l.name)
    } else {
        format!("  {}{count} ({})", l.name, props.join(", "))
    }
}

/// The schema as compact text. Without `share_samples`, only structure: labels, keys, types, counts.
pub fn compact_schema(schema: &Schema, share_samples: bool) -> String {
    let mut out = String::new();
    out.push_str("Vertex labels (approximate counts, property:type):\n");
    for l in &schema.vertex_labels {
        out.push_str(&label_line(l, share_samples));
        out.push('\n');
    }
    out.push_str("Edge labels:\n");
    for l in &schema.edge_labels {
        out.push_str(&label_line(l, share_samples));
        out.push('\n');
    }
    out.push_str("Connections (out label -edge label-> in label):\n");
    for t in &schema.triples {
        out.push_str(&format!(
            "  {} -{}-> {}\n",
            t.out_label, t.label, t.in_label
        ));
    }
    if schema.partial.any() {
        out.push_str("(The schema was sampled; rare labels or keys may be missing.)\n");
    }
    out
}

const RULES: &str = "\
Rules:
- Use only the labels, property keys and edge directions in the schema. An edge `a -e-> b` is traversed from a with out('e') and from b with in('e').
- Write exactly one traversal starting with g. No variables, no Groovy, no lambdas or closures, no terminal step such as toList().
- Prefer has(label, key, value) to find vertices by label and property.
- Bound element results with limit(...) unless the request asks for an aggregate (count, sum, mean, groupCount, ...).
- For tabular answers use project(...).by(...) or elementMap(...); for requests about connections or routes return vertices, edges or path().
- Quote strings with single quotes; write numbers for numeric properties.
- If the request changes data, write the mutation and set intent to \"write\"; otherwise intent is \"read\".
- expected_shape is \"graph\" for vertices, edges or paths, \"table\" for maps or rows, \"scalar\" for a single number or value.

Answer with one JSON object and nothing else:
{\"query\": \"...\", \"explanation\": \"...\", \"intent\": \"read\", \"expected_shape\": \"table\"}

Example (for a graph with person -knows-> person):
Request: who does marko know, oldest first
{\"query\": \"g.V().has('person', 'name', 'marko').out('knows').order().by('age', desc).values('name')\", \"explanation\": \"Names of the people marko knows, oldest first.\", \"intent\": \"read\", \"expected_shape\": \"table\"}";

/// The system prompt.
pub fn system_prompt(ctx: &Context) -> String {
    let mut s = format!(
        "You translate requests about a graph database into one Gremlin traversal for {}.\n\n{RULES}\n",
        ctx.dialect.describe()
    );
    let unsupported = ctx.dialect.unsupported_steps();
    if !unsupported.is_empty() {
        s.push_str(&format!(
            "\nThis database does not support these steps: {}.\n",
            unsupported.join(", ")
        ));
    }
    match &ctx.schema {
        Some(schema) => {
            s.push_str("\nSchema of the connected graph:\n");
            s.push_str(&compact_schema(schema, ctx.share_samples));
        }
        None => s.push_str("\nThe schema of the connected graph is not known yet.\n"),
    }
    if let Some(q) = ctx
        .current_query
        .as_deref()
        .filter(|q| !q.trim().is_empty())
    {
        s.push_str(&format!("\nThe editor currently contains:\n{}\n", q.trim()));
    }
    s
}

/// The request for `prompt` after `history` (earlier prompts and answers, for refinements).
pub fn build_request(ctx: &Context, history: &[Message], prompt: &str) -> ChatRequest {
    let mut messages = history.to_vec();
    messages.push(Message::user(prompt));
    ChatRequest {
        system: Some(system_prompt(ctx)),
        messages,
        output_schema: Some(answer_schema()),
        temperature: Some(0.1),
        ..ChatRequest::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn air() -> Arc<Schema> {
        Arc::new(
            Schema::from_json(include_str!(
                "../../gremlin-syntax/tests/fixtures/air-routes-schema.json"
            ))
            .unwrap_or_default(),
        )
    }

    fn ctx(share: bool) -> Context {
        Context {
            schema: Some(air()),
            dialect: Dialect {
                tinkerpop_version: Some("3.7.3".into()),
                neptune: false,
            },
            current_query: None,
            share_samples: share,
        }
    }

    #[test]
    fn prompt_without_samples() {
        insta::assert_snapshot!(system_prompt(&ctx(false)));
    }

    #[test]
    fn prompt_with_samples_on_neptune() {
        let mut c = ctx(true);
        c.dialect.neptune = true;
        c.current_query = Some("g.V().hasLabel('airport').limit(10)".into());
        insta::assert_snapshot!(system_prompt(&c));
    }

    #[test]
    fn samples_only_when_shared() {
        let schema = air();
        let sample = &schema
            .vertex_label("airport")
            .unwrap()
            .property("code")
            .unwrap()
            .samples[0];
        assert!(!system_prompt(&ctx(false)).contains(sample.as_str()));
        assert!(system_prompt(&ctx(true)).contains(sample.as_str()));
    }
}
