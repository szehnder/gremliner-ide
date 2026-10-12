//! One tab's assistant conversation: quick actions, context chips, the transcript (streamed Markdown,
//! tool calls, explanation items, verified Gremlin blocks) and the input. The host runs turns and feeds
//! [`TurnEvent`]s back with [`ChatView::apply`].

use std::ops::Range;

use assistant::DryRun;
use assistant::chat::{CodeBlock, ExplainItem, Mode, TurnEvent};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ui_theme::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChipKind {
    Query,
    Error,
    Result,
    Schema,
    Samples,
}

impl ChipKind {
    pub const ALL: [ChipKind; 5] = [
        ChipKind::Query,
        ChipKind::Error,
        ChipKind::Result,
        ChipKind::Schema,
        ChipKind::Samples,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ChipKind::Query => "query",
            ChipKind::Error => "error",
            ChipKind::Result => "result",
            ChipKind::Schema => "schema",
            ChipKind::Samples => "samples",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ChipKind::Query => "Query",
            ChipKind::Error => "Error",
            ChipKind::Result => "Result",
            ChipKind::Schema => "Schema",
            ChipKind::Samples => "Sample data",
        }
    }
}

/// Which context items exist right now (the host updates this as the tab changes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Available {
    pub query: bool,
    pub error: bool,
    pub result: bool,
    pub schema: bool,
    /// The connection allows sharing sample data.
    pub samples: bool,
}

