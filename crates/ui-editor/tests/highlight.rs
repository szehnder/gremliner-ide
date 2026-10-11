//! The Gremlin language registered with gpui-kit's highlighter styles tokens by role.

#![allow(clippy::unwrap_used)]

use std::ops::Range;

use gpui_kit::HighlightStyle;
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use gpui_kit::component::input::{HighlightStyleResolver as _, Rope};

fn style_at(styles: &[(Range<usize>, HighlightStyle)], offset: usize) -> Option<HighlightStyle> {
    styles
        .iter()
        .find(|(r, _)| r.contains(&offset))
        .map(|(_, s)| *s)
}

#[test]
fn scenario_query_editor_highlight_a_query() {
    ui_editor::register();
    let text = "g.V().has('code','AUS')";
    let mut highlighter = SyntaxHighlighter::new(ui_editor::LANGUAGE_NAME);
    highlighter.update(None, &Rope::from(text), None);
    let theme = HighlightTheme::default_light();
    let styles = highlighter.styles(&(0..text.len()), &*theme);
    let at = |needle: &str| style_at(&styles, text.find(needle).unwrap());

    assert_eq!(
        at("g."),
        theme.style("variable.special"),
        "g is a traversal source"
    );
    assert_eq!(at("V("), theme.style("function"), "V is a step");
    assert_eq!(at("has"), theme.style("function"), "has is a step");
    assert_eq!(at("'code'"), theme.style("string"));
    assert_eq!(at("'AUS'"), theme.style("string"));
    assert_ne!(theme.style("function"), theme.style("string"));
}
