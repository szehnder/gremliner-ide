//! The tree-sitter language and its highlight query.

use tree_sitter_language::LanguageFn;

// The only unsafe code in the crate: binding the generated C parser (ADR-0014).
#[allow(unsafe_code)]
unsafe extern "C" {
    fn tree_sitter_gremlin() -> *const ();
}

/// The Gremlin tree-sitter language, for sharing with editors (gpui-kit's highlighter).
// SAFETY: `tree_sitter_gremlin` is the generated, checked-in parser's entry point; it takes no arguments
// and returns a pointer to static language data, which is exactly what `LanguageFn` expects.
#[allow(unsafe_code)]
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_gremlin) };

/// Syntax highlighting captures (`@function.method`, `@string`, ...).
pub const HIGHLIGHTS_QUERY: &str = include_str!("../grammar/queries/highlights.scm");

pub fn language() -> tree_sitter::Language {
    LANGUAGE.into()
}
