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
