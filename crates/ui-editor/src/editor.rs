//! `QueryEditor`: the gpui-kit code editor with Gremlin highlighting, live diagnostics, a problems list,
//! hover docs, format, comment toggling, a current-statement highlight and run requests.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::highlighter::{Diagnostic as EditorDiagnostic, DiagnosticSeverity};
use gpui_kit::component::input::{
    Editor, EditorState, InputEvent, Position, RangeDecoration, RangeDecorationCollection,
    RangeDecorationStyle, RopeExt as _,
};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_syntax::{Diagnostic, Document, Severity, line_col};

use crate::actions::{FormatQuery, RunQuery, ToggleLineComment};
use crate::hover::StepHover;
use crate::language::LANGUAGE_NAME;
use crate::{comment, tokens};

/// What the editor asks its host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryEditorEvent {
    /// Run `text` (the statement at the cursor, or the selection), found at `range` in the document.
    RunRequested { text: String, range: Range<usize> },
    /// The text changed (for dirty markers and persistence).
    Changed,
}

/// The key context of the editor wrapper; bindings for Run/Format/Comment use `QueryEditor > Input`.
pub const KEY_CONTEXT: &str = "QueryEditor";

/// Documents larger than this are analysed on a background thread after each edit.
const BACKGROUND_ANALYSIS_BYTES: usize = 64 * 1024;

