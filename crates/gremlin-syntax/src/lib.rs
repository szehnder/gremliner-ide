//! The Gremlin language service (ADR-0014): a tree-sitter grammar plus the analyses every feature reuses:
//! diagnostics, statements, step trees, mutation and parameter detection, step prefixes for the
//! debugger, formatting and step documentation.

pub mod catalogue;
pub mod document;
pub mod format;
pub mod language;

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
