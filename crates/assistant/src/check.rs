//! Static validation of a generated query (ADR-0019 step 3): syntax, one traversal from `g`, schema labels
//! and keys, edge directions, lambdas and steps the database does not support.

use gremlin_syntax::complete::{direction_issues, lambdas, schema_issues};
use gremlin_syntax::{Catalogue, Document, Severity};
use schema::Schema;
use serde::Serialize;

use crate::context::Dialect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Syntax,
    Schema,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub message: String,
}

impl Issue {
    fn new(kind: IssueKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Everything wrong with `query`; empty when it may be dry-run.
pub fn check(query: &str, schema: Option<&Schema>, dialect: &Dialect) -> Vec<Issue> {
    let mut out = Vec::new();
    let doc = Document::parse(query);
    for d in doc.diagnostics() {
        match d.severity {
            Severity::Error => out.push(Issue::new(IssueKind::Syntax, d.message)),
            Severity::Warning if d.message.starts_with("Unknown step") => {
                out.push(Issue::new(IssueKind::Unsupported, d.message));
            }
            Severity::Warning => {}
        }
    }
    let statements = doc.statements();
    if statements.len() > 1 {
        out.push(Issue::new(
            IssueKind::Syntax,
            "Return a single traversal, not several statements.",
        ));
    }
    if !query.trim_start().starts_with("g.") {
        out.push(Issue::new(
            IssueKind::Syntax,
            "The traversal must start with g.",
        ));
    }
    if !lambdas(query).is_empty() {
        out.push(Issue::new(
            IssueKind::Unsupported,
            "Lambdas/closures are not allowed; use Gremlin steps instead.",
        ));
    }
    let restrictions = dialect.restrictions();
    if !restrictions.unsupported_steps.is_empty()
        && let Some(stmt) = statements.first()
    {
        for step in doc.steps(stmt).iter().flat_map(|s| s.walk()) {
            if let Some(why) = restrictions.unsupported(&step.name) {
                out.push(Issue::new(IssueKind::Unsupported, why.to_owned()));
            }
        }
    }
    if let Some(schema) = schema {
        for i in schema_issues(query, schema)
            .into_iter()
            .chain(direction_issues(query, schema))
        {
            out.push(Issue::new(IssueKind::Schema, i.message));
        }
    }
    let _ = Catalogue::get();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn air() -> Schema {
        Schema::from_json(include_str!(
            "../../gremlin-syntax/tests/fixtures/air-routes-schema.json"
        ))
        .unwrap_or_default()
    }

    #[test]
    fn scenario_nl_to_gremlin_schema_mismatch_detected() {
        let issues = check(
            "g.V().hasLabel('airports').values('code')",
            Some(&air()),
            &Dialect::default(),
        );
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].kind, IssueKind::Schema);
        assert!(
            issues[0]
                .message
                .contains("Unknown vertex label `airports`"),
            "{}",
            issues[0].message
        );
        assert!(issues[0].message.contains("`airport`"));
    }

    #[test]
    fn syntax_direction_lambdas_and_dialect() {
        let d = Dialect::default();
        let s = air();
        assert!(
            check(
                "g.V().has('airport','code','AUS').out('route').values('city')",
                Some(&s),
                &d
            )
            .is_empty()
        );
        assert_eq!(
            check("g.V().has('code'", Some(&s), &d)[0].kind,
            IssueKind::Syntax
        );
        assert!(
            check("g.V().hasLabel('airport').out('contains')", Some(&s), &d)[0]
                .message
                .contains("in('contains')")
        );
        assert_eq!(
            check("g.V().map{ it.get() }", None, &d)[0].kind,
            IssueKind::Unsupported
        );
        assert!(
            check("g.V().count(); g.E().count()", None, &d)
                .iter()
                .any(|i| i.message.contains("single traversal"))
        );
        let neptune = Dialect {
            neptune: true,
            ..Dialect::default()
        };
        assert!(
            check("g.V().pageRank()", None, &neptune)
                .iter()
                .any(|i| i.message.contains("pageRank"))
        );
        assert!(
            check("g.V().frobnicate()", None, &d)
                .iter()
                .any(|i| i.kind == IssueKind::Unsupported)
        );
    }
}