impl Available {
    fn has(&self, k: ChipKind) -> bool {
        match k {
            ChipKind::Query => self.query,
            ChipKind::Error => self.error,
            ChipKind::Result => self.result,
            ChipKind::Schema => self.schema,
            ChipKind::Samples => self.samples,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatViewEvent {
    /// Run a turn with these context items.
    Send {
        mode: Mode,
        question: String,
        include: Vec<ChipKind>,
    },
    Insert(String),
    Run {
        query: String,
        writes: bool,
    },
    /// Highlight these ranges of the explained statement in the editor (empty: clear).
    HighlightSteps(Vec<Range<usize>>),
    Clear,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User { text: String, mode: Mode },
    Assistant(Answer),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Answer {
    pub markdown: String,
    pub tools: Vec<(String, String, String)>,
    pub blocks: Vec<CodeBlock>,
    pub explain: Vec<ExplainItem>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub done: bool,
}

pub struct ChatView {
    pub input: Entity<InputState>,
    pub entries: Vec<Entry>,
    available: Available,
    removed: Vec<ChipKind>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ChatViewEvent> for ChatView {}

impl Focusable for ChatView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ChatView {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Ask about this query or your graph\u{2026}")
        });
        let subscriptions =
            vec![
                cx.subscribe_in(&input, window, |this, _, e: &InputEvent, window, cx| {
                    if matches!(e, InputEvent::PressEnter { .. }) {
                        this.send_input(window, cx);
                    }
                }),
            ];
        Self {
            input,
            entries: Vec::new(),
            available: Available::default(),
            removed: Vec::new(),
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_available(&mut self, available: Available, cx: &mut Context<'_, Self>) {
        if self.available != available {
            self.available = available;
            cx.notify();
        }
    }

    /// The context items that will be sent.
    pub fn included(&self) -> Vec<ChipKind> {
        ChipKind::ALL
            .into_iter()
            .filter(|k| self.available.has(*k) && !self.removed.contains(k))
            .collect()
    }

    pub fn toggle_chip(&mut self, kind: ChipKind, cx: &mut Context<'_, Self>) {
        if let Some(i) = self.removed.iter().position(|k| *k == kind) {
            self.removed.remove(i);
        } else {
            self.removed.push(kind);
        }
        cx.notify();
    }

    pub fn busy(&self) -> bool {
        matches!(self.entries.last(), Some(Entry::Assistant(a)) if !a.done)
    }

    /// Start a turn (quick action or typed question).
    pub fn send(&mut self, mode: Mode, question: String, cx: &mut Context<'_, Self>) {
        if self.busy() {
            return;
        }
        let shown = if question.is_empty() {
            mode.label().to_owned()
        } else {
            question.clone()
        };
        self.entries.push(Entry::User { text: shown, mode });
        self.entries.push(Entry::Assistant(Answer {
            status: Some("Thinking\u{2026}".into()),
            ..Answer::default()
        }));
        cx.emit(ChatViewEvent::Send {
            mode,
            question,
            include: self.included(),
        });
        cx.notify();
    }

    fn send_input(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let text = self.input.read(cx).value().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.input.update(cx, |i, cx| i.set_value("", window, cx));
        self.send(Mode::Chat, text, cx);
    }

    pub fn clear(&mut self, cx: &mut Context<'_, Self>) {
        self.entries.clear();
        cx.emit(ChatViewEvent::Clear);
        cx.emit(ChatViewEvent::HighlightSteps(Vec::new()));
        cx.notify();
    }

    /// Apply an event of the running turn.
    pub fn apply(&mut self, event: TurnEvent, cx: &mut Context<'_, Self>) {
        let Some(Entry::Assistant(a)) = self.entries.last_mut() else {
            return;
        };
        match event {
            TurnEvent::Status(s) => a.status = Some(s),
            TurnEvent::Text(t) => {
                a.status = None;
                a.markdown.push_str(&t);
            }
            TurnEvent::Tool {
                name,
                input,
                summary,
            } => a.tools.push((name, input, summary)),
            TurnEvent::Block(b) => a.blocks.push(b),
            TurnEvent::Explain(items) => {
                a.status = None;
                a.explain = items;
            }
            TurnEvent::Error(e) => {
                a.status = None;
                a.error = Some(e);
            }
            TurnEvent::Done => {
                a.status = None;
                a.done = true;
            }
        }
        cx.notify();
    }

    fn render_chips(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        h_flex()
            .id("assistant-chips")
            .test_support()
            .flex_wrap()
            .gap(Tokens::space(1.0))
            .children(
                ChipKind::ALL
                    .into_iter()
                    .filter(|k| self.available.has(*k))
                    .map(|k| {
                        let on = !self.removed.contains(&k);
                        h_flex()
                            .id(SharedString::from(format!("chip-{}", k.id())))
                            .test_support()
                            .gap(Tokens::space(0.5))
                            .px(Tokens::space(1.5))
                            .rounded(Tokens::RADIUS_SM)
                            .border(Tokens::BORDER)
                            .border_color(if on { theme.primary } else { theme.border })
                            .text_size(Tokens::text_xs())
                            .text_color(if on {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            })
                            .when(!on, |c| c.line_through())
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle_chip(k, cx)))
                            .child(k.label())
                            .child(if on { "\u{d7}" } else { "+" })
                    }),
            )
    }

    fn render_block(
        &self,
        idx: usize,
        b: &CodeBlock,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let highlights = ui_editor::highlight_styles(&b.query, cx);
        let badge = |text: String, color: Hsla| {
            div()
                .px(Tokens::space(1.5))
                .rounded(Tokens::RADIUS_SM)
                .border(Tokens::BORDER)
                .border_color(color)
                .text_size(Tokens::text_xs())
                .text_color(color)
                .child(text)
        };
        let mut badges = h_flex().gap(Tokens::space(1.0)).flex_wrap();
        badges = badges.child(if b.issues.is_empty() {
            badge("Valid \u{2713}".into(), theme.success)
        } else {
            badge(format!("{} problem(s)", b.issues.len()), theme.danger)
        });
        badges = badges.child(match &b.dry_run {
            DryRun::Passed { rows, elapsed } => badge(
                format!(
                    "Dry run \u{2713} {rows} rows \u{b7} {} ms",
                    elapsed.as_millis()
                ),
                theme.success,
            ),
            DryRun::Failed { .. } => badge("Dry run \u{2717}".into(), theme.danger),
            DryRun::Skipped { reason } => {
                badge(format!("Not run: {reason}"), theme.muted_foreground)
            }
        });
        if let Some(eq) = b.equivalent {
            badges = badges.child(if eq {
                badge("Equivalent on sample \u{2713}".into(), theme.success)
            } else {
                badge("Different count on sample".into(), theme.warning)
            });
        }
        let query = b.query.clone();
        let insert_query = query.clone();
        let copy_query = query.clone();
        let writes = b.writes;
        v_flex()
            .id(SharedString::from(format!("assistant-block-{idx}")))
            .test_support()
            .gap(Tokens::space(1.0))
            .p(Tokens::space(2.0))
            .rounded(Tokens::RADIUS_MD)
            .bg(theme.muted)
            .child(
                div()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(Tokens::text_sm())
                    .child(StyledText::new(query.clone()).with_highlights(highlights)),
            )
            .child(badges)
            .children(b.issues.iter().map(|i| {
                div()
                    .text_size(Tokens::text_xs())
                    .text_color(theme.danger)
                    .child(i.message.clone())
            }))
            .child(
                h_flex()
                    .gap(Tokens::space(1.0))
                    .child(
                        Button::new(SharedString::from(format!("assistant-insert-{idx}")))
                            .label("Insert")
                            .xsmall()
                            .outline()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(ChatViewEvent::Insert(insert_query.clone()))
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("assistant-run-{idx}")))
                            .label("Run")
                            .xsmall()
                            .outline()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(ChatViewEvent::Run {
                                    query: query.clone(),
                                    writes,
                                })
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("assistant-copy-{idx}")))
                            .label("Copy")
                            .xsmall()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_query.clone()))
                            }),
                    ),
            )
    }

    fn render_answer(&self, n: usize, a: &Answer, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut col = v_flex()
            .id(SharedString::from(format!("assistant-answer-{n}")))
            .test_support()
            .gap(Tokens::space(1.5));
        for (i, (name, input, summary)) in a.tools.iter().enumerate() {
            col = col.child(
                h_flex()
                    .id(SharedString::from(format!("assistant-tool-{n}-{i}")))
                    .test_support()
                    .gap(Tokens::space(1.0))
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(IconName::Settings2).xsmall())
                    .child(format!("{name}: {input} \u{2192} {summary}")),
            );
        }
        if !a.markdown.is_empty() {
            col = col.child(
                TextView::markdown(
                    SharedString::from(format!("assistant-md-{n}")),
                    a.markdown.clone(),
                )
                .text_size(Tokens::text_sm()),
            );
        }
        for (i, item) in a.explain.iter().enumerate() {
            let spans = item.spans.clone();
            col = col.child(
                h_flex()
                    .id(SharedString::from(format!("explain-item-{i}")))
                    .test_support()
                    .gap(Tokens::space(2.0))
                    .px(Tokens::space(1.5))
                    .py(Tokens::space(0.5))
                    .rounded(Tokens::RADIUS_SM)
                    .hover(|s| s.bg(theme.list_hover))
                    .text_size(Tokens::text_sm())
                    .on_hover(cx.listener(move |_, hovered: &bool, _, cx| {
                        let ranges = if *hovered { spans.clone() } else { Vec::new() };
                        cx.emit(ChatViewEvent::HighlightSteps(ranges));
                    }))
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme.muted_foreground)
                            .child(format!("{}.", i + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(theme.foreground)
                            .child(item.text.clone()),
                    ),
            );
        }
        for (i, b) in a.blocks.iter().enumerate() {
            col = col.child(self.render_block(n * 100 + i, b, cx));
        }
        if let Some(s) = &a.status {
            col = col.child(
                h_flex()
                    .gap(Tokens::space(1.5))
                    .text_size(Tokens::text_sm())
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child(s.clone()),
            );
        }
        if let Some(e) = &a.error {
            col = col.child(
                div()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.danger)
                    .child(e.clone()),
            );
        }
        col.into_any_element()
    }
}

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let busy = self.busy();
        let quick = |id: &'static str, mode: Mode, enabled: bool, cx: &mut Context<'_, Self>| {
            Button::new(id)
                .label(mode.label())
                .xsmall()
                .outline()
                .disabled(!enabled || busy)
                .on_click(cx.listener(move |this, _, _, cx| this.send(mode, String::new(), cx)))
        };
        let a = self.available.clone();
        let entries: Vec<AnyElement> = self
            .entries
            .clone()
            .iter()
            .enumerate()
            .map(|(n, e)| match e {
                Entry::User { text, .. } => div()
                    .id(SharedString::from(format!("assistant-user-{n}")))
                    .test_support()
                    .self_end()
                    .max_w(Tokens::size(320.0))
                    .px(Tokens::space(2.0))
                    .py(Tokens::space(1.0))
                    .rounded(Tokens::RADIUS_MD)
                    .bg(theme.secondary)
                    .text_size(Tokens::text_sm())
                    .child(text.clone())
                    .into_any_element(),
                Entry::Assistant(ans) => self.render_answer(n, ans, cx),
            })
            .collect();
        let empty = entries.is_empty();
        v_flex()
            .id("assistant-chat")
            .test_support()
            .track_focus(&self.focus)
            .size_full()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(Tokens::space(1.0))
                    .p(Tokens::space(2.0))
                    .border_b(Tokens::BORDER)
                    .border_color(theme.border)
                    .child(quick("assistant-explain", Mode::Explain, a.query, cx))
                    .child(quick("assistant-fix", Mode::Fix, a.query && a.error, cx))
                    .child(quick("assistant-optimize", Mode::Optimize, a.query, cx))
                    .child(quick("assistant-describe", Mode::Describe, a.result, cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("assistant-clear")
                            .label("Clear")
                            .xsmall()
                            .ghost()
                            .disabled(empty || busy)
                            .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                    ),
            )
            .child(
                v_flex()
                    .id("assistant-transcript")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(Tokens::space(3.0))
                    .p(Tokens::space(2.0))
                    .children(entries)
                    .when(empty, |c| {
                        c.child(
                            div()
                                .text_size(Tokens::text_sm())
                                .text_color(theme.muted_foreground)
                                .child("Ask anything about the current query, its results or your graph. Quick actions above explain, fix or speed up the query."),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap(Tokens::space(1.5))
                    .p(Tokens::space(2.0))
                    .border_t(Tokens::BORDER)
                    .border_color(theme.border)
                    .child(self.render_chips(cx))
                    .child(Input::new(&self.input).small()),
            )
    }
}