pub struct QueryEditor {
    state: Entity<EditorState>,
    /// The last analysed document; may trail the editor text while a background analysis runs.
    doc: Document,
    /// Text of the editor as of the latest edit (what `doc` will become).
    latest_text: String,
    revision: u64,
    analysed_revision: u64,
    analysis: Option<Task<()>>,
    diagnostics: Vec<Diagnostic>,
    statement_highlight: RangeDecorationCollection,
    highlighted: Option<Range<usize>>,
    last_cursor: usize,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<QueryEditorEvent> for QueryEditor {}

impl QueryEditor {
    pub fn new(text: &str, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let initial = text.to_owned();
        let state = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(LANGUAGE_NAME)
                .default_value(initial);
            state.lsp_mut().hover_provider = Some(Rc::new(StepHover));
            state
        });
        let statement_highlight = state.update(cx, |s, cx| {
            s.create_range_decorations_collection(Vec::new(), cx)
        });
        let subscriptions = vec![
            cx.subscribe_in(&state, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.on_change(cx);
                }
            }),
            // Cursor moves notify the editor state; keep the statement highlight in step.
            cx.observe(&state, |this, _, cx| this.sync_statement_highlight(cx)),
        ];
        let mut this = Self {
            doc: Document::parse(text),
            latest_text: text.to_owned(),
            revision: 0,
            analysed_revision: 0,
            analysis: None,
            state,
            diagnostics: Vec::new(),
            statement_highlight,
            highlighted: None,
            last_cursor: usize::MAX,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.publish_diagnostics(cx);
        this
    }

    pub fn state(&self) -> &Entity<EditorState> {
        &self.state
    }

    pub fn text(&self, cx: &App) -> String {
        self.state.read(cx).value().to_string()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The range of the statement highlighted as "will run".
    pub fn highlighted_statement(&self) -> Option<Range<usize>> {
        self.highlighted.clone()
    }

    pub fn cursor(&self, cx: &App) -> usize {
        self.state.read(cx).cursor()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |s, cx| s.focus(window, cx));
    }

    /// Mirror the edit into the analysis document as one incremental edit (common prefix/suffix diff
    /// against the previous text). Large documents are analysed on a background thread.
    fn on_change(&mut self, cx: &mut Context<'_, Self>) {
        let new = self.state.read(cx).text().to_string();
        let (range, inserted) = diff(&self.latest_text, &new);
        self.latest_text = new;
        self.revision += 1;
        let revision = self.revision;
        cx.emit(QueryEditorEvent::Changed);

        if self.latest_text.len() < BACKGROUND_ANALYSIS_BYTES && self.analysis.is_none() {
            self.doc.edit(range, &inserted);
            self.analysed_revision = revision;
            self.after_analysis(cx);
            return;
        }
        // Start from the last analysed document (plus any edits made while it was pending, via a fresh
        // diff against the latest text) so superseded analyses are simply discarded.
        let mut doc = self.doc.fork();
        let target = self.latest_text.clone();
        let base_range_and_text = diff(doc.text(), &target);
        let task = cx.background_spawn(async move {
            doc.edit(base_range_and_text.0, &base_range_and_text.1);
            let diagnostics = doc.diagnostics();
            (doc, diagnostics)
        });
        self.analysis = Some(cx.spawn(async move |this, cx| {
            let (doc, diagnostics) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return; // superseded; the newer edit's analysis will land
                }
                this.doc = doc;
                this.analysed_revision = revision;
                this.analysis = None;
                this.diagnostics = diagnostics;
                this.push_diagnostics(cx);
                this.highlighted = None;
                this.last_cursor = usize::MAX;
                this.sync_statement_highlight(cx);
                cx.notify();
            });
        }));
    }

    /// Replace the whole text programmatically (restores, tests). Not an undoable edit.
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        let owned = text.to_owned();
        self.state
            .update(cx, |s, cx| s.set_value(owned, window, cx));
        self.doc = Document::parse(text);
        self.latest_text = text.to_owned();
        self.revision += 1;
        self.analysed_revision = self.revision;
        self.analysis = None;
        self.after_analysis(cx);
    }

    fn after_analysis(&mut self, cx: &mut Context<'_, Self>) {
        self.publish_diagnostics(cx);
        self.highlighted = None;
        self.last_cursor = usize::MAX;
        self.sync_statement_highlight(cx);
        cx.notify();
    }

    /// The analysis matches the editor text (no background analysis pending).
    pub fn is_analysis_current(&self) -> bool {
        self.analysed_revision == self.revision
    }

    fn publish_diagnostics(&mut self, cx: &mut Context<'_, Self>) {
        self.diagnostics = self.doc.diagnostics();
        self.push_diagnostics(cx);
    }

    /// Hand `self.diagnostics` to the editor for underlines.
    fn push_diagnostics(&mut self, cx: &mut Context<'_, Self>) {
        let diagnostics = self.diagnostics.clone();
        self.state.update(cx, |state, cx| {
            let rope = state.text().clone();
            if let Some(set) = state.diagnostics_mut() {
                set.reset(&rope);
                for d in diagnostics {
                    let start = rope.offset_to_position(d.range.start);
                    // Zero-width diagnostics (a missing `)`) still need a visible underline.
                    let end_offset = if d.range.is_empty() {
                        (d.range.end + 1).min(rope.len())
                    } else {
                        d.range.end
                    };
                    let start_offset = if d.range.is_empty() && end_offset == d.range.end {
                        d.range.start.saturating_sub(1)
                    } else {
                        d.range.start
                    };
                    let start = if start_offset == d.range.start {
                        start
                    } else {
                        rope.offset_to_position(start_offset)
                    };
                    let end = rope.offset_to_position(end_offset);
                    let severity = match d.severity {
                        Severity::Error => DiagnosticSeverity::Error,
                        Severity::Warning => DiagnosticSeverity::Warning,
                    };
                    set.push(
                        EditorDiagnostic::new(start..end, d.message)
                            .with_severity(severity)
                            .with_source("gremlin"),
                    );
                }
            }
            cx.notify();
        });
    }

    fn sync_statement_highlight(&mut self, cx: &mut Context<'_, Self>) {
        let cursor = self.state.read(cx).cursor();
        if cursor == self.last_cursor {
            return;
        }
        self.last_cursor = cursor;
        let range = self.doc.statement_at(cursor).map(|s| s.range);
        // Only worth highlighting when there is more than one statement to choose from.
        let range = range.filter(|_| self.doc.statement_count() > 1);
        if range == self.highlighted {
            return;
        }
        self.highlighted = range.clone();
        let decorations = range
            .map(|r| {
                vec![
                    RangeDecoration::new(r)
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(tokens::statement_highlight(cx)),
                ]
            })
            .unwrap_or_default();
        self.statement_highlight.set(decorations, cx);
    }

    /// What Run would execute: the selection if any, else the statement at the cursor.
    pub fn run_target(&self, cx: &App) -> Option<(String, Range<usize>)> {
        let state = self.state.read(cx);
        let selection = state.selected_range();
        let text = state.value();
        if !selection.is_empty() {
            let selected = text[selection.clone()].to_owned();
            return (!selected.trim().is_empty()).then_some((selected, selection));
        }
        let statement = if self.is_analysis_current() {
            self.doc.statement_at(state.cursor())?
        } else {
            // A background analysis is still running: parse the current text for this one request.
            Document::parse(self.latest_text.as_str()).statement_at(state.cursor())?
        };
        Some((statement.text, statement.range))
    }

    pub fn run(&mut self, _: &RunQuery, _: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some((text, range)) = self.run_target(cx) {
            cx.emit(QueryEditorEvent::RunRequested { text, range });
        }
    }

    pub fn format(&mut self, _: &FormatQuery, window: &mut Window, cx: &mut Context<'_, Self>) {
        let width = gremlin_syntax::format::DEFAULT_WIDTH;
        self.state.update(cx, |state, cx| {
            let selection = state.selected_range();
            if selection.is_empty() {
                let formatted = gremlin_syntax::format(&state.value(), width);
                let formatted = formatted.trim_end_matches('\n').to_owned();
                if formatted != state.value().as_ref() {
                    // One undoable edit.
                    state.replace_all(formatted, window, cx);
                }
            } else {
                let selected = state.value()[selection].to_owned();
                let formatted = gremlin_syntax::format(&selected, width);
                state.replace(formatted.trim_end_matches('\n').to_owned(), window, cx);
            }
        });
    }

    pub fn toggle_comment(
        &mut self,
        _: &ToggleLineComment,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.state.update(cx, |state, cx| {
            let text = state.value().to_string();
            let span = comment::line_span(&text, &state.selected_range());
            let toggled = comment::toggle(&text[span.clone()]);
            state.set_selected_range(span.clone(), cx);
            state.replace(toggled.clone(), window, cx);
            state.set_selected_range(span.start..span.start + toggled.len(), cx);
        });
    }

    /// Move the cursor to a diagnostic (problems list click).
    pub fn jump_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.state.update(cx, |state, cx| {
            let position: Position = state.text().offset_to_position(offset);
            state.set_cursor_position(position, window, cx);
        });
    }

    fn render_problems(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let text = self.doc.text().to_owned();
        let errors = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warnings = self.diagnostics.len() - errors;
        let rows = self.diagnostics.iter().enumerate().map(|(i, d)| {
            let (line, col) = line_col(&text, d.range.start);
            let offset = d.range.start;
            let color = match d.severity {
                Severity::Error => theme.danger,
                Severity::Warning => theme.warning,
            };
            h_flex()
                .id(SharedString::from(format!("problem-{i}")))
                .test_support()
                .gap(tokens::space(2.0))
                .px(tokens::space(3.0))
                .py(tokens::space(0.5))
                .cursor_pointer()
                .hover(|s| s.bg(theme.list_hover))
                .on_click(cx.listener(move |this, _, window, cx| this.jump_to(offset, window, cx)))
                .child(div().size(tokens::size(6.0)).rounded_full().bg(color))
                .child(div().text_color(theme.foreground).child(d.message.clone()))
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child(format!("Ln {line}, Col {col}")),
                )
        });
        let summary = match (errors, warnings) {
            (0, 0) => "No problems".to_owned(),
            (e, 0) => format!("{e} error{}", if e == 1 { "" } else { "s" }),
            (0, w) => format!("{w} warning{}", if w == 1 { "" } else { "s" }),
            (e, w) => format!(
                "{e} error{}, {w} warning{}",
                if e == 1 { "" } else { "s" },
                if w == 1 { "" } else { "s" }
            ),
        };
        v_flex()
            .id("problems")
            .test_support()
            .flex_none()
            .max_h(tokens::size(120.0))
            .border_t(tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.background)
            .text_size(tokens::text_sm())
            .child(
                div()
                    .px(tokens::space(3.0))
                    .py(tokens::space(1.0))
                    .text_color(if errors > 0 {
                        theme.danger
                    } else {
                        theme.muted_foreground
                    })
                    .child(summary),
            )
            .child(
                v_flex()
                    .id("problems-list")
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}

