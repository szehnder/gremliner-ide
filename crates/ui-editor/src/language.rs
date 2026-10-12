//! Registers tree-sitter-gremlin with gpui-kit's highlighter under the name [`LANGUAGE_NAME`].

use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

pub const LANGUAGE_NAME: &str = "gremlin";

/// Idempotent; call once at startup (and in tests) before creating editors.
pub fn register() {
    let config = LanguageConfig::new(
        LANGUAGE_NAME,
        gremlin_syntax::language(),
        vec![],
        gremlin_syntax::HIGHLIGHTS_QUERY,
        "",
        "",
    );
    LanguageRegistry::singleton().register(LANGUAGE_NAME, &config);
}

/// Highlight ranges for `text` as Gremlin in the current theme (read-only snippets such as the
/// assistant's result card). Build a `StyledText` with `with_highlights`; cache the result, it parses.
pub fn highlight_styles(
    text: &str,
    cx: &gpui_kit::App,
) -> Vec<(std::ops::Range<usize>, gpui_kit::HighlightStyle)> {
    use gpui_kit::component::ActiveTheme as _;
    use gpui_kit::component::highlighter::SyntaxHighlighter;
    use gpui_kit::component::input::Rope;
    let mut highlighter = SyntaxHighlighter::new(LANGUAGE_NAME);
    highlighter.update(None, &Rope::from(text), None);
    let theme = cx.theme().highlight_theme.clone();
    highlighter.styles(&(0..text.len()), &*theme)
}
