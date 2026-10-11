//! Read-only connections refuse mutating queries unless the user overrides one execution.
//!
//! Until `gremlin-language` provides `analyze().is_mutating`, detection is a conservative keyword scan
//! that ignores string literals. It errs towards blocking: a false positive costs one confirmation.

/// Steps and calls that change the graph.
const MUTATING: &[&str] = &[
    "addV", "addE", "property", "drop", "mergeV", "mergeE", "io", "tx",
];

/// The mutating steps found in `script`, in order of first appearance, without duplicates.
pub fn mutating_steps(script: &str) -> Vec<&'static str> {
    let code = strip_strings(script);
    let bytes = code.as_bytes();
    let mut found: Vec<(usize, &'static str)> = Vec::new();
    for step in MUTATING {
        let mut from = 0;
        while let Some(pos) = code[from..].find(step) {
            let at = from + pos;
            let end = at + step.len();
            let before_ok = at == 0 || !is_ident(bytes[at - 1]);
            let after = code[end..].trim_start();
            if before_ok && after.starts_with('(') {
                found.push((at, step));
                break;
            }
            from = end;
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, s)| s).collect()
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Replace the contents of '...' and "..." literals with spaces (keeping offsets).
fn strip_strings(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in script.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                    out.push(ch);
                    continue;
                }
                out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
            }
            None => {
                if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                }
                out.push(ch);
            }
        }
    }
    out
}

/// Why an execution was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{connection} is read-only and this query changes the graph ({steps}).", steps = .steps.join(", "))]
pub struct ReadOnlyBlocked {
    pub connection: String,
    pub steps: Vec<&'static str>,
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
    let steps = mutating_steps(script);
    if steps.is_empty() {
        Ok(())
    } else {
        Err(ReadOnlyBlocked {
            connection: connection.to_owned(),
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_connections_mutation_blocked() {
        let blocked = check("Prod", true, "g.addV('person')", false).unwrap_err();
        assert_eq!(blocked.steps, ["addV"]);
        assert_eq!(
            blocked.to_string(),
            "Prod is read-only and this query changes the graph (addV)."
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
    fn detects_mutations_and_ignores_lookalikes() {
        assert_eq!(
            mutating_steps("g.V(1).property('age', 3).as('a').V(2).drop()"),
            ["property", "drop"]
        );
        assert_eq!(mutating_steps("g.mergeV([name:'x'])"), ["mergeV"]);
        assert_eq!(mutating_steps("g.V().properties()"), Vec::<&str>::new());
        assert_eq!(
            mutating_steps("g.V().hasLabel(\"drop()\")"),
            Vec::<&str>::new()
        );
        assert_eq!(mutating_steps("g.V().myaddV ()"), Vec::<&str>::new());
        assert_eq!(mutating_steps("g.addE ('knows')"), ["addE"]);
    }
}