/// The single edit turning `old` into `new`: the byte range replaced in `old` and the inserted text,
/// kept on character boundaries of both.
fn diff(old: &str, new: &str) -> (Range<usize>, String) {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let mut start = prefix;
    while start > 0 && (!old.is_char_boundary(start) || !new.is_char_boundary(start)) {
        start -= 1;
    }
    let mut old_end = old.len() - suffix;
    let mut new_end = new.len() - suffix;
    while old_end < old.len() && (!old.is_char_boundary(old_end) || !new.is_char_boundary(new_end))
    {
        old_end += 1;
        new_end += 1;
    }
    (start..old_end, new[start..new_end].to_owned())
}

impl Focusable for QueryEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).focus_handle(cx)
    }
}

impl Render for QueryEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let has_problems = !self.diagnostics.is_empty();
        v_flex()
            .id("query-editor")
            .key_context(KEY_CONTEXT)
            .size_full()
            .on_action(cx.listener(Self::run))
            .on_action(cx.listener(Self::format))
            .on_action(cx.listener(Self::toggle_comment))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Editor::new(&self.state).size_full().bordered(false)),
            )
            .when(has_problems, |el| el.child(self.render_problems(cx)))
            .track_focus(&self.focus_handle)
    }
}

#[cfg(test)]
mod tests {
    use super::diff;

    #[test]
    fn diff_finds_the_single_edit() {
        assert_eq!(diff("g.V()", "g.V().out()"), (5..5, ".out()".to_owned()));
        assert_eq!(diff("g.V().out()", "g.V()"), (5..11, String::new()));
        assert_eq!(diff("abc", "aXc"), (1..2, "X".to_owned()));
        assert_eq!(diff("héllo", "hállo"), (1..3, "á".to_owned()));
        assert_eq!(diff("aa", "aaa"), (2..2, "a".to_owned()));
    }
}
