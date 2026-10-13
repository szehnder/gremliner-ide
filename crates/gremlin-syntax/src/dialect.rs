//! What a particular database refuses: lambdas and unsupported steps (Neptune, older TinkerPop servers),
//! from the table in `compliance.toml`. The editor and the assistant report them as warnings.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::document::{Diagnostic, Document, Severity};

/// One database's Gremlin restrictions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Restrictions {
    /// Shown in messages, e.g. "Neptune".
    pub database: String,
    /// False when closures such as `{ it.get() }` are rejected.
    pub lambdas: bool,
    /// Step names the database rejects, with the reason shown to the user.
    pub unsupported_steps: Vec<(String, String)>,
}

impl Restrictions {
    /// No restrictions (lambdas allowed, every step supported).
    pub fn none() -> Self {
        Self {
            lambdas: true,
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lambdas && self.unsupported_steps.is_empty()
    }

    /// Why `step` is rejected, when it is.
    pub fn unsupported(&self, step: &str) -> Option<&str> {
        self.unsupported_steps
            .iter()
            .find(|(s, _)| s == step)
            .map(|(_, why)| why.as_str())
    }
}

#[derive(Deserialize)]
struct Table {
    neptune: Database,
    added: Vec<Added>,
}

#[derive(Deserialize)]
struct Database {
    lambdas: bool,
    unsupported: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Added {
    since: String,
    steps: Vec<String>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    toml::from_str(include_str!("compliance.toml")).expect("compliance.toml parses")
});

/// `3.6.2`, `tinkerpop-3.6.2` or `3.7.3-SNAPSHOT` → (3, 6, 2).
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let v = v.trim().trim_start_matches("tinkerpop-");
    let mut parts = v.split(['.', '-']).map(|p| p.parse::<u32>());
    let major = parts.next()?.ok()?;
    let minor = parts.next()?.ok()?;
    let patch = parts.next().and_then(Result::ok).unwrap_or(0);
    Some((major, minor, patch))
}

/// Steps the given TinkerPop version predates. Unknown versions are given the benefit of the doubt.
fn too_new(tinkerpop: Option<&str>) -> Vec<(String, String)> {
    let Some(version) = tinkerpop.and_then(parse_version) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for added in &TABLE.added {
        if parse_version(&added.since).is_some_and(|since| version < since) {
            for step in &added.steps {
                out.push((
                    step.clone(),
                    format!(
                        "{step}() needs TinkerPop {} or later; this server runs {}.",
                        added.since,
                        tinkerpop
                            .unwrap_or_default()
                            .trim_start_matches("tinkerpop-")
                    ),
                ));
            }
        }
    }
    out
}

/// Neptune's restrictions at a TinkerPop version.
pub fn neptune(tinkerpop: Option<&str>) -> Restrictions {
    let mut unsupported: Vec<(String, String)> = TABLE
        .neptune
        .unsupported
        .iter()
        .map(|(s, why)| (s.clone(), why.clone()))
        .collect();
    for (step, why) in too_new(tinkerpop) {
        if !unsupported.iter().any(|(s, _)| *s == step) {
            unsupported.push((step, why));
        }
    }
    Restrictions {
        database: "Neptune".into(),
        lambdas: TABLE.neptune.lambdas,
        unsupported_steps: unsupported,
    }
}

/// A TinkerPop Gremlin Server's restrictions: only steps newer than its version.
pub fn gremlin_server(tinkerpop: Option<&str>) -> Restrictions {
    Restrictions {
        database: "Gremlin Server".into(),
        lambdas: true,
        unsupported_steps: too_new(tinkerpop),
    }
}

/// Warnings for every lambda and unsupported step in the document.
pub fn restriction_issues(doc: &Document, restrictions: &Restrictions) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if restrictions.is_empty() {
        return out;
    }
    if !restrictions.lambdas {
        for range in doc.lambda_ranges() {
            out.push(Diagnostic {
                severity: Severity::Warning,
                range,
                message: format!(
                    "{} does not support lambdas; use Gremlin steps instead.",
                    restrictions.database
                ),
            });
        }
    }
    for statement in doc.statements() {
        for step in doc.steps(&statement).iter().flat_map(|s| s.walk()) {
            if !step.is_gremlin() {
                continue;
            }
            if let Some(why) = restrictions.unsupported(&step.name) {
                out.push(Diagnostic {
                    severity: Severity::Warning,
                    range: step.name_span.clone(),
                    message: why.to_owned(),
                });
            }
        }
    }
    out.sort_by_key(|d| d.range.start);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("tinkerpop-3.6.2"), Some((3, 6, 2)));
        assert_eq!(parse_version("3.7.3-SNAPSHOT"), Some((3, 7, 3)));
        assert_eq!(parse_version("3.4"), Some((3, 4, 0)));
        assert_eq!(parse_version("x"), None);
    }

    #[test]
    fn steps_newer_than_the_server_are_flagged() {
        let old = neptune(Some("3.5.2"));
        assert!(!old.lambdas);
        assert!(old.unsupported("mergeV").unwrap().contains("3.6.0"));
        assert!(old.unsupported("io").is_some());
        let new = neptune(Some("3.7.1"));
        assert!(new.unsupported("mergeV").is_none() && new.unsupported("asString").is_none());
        assert!(gremlin_server(Some("3.7.3")).is_empty());
        assert!(gremlin_server(None).is_empty());
        assert!(
            gremlin_server(Some("3.6.0"))
                .unsupported("concat")
                .is_some()
        );
    }

    fn restricted() -> Restrictions {
        Restrictions {
            database: "Neptune".into(),
            lambdas: false,
            unsupported_steps: vec![("io".into(), "Neptune does not support io().".into())],
        }
    }

    #[test]
    fn lambdas_and_steps_are_flagged() {
        let text = "g.V().map{ it.get().value('code') }\ng.io('x.xml').read()";
        let doc = Document::parse(text);
        let issues = restriction_issues(&doc, &restricted());
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues[0].message.contains("does not support lambdas"));
        assert!(text[issues[0].range.clone()].starts_with('{'));
        assert_eq!(&text[issues[1].range.clone()], "io");
        assert!(restriction_issues(&doc, &Restrictions::none()).is_empty());
    }
}
