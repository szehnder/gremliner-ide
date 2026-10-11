//! Hover documentation for steps, from the gremlin-syntax catalogue.

use gpui_kit::component::input::{HoverProvider, Rope, RopeExt as _};
use gpui_kit::{App, Task, Window};
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

pub struct StepHover;

/// The identifier at `offset`, if any.
pub fn word_at(text: &str, offset: usize) -> Option<(usize, usize)> {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if offset > text.len() || !text.is_char_boundary(offset) {
        return None;
    }
    let start = text[..offset]
        .rfind(|c: char| !is_ident(c))
        .map_or(0, |i| i + 1);
    let end = text[offset..]
        .find(|c: char| !is_ident(c))
        .map_or(text.len(), |i| offset + i);
    (start < end).then_some((start, end))
}

/// Markdown for a step, or `None` if the word is not a step call.
pub fn step_markdown(text: &str, offset: usize) -> Option<String> {
    let (start, end) = word_at(text, offset)?;
    // Only a call (`name(` or `name{`) is a step.
    let after = text[end..].trim_start();
    if !(after.starts_with('(') || after.starts_with('{')) {
        return None;
    }
    let info = gremlin_syntax::documentation(&text[start..end])?;
    let signatures = info
        .signatures
        .iter()
        .map(|s| format!("    {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(
        "**{}()** step\n\n{}\n\n{signatures}\n\n[Reference]({})",
        info.name,
        info.doc,
        info.link()
    ))
}

impl HoverProvider for StepHover {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Option<Hover>>> {
        let source = text.to_string();
        let hover = step_markdown(&source, offset).map(|value| {
            let range = word_at(&source, offset).map(|(s, e)| lsp_types::Range {
                start: text.offset_to_position(s),
                end: text.offset_to_position(e),
            });
            Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range,
            }
        });
        Task::ready(Ok(hover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_query_editor_hover_a_step() {
        let text = "g.V(1).repeat(out()).times(2)";
        let md = step_markdown(text, text.find("repeat").unwrap() + 2).unwrap();
        assert!(md.starts_with("**repeat()** step"));
        assert!(md.contains("Loops over the traversal"));
        assert!(md.contains("repeat(Traversal repeatTraversal)"));
        assert!(md.contains("#repeat-step"));
        // Strings and non-call words have no hover.
        let text = "g.V().has('repeat')";
        assert!(step_markdown(text, text.find("repeat").unwrap()).is_none());
    }
}
