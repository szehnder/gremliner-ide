//! The editor side of autocomplete: a completion provider over `gremlin_syntax::complete`, and quick
//! fixes for schema warnings. Both read the connection's schema through shared state the
//! [`QueryEditor`](crate::QueryEditor) keeps current.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::input::{
    CodeActionProvider, CompletionProvider, EditorState, Rope, RopeExt as _,
};
use gpui_kit::{App, Entity, SharedString, Task, Window};
use gremlin_syntax::complete::{Item, ItemKind, SchemaIssue, complete};
use lsp_types::{
    CodeAction, CodeActionKind, CompletionContext, CompletionItem, CompletionItemKind,
    CompletionResponse, CompletionTextEdit, TextEdit,
};
use schema::Schema;

/// State shared between the editor and its providers.
#[derive(Default)]
pub struct Shared {
    pub schema: Option<Arc<Schema>>,
    /// Schema warnings of the current text (with quick fixes).
    pub issues: Vec<SchemaIssue>,
    /// Insert texts of the last completions offered, with the cursor offset inside each.
    pub offered: Vec<(String, usize)>,
    /// Labels accepted this session, and how often.
    pub recent: HashMap<String, u32>,
}

pub type SharedState = Rc<RefCell<Shared>>;

pub struct GremlinCompletions {
    pub shared: SharedState,
}

fn kind(kind: ItemKind) -> CompletionItemKind {
    match kind {
        ItemKind::Step => CompletionItemKind::METHOD,
        ItemKind::VertexLabel | ItemKind::EdgeLabel => CompletionItemKind::CLASS,
        ItemKind::PropertyKey => CompletionItemKind::FIELD,
        ItemKind::Value => CompletionItemKind::VALUE,
        ItemKind::Predicate => CompletionItemKind::FUNCTION,
        ItemKind::Enum => CompletionItemKind::ENUM_MEMBER,
        ItemKind::StepLabel => CompletionItemKind::VARIABLE,
        ItemKind::Snippet => CompletionItemKind::SNIPPET,
        ItemKind::Keyword => CompletionItemKind::KEYWORD,
    }
}

/// Completion items for `text` at `offset`, ranked, with recently accepted names first among equally
/// good prefix matches.
pub fn items_at(shared: &Shared, text: &str, offset: usize) -> (Range<usize>, Vec<Item>) {
    let result = complete(text, offset, shared.schema.as_deref());
    let typed = &text[result.replace.clone()];
    let mut items = result.items;
    if !shared.recent.is_empty() {
        // Stable: only reorders recent names that are exact prefix matches to the front.
        items.sort_by_key(|i| {
            let recent = i.label.starts_with(typed) && shared.recent.contains_key(&i.label);
            !recent
        });
    }
    (result.replace, items)
}

impl CompletionProvider for GremlinCompletions {
    fn completions(
        &self,
        rope: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let text = rope.to_string();
        let (replace, items) = items_at(&self.shared.borrow(), &text, offset);
        let range = lsp_types::Range {
            start: rope.offset_to_position(replace.start),
            end: rope.offset_to_position(replace.end),
        };
        let mut offered = Vec::with_capacity(items.len());
        let lsp: Vec<CompletionItem> = items
            .into_iter()
            .take(200)
            .enumerate()
            .map(|(i, item)| {
                if let Some(cursor) = item.cursor {
                    offered.push((item.insert.clone(), cursor));
                }
                CompletionItem {
                    label: item.label.clone(),
                    kind: Some(kind(item.kind)),
                    detail: Some(item.detail.clone()),
                    sort_text: Some(format!("{i:04}")),
                    filter_text: Some(item.label.clone()),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range,
                        new_text: item.insert,
                    })),
                    ..Default::default()
                }
            })
            .collect();
        self.shared.borrow_mut().offered = offered;
        Task::ready(Ok(CompletionResponse::Array(lsp)))
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        !new_text.is_empty()
            && (new_text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                || matches!(new_text, "." | "(" | "'" | "\"" | ","))
    }
}

/// "Replace with `airport`" for the schema warning under the cursor.
pub struct SchemaFixes {
    pub shared: SharedState,
}

/// The quick fixes overlapping `range`: (title, range to replace, replacement).
pub fn fixes_at(shared: &Shared, range: &Range<usize>) -> Vec<(String, Range<usize>, String)> {
    shared
        .issues
        .iter()
        .filter(|i| i.range.start <= range.end && range.start <= i.range.end)
        .filter_map(|i| {
            let fix = i.replacement.clone()?;
            Some((format!("Replace with `{fix}`"), i.range.clone(), fix))
        })
        .collect()
}

impl CodeActionProvider for SchemaFixes {
    fn id(&self) -> SharedString {
        "gremlin-schema".into()
    }

    fn code_actions(
        &self,
        _: Entity<EditorState>,
        range: Range<usize>,
        _: &mut Window,
        _: &mut App,
    ) -> Task<anyhow::Result<Vec<CodeAction>>> {
        let actions = fixes_at(&self.shared.borrow(), &range)
            .into_iter()
            .map(|(title, range, fix)| CodeAction {
                title,
                kind: Some(CodeActionKind::QUICKFIX),
                is_preferred: Some(true),
                data: Some(serde_json::json!({
                    "start": range.start,
                    "end": range.end,
                    "text": fix,
                })),
                ..Default::default()
            })
            .collect();
        Task::ready(Ok(actions))
    }

    fn perform_code_action(
        &self,
        state: Entity<EditorState>,
        action: CodeAction,
        _: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let Some(data) = action.data else {
            return Task::ready(Ok(()));
        };
        let start = data["start"].as_u64().unwrap_or(0) as usize;
        let end = data["end"].as_u64().unwrap_or(0) as usize;
        let text = data["text"].as_str().unwrap_or_default().to_owned();
        // Called while the editor state is being updated: edit it right after.
        window.defer(cx, move |window, cx| {
            apply_replacement(&state, start..end, &text, window, cx);
        });
        Task::ready(Ok(()))
    }
}

/// Replace `range` with `text` as one undoable edit.
pub fn apply_replacement(
    state: &Entity<EditorState>,
    range: Range<usize>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let text = text.to_owned();
    state.update(cx, |s, cx| {
        if range.end > s.text().len() {
            return;
        }
        s.set_selected_range(range, cx);
        s.replace(text, window, cx);
    });
}
