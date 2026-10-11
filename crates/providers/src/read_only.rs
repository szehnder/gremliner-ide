//! Read-only connections refuse mutating queries unless the user overrides one execution (ADR-0020).
//!
//! Detection uses the language service (`gremlin_syntax`): a statement mutates if it contains a writing
//! step at any depth, an opaque Groovy closure, or a syntax error. It errs towards blocking: a false
//! positive costs one confirmation.

/// Why each statement of `script` counts as mutating: writing step names, `closure`, or `syntax error`,
/// in order of appearance and without duplicates. Empty means read-only.
pub fn mutating_steps(script: &str) -> Vec<String> {
    let doc = gremlin_syntax::Document::parse(script);
    let catalogue = gremlin_syntax::Catalogue::get();
    let mut reasons: Vec<String> = Vec::new();
    let mut push = |r: String| {
        if !reasons.contains(&r) {
            reasons.push(r);
        }
    };
    for analysis in doc.analyze_all() {
        if !analysis.is_mutating {
            continue;
        }
        if analysis.statement.has_error {
            push("syntax error".into());
        }
        let mut found_step = false;
        for step in analysis
            .steps
            .iter()
            .flat_map(gremlin_syntax::StepNode::walk)
        {
            if catalogue.step(&step.name).is_some_and(|s| s.mutating) {
                push(step.name.clone());
                found_step = true;
            }
        }
        if !found_step && !analysis.statement.has_error {
            push("closure".into());
        }
    }
    reasons
}

/// Why an execution was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{connection} is read-only and this query may change the graph ({reasons}).", reasons = .reasons.join(", "))]
pub struct ReadOnlyBlocked {
    pub connection: String,
    pub reasons: Vec<String>,
}

/// Decide whether `script` may run on a connection. `override_confirmed` is the user's explicit
/// "Run anyway" for this single execution.
pub fn check(
    connection: &str,
    read_only: bool,
    script: &str,
    override_confirmed: bool,
) -> Result<(), ReadOnlyBlocked> {
    if !read_only || override_confirmed {
        return Ok(());
    }
    let reasons = mutating_steps(script);
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(ReadOnlyBlocked {
            connection: connection.to_owned(),
            reasons,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_connections_mutation_blocked() {
        let blocked = check("Prod", true, "g.addV('person')", false).unwrap_err();
        assert_eq!(blocked.reasons, ["addV"]);
        assert_eq!(
            blocked.to_string(),
            "Prod is read-only and this query may change the graph (addV)."
        );
        // The explicit, confirmed override runs it once.
        assert!(check("Prod", true, "g.addV('person')", true).is_ok());
        // Writable connections and reads are unaffected.
        assert!(check("Prod", false, "g.addV('person')", false).is_ok());
        assert!(
            check(
                "Prod",
                true,
                "g.V().has('name','addV(').values('property')",
                false
            )
            .is_ok()
        );
    }

    #[test]
    fn detects_mutations_closures_and_errors() {
        assert_eq!(
            mutating_steps("g.V(1).property('age', 3).as('a').V(2).drop()"),
            ["property", "drop"]
        );
        assert_eq!(mutating_steps("g.mergeV([name:'x'])"), ["mergeV"]);
        assert_eq!(
            mutating_steps("g.V().coalesce(unfold(), addV('p'))"),
            ["addV"]
        );
        assert!(mutating_steps("g.V().properties()").is_empty());
        assert!(mutating_steps("g.V().hasLabel(\"drop()\")").is_empty());
        assert_eq!(
            mutating_steps("g.V().filter{ it.get().remove() }"),
            ["closure"]
        );
        assert_eq!(mutating_steps("g.V().has('x'"), ["syntax error"]);
    }
}
