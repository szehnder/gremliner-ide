//! The Gremlin language service (ADR-0014): a tree-sitter grammar plus the analyses every feature reuses:
//! diagnostics, statements, step trees, mutation and parameter detection, step prefixes for the
//! debugger, formatting and step documentation.

pub mod catalogue;
pub mod complete;
pub mod dialect;
pub mod document;
pub mod explorer;
pub mod format;
pub mod language;
pub mod profile_match;

pub use catalogue::{Catalogue, StepInfo, StepKind, documentation};
pub use document::{
    Diagnostic, Document, Severity, Statement, StatementAnalysis, StepNode, line_col,
};
pub use format::format;
pub use language::{HIGHLIGHTS_QUERY, LANGUAGE, language};

/// Analyse every statement of `text`.
pub fn analyze(text: &str) -> Vec<StatementAnalysis> {
    Document::parse(text).analyze_all()
}

/// True if any statement in `text` mutates the graph (or cannot be fully analysed). Use this for
/// read-only enforcement (ADR-0020).
pub fn is_mutating(text: &str) -> bool {
    let analyses = analyze(text);
    analyses.is_empty() && !text.trim().is_empty() || analyses.iter().any(|a| a.is_mutating)
}

/// The script to send for a statement: Gremlin traversals without a terminal step get `.toList()`
/// appended; everything else (explicit terminals, assignments, Groovy) is sent unchanged.
pub fn ensure_terminal(statement: &str) -> String {
    let trimmed = statement.trim().trim_end_matches(';').trim_end();
    let doc = Document::parse(trimmed);
    let statements = doc.statements();
    let [only] = statements.as_slice() else {
        return trimmed.to_owned();
    };
    if only.has_error {
        return trimmed.to_owned();
    }
    let steps = doc.steps(only);
    let Some(last) = steps.last() else {
        return trimmed.to_owned();
    };
    // Only top-level Gremlin chains rooted at `g` (not assignments or variables).
    let tree = doc.tree().root_node();
    let is_traversal = tree.named_child(0).is_some_and(|n| n.kind() == "traversal");
    if !is_traversal || last.chain_root != "g" {
        return trimmed.to_owned();
    }
    let terminal = Catalogue::get()
        .step(&last.name)
        .is_some_and(|s| s.kind == StepKind::Terminal);
    if terminal {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.toList()")
    }
}

#[cfg(test)]
mod terminal_tests {
    use super::ensure_terminal;

    #[test]
    fn scenario_query_execution_append_tolist() {
        assert_eq!(ensure_terminal("g.V().limit(2)"), "g.V().limit(2).toList()");
        assert_eq!(
            ensure_terminal("  g.V().\n  out()  ;"),
            "g.V().\n  out().toList()"
        );
    }

    #[test]
    fn scenario_query_execution_keep_explicit_terminal() {
        assert_eq!(
            ensure_terminal("g.V().count().next()"),
            "g.V().count().next()"
        );
        for s in [
            "g.V().iterate()",
            "g.V().out().explain()",
            "x = g.V().next()",
            "graph.features()",
            "g.V(",
        ] {
            assert_eq!(ensure_terminal(s), s);
        }
    }
}
