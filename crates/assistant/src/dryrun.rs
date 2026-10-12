//! The read-only dry run (ADR-0020): mutations are never run; reads get an injected `limit` and a timeout.

use std::time::Duration;

use futures::future::BoxFuture;
use gremlin_client::{Client, Error, Request};
use gremlin_model::GValue;
use gremlin_syntax::{Catalogue, Document, StepKind};

/// Rows a dry run returns at most.
pub const DEFAULT_LIMIT: usize = 25;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Steps after which the result is already bounded (aggregates): no limit is added after them.
const REDUCING: &[&str] = &[
    "count",
    "sum",
    "mean",
    "max",
    "min",
    "fold",
    "groupCount",
    "group",
    "tree",
    "cap",
    "math",
];

/// `query` with `.limit(n)` at the end of its main chain (before a terminal step), unless the chain
/// aggregates. Text it cannot analyse is returned unchanged.
pub fn inject_limit(query: &str, n: usize) -> String {
    let trimmed = query.trim().trim_end_matches(';').trim_end();
    let doc = Document::parse(trimmed);
    let statements = doc.statements();
    let [stmt] = statements.as_slice() else {
        return trimmed.to_owned();
    };
    if stmt.has_error {
        return trimmed.to_owned();
    }
    let steps = doc.steps(stmt);
    let main: Vec<_> = steps.iter().filter(|s| s.depth == 0).collect();
    if main.iter().any(|s| REDUCING.contains(&s.name.as_str())) {
        return trimmed.to_owned();
    }
    let catalogue = Catalogue::get();
    let terminal = main.iter().rev().find(|s| {
        catalogue
            .step(&s.name)
            .is_some_and(|i| i.kind == StepKind::Terminal)
    });
    let offset = stmt.range.start;
    match (terminal, main.last()) {
        (Some(t), _) => {
            let at = t.span.start - offset;
            format!("{}limit({n}).{}", &trimmed[..at], &trimmed[at..])
        }
        (None, Some(last)) => {
            let at = last.span.end - offset;
            format!("{}.limit({n}){}", &trimmed[..at], &trimmed[at..])
        }
        (None, None) => trimmed.to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DryRunResult {
    pub items: Vec<GValue>,
    pub elapsed: Duration,
    /// The script that ran.
    pub script: String,
}

/// Runs dry-run scripts: the tab's connection in the app, a fake in tests.
pub trait DryRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        script: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<(Vec<GValue>, Duration), String>>;
}

impl DryRunner for Client {
    fn run<'a>(
        &'a self,
        script: &'a str,
        timeout: Duration,
    ) -> BoxFuture<'a, Result<(Vec<GValue>, Duration), String>> {
        Box::pin(async move {
            match self
                .execute_request(Request::new(script).timeout(timeout))
                .await
            {
                Ok(r) => Ok((r.items, r.elapsed)),
                Err(Error::Server { message, .. }) => Err(message),
                Err(Error::Timeout(t)) => Err(format!("timed out after {} s", t.as_secs())),
                Err(e) => Err(e.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_goes_at_the_end_or_before_the_terminal() {
        assert_eq!(
            inject_limit("g.V().out('route')", 25),
            "g.V().out('route').limit(25)"
        );
        assert_eq!(
            inject_limit("g.V().values('code').toList()", 5),
            "g.V().values('code').limit(5).toList()"
        );
        assert_eq!(
            inject_limit("g.V().where(out().count().is(gt(3)))", 5),
            "g.V().where(out().count().is(gt(3))).limit(5)"
        );
    }

    #[test]
    fn aggregates_and_unparseable_text_are_unchanged() {
        assert_eq!(inject_limit("g.V().count()", 25), "g.V().count()");
        assert_eq!(
            inject_limit("g.V().groupCount().by(label)", 25),
            "g.V().groupCount().by(label)"
        );
        assert_eq!(inject_limit("g.V(", 25), "g.V(");
    }
}
