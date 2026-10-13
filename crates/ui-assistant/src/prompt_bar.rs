//! The prompt bar above the editor (Ctrl/Cmd+K) and its result card. The bar only holds state and
//! renders; the host runs the pipeline and feeds [`PipelineEvent`]s back with [`PromptBar::apply`].

use std::ops::Range;

use assistant::{DryRun, Outcome, PipelineEvent, Stage};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gremlin_model::normalize;
use llm::Message;
use ui_results::GraphView;
use ui_theme::ButtonTip as _;
use ui_theme::Tokens;

gpui_kit::actions!(assistant, [OpenPromptBar, ClosePromptBar]);

pub const KEY_CONTEXT: &str = "PromptBar";

/// What the bar shows about the model and the data it may see (ADR-0021).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disclosure {
    pub profile: String,
    pub local: bool,
    pub share_samples: bool,
}

impl Disclosure {
    pub fn text(&self) -> String {
        let data = if self.share_samples {
            "schema + sample values"
        } else {
            "schema structure only"
        };
        format!("{} \u{b7} sends {data}", self.profile)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptBarEvent {
    /// Run the pipeline for `prompt` after `history` (empty for a new request).
    Submit {
        prompt: String,
        history: Vec<Message>,
    },
    /// Insert the query into the editor as a new statement.
    Insert(String),
    /// Insert and run (writes need the host's confirmation).
    Run {
        query: String,
        writes: bool,
    },
    OpenSettings,
    Close,
}

/// One request's card.
pub struct Card {
    pub prompt: String,
    pub stage: Option<Stage>,
    pub candidate: Option<String>,
    pub outcome: Option<Outcome>,
    pub error: Option<String>,
    /// Earlier turns, for refinements of this card.
    pub history: Vec<Message>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    preview_graph: Option<Entity<GraphView>>,
}

pub struct PromptBar {
    pub input: Entity<InputState>,
    pub refine: Entity<InputState>,
    disclosure: Option<Disclosure>,
    pub card: Option<Card>,
    run_id: u64,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PromptBarEvent> for PromptBar {}

impl Focusable for PromptBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PromptBar {
    pub fn new(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Ask about your graph, e.g. \u{201c}airports in Texas with more than 3 runways\u{201d}")
        });
        let refine = cx.new(|cx| {
            InputState::new(window, cx).placeholder(
                "Refine: \u{201c}only those\u{2026}\u{201d}, \u{201c}also show\u{2026}\u{201d}",
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::PressEnter { .. }) {
                    this.submit(window, cx);
                }
            }),
            cx.subscribe_in(&refine, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::PressEnter { .. }) {
                    this.submit_refine(window, cx);
                }
            }),
        ];
        Self {
            input,
            refine,
            disclosure: None,
            card: None,
            run_id: 0,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// `None`: no LLM profile is configured.
    pub fn set_disclosure(&mut self, d: Option<Disclosure>, cx: &mut Context<'_, Self>) {
        self.disclosure = d;
        cx.notify();
    }

    pub fn disclosure(&self) -> Option<&Disclosure> {
        self.disclosure.as_ref()
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |i, cx| i.focus(window, cx));
    }

    /// The id of the current request; events for older ones are ignored by the host.
    pub fn run_id(&self) -> u64 {
        self.run_id
    }

    fn start(&mut self, prompt: String, history: Vec<Message>, cx: &mut Context<'_, Self>) {
        self.run_id += 1;
        self.card = Some(Card {
            prompt: prompt.clone(),
            stage: Some(Stage::Generating),
            candidate: None,
            outcome: None,
            error: None,
            history: history.clone(),
            highlights: Vec::new(),
            preview_graph: None,
        });
        cx.emit(PromptBarEvent::Submit { prompt, history });
        cx.notify();
    }

    /// Type `prompt` into the bar and submit it (the app's `--ask`).
    pub fn ask(&mut self, prompt: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.input
            .update(cx, |i, cx| i.set_value(prompt.to_owned(), window, cx));
        self.submit(window, cx);
    }

    pub fn submit(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) {
        let prompt = self.input.read(cx).value().trim().to_owned();
        if prompt.is_empty() || self.disclosure.is_none() {
            return;
        }
        self.start(prompt, Vec::new(), cx);
    }

    /// A follow-up on the current card, using its conversation.
    pub fn submit_refine(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let text = self.refine.read(cx).value().trim().to_owned();
        let Some(history) = self
            .card
            .as_ref()
            .and_then(|c| c.outcome.as_ref())
            .map(|o| o.history.clone())
        else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.refine.update(cx, |i, cx| i.set_value("", window, cx));
        self.input
            .update(cx, |i, cx| i.set_value(text.clone(), window, cx));
        self.start(text, history, cx);
    }

    /// Feed a pipeline event for request `run_id`.
    pub fn apply(
        &mut self,
        run_id: u64,
        event: PipelineEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if run_id != self.run_id {
            return;
        }
        let Some(card) = self.card.as_mut() else {
            return;
        };
        match event {
            PipelineEvent::Stage(s) => card.stage = Some(s),
            PipelineEvent::Candidate(q) => card.candidate = Some(q),
            PipelineEvent::Failed(e) => {
                card.error = Some(e);
                card.stage = None;
            }
            PipelineEvent::Done(outcome) => {
                card.stage = None;
                card.highlights = ui_editor::highlight_styles(&outcome.answer.query, cx);
                if gremlin_model::elements::has_graph_elements(&outcome.preview) {
                    let items = outcome.preview.clone();
                    let graph = cx.new(|cx| {
                        let mut g = GraphView::new(window, cx);
                        g.set_offline(true, cx);
                        g.set_edges_between(false, cx);
                        g.ingest(1, &items, true, cx);
                        g
                    });
                    card.preview_graph = Some(graph);
                }
                card.outcome = Some(*outcome);
            }
        }
        cx.notify();
    }

    /// The verified (or best) query on the card.
    pub fn query(&self) -> Option<&str> {
        self.card
            .as_ref()?
            .outcome
            .as_ref()
            .map(|o| o.answer.query.as_str())
    }

    pub fn insert(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(q) = self.query().map(str::to_owned) {
            self.card = None;
            cx.emit(PromptBarEvent::Insert(q));
            cx.notify();
        }
    }

    pub fn run(&mut self, cx: &mut Context<'_, Self>) {
        let Some(outcome) = self.card.as_ref().and_then(|c| c.outcome.as_ref()) else {
            return;
        };
        let event = PromptBarEvent::Run {
            query: outcome.answer.query.clone(),
            writes: outcome.writes,
        };
        cx.emit(event);
    }

    pub fn copy(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(q) = self.query() {
            cx.write_to_clipboard(ClipboardItem::new_string(q.to_owned()));
        }
    }

    fn render_bar(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let busy = self.card.as_ref().is_some_and(|c| c.stage.is_some());
        let row = h_flex()
            .id("prompt-bar")
            .test_support()
            .gap(Tokens::space(2.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(1.5))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                Icon::new(gpui_kit::assets::IconName::Sparkles)
                    .small()
                    .text_color(theme.primary),
            );
        match &self.disclosure {
            None => row
                .child(
                    div()
                        .id("prompt-no-llm")
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .text_size(Tokens::text_sm())
                        .text_color(theme.muted_foreground)
                        .child("The assistant needs a language model. Add a cloud or local model (Ollama, LM Studio) in AI settings."),
                )
                .child(
                    Button::new("prompt-open-settings")
                        .label("Open AI settings")
                        .small()
                        .primary()
                        .flex_none()
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(PromptBarEvent::OpenSettings))),
                ),
            Some(d) => row
                .child(div().flex_1().min_w_0().child(Input::new(&self.input).small().appearance(false)))
                .when(busy, |r| r.child(Spinner::new().xsmall()))
                .child(
                    div()
                        .id("prompt-disclosure")
                        .test_support()
                        .flex_none()
                        .max_w(Tokens::size(260.0))
                        .truncate()
                        .px(Tokens::space(2.0))
                        .rounded(Tokens::RADIUS_SM)
                        .bg(theme.muted)
                        .text_size(Tokens::text_xs())
                        .text_color(if d.share_samples { theme.warning } else { theme.muted_foreground })
                        .child(d.text()),
                ),
        }
        .child(
            Button::new("prompt-close")
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .tip("Close (Esc)")
                .on_click(cx.listener(|_, _, _, cx| cx.emit(PromptBarEvent::Close))),
        )
    }

    fn render_card(&self, card: &Card, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let mono = theme.mono_font_family.clone();
        let mut col = v_flex()
            .id("prompt-card")
            .test_support()
            .gap(Tokens::space(2.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(2.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .bg(theme.background)
            .text_size(Tokens::text_sm())
            .key_context(KEY_CONTEXT);
        if let Some(stage) = &card.stage {
            col = col.child(
                h_flex()
                    .id("prompt-status")
                    .test_support()
                    .gap(Tokens::space(2.0))
                    .text_color(theme.muted_foreground)
                    .child(Spinner::new().xsmall())
                    .child(stage.label()),
            );
            if let Some(q) = &card.candidate {
                col = col.child(
                    div()
                        .font_family(mono.clone())
                        .text_color(theme.muted_foreground)
                        .child(q.clone()),
                );
            }
        }
        if let Some(e) = &card.error {
            col = col.child(
                div()
                    .id("prompt-error")
                    .test_support()
                    .text_color(theme.danger)
                    .child(e.clone()),
            );
        }
        let Some(o) = &card.outcome else {
            return col;
        };
        let badge = |id: &'static str, ok: Option<bool>, text: String| {
            let color = match ok {
                Some(true) => theme.success,
                Some(false) => theme.danger,
                None => theme.muted_foreground,
            };
            div()
                .id(id)
                .test_support()
                .px(Tokens::space(1.5))
                .rounded(Tokens::RADIUS_SM)
                .border(Tokens::BORDER)
                .border_color(color)
                .text_size(Tokens::text_xs())
                .text_color(color)
                .child(text)
        };
        let mark = |ok: bool| if ok { "\u{2713}" } else { "\u{2717}" };
        let dry = match &o.badges.dry_run {
            DryRun::Passed { rows, elapsed } => (
                Some(true),
                format!(
                    "Dry run {} {rows} rows \u{b7} {} ms",
                    mark(true),
                    elapsed.as_millis()
                ),
            ),
            DryRun::Failed { .. } => (Some(false), format!("Dry run {}", mark(false))),
            DryRun::Skipped { reason } => (None, format!("Dry run skipped: {reason}")),
        };
        let badges = h_flex()
            .id("prompt-badges")
            .test_support()
            .gap(Tokens::space(1.5))
            .child(badge(
                "badge-syntax",
                Some(o.badges.syntax),
                format!("Syntax {}", mark(o.badges.syntax)),
            ))
            .child(badge(
                "badge-schema",
                o.badges.schema,
                match o.badges.schema {
                    Some(ok) => format!("Schema {}", mark(ok)),
                    None => "Schema not checked".into(),
                },
            ))
            .child(badge("badge-dry-run", dry.0, dry.1));
        col = col
            .child(
                div()
                    .id("prompt-query")
                    .test_support()
                    .p(Tokens::space(2.0))
                    .rounded(Tokens::RADIUS_MD)
                    .bg(theme.muted)
                    .font_family(mono)
                    .child(
                        StyledText::new(o.answer.query.clone())
                            .with_highlights(card.highlights.clone()),
                    ),
            )
            .children((!o.answer.explanation.is_empty()).then(|| {
                div()
                    .text_color(theme.foreground)
                    .child(o.answer.explanation.clone())
            }))
            .child(badges);
        if o.writes {
            col = col.child(
                h_flex()
                    .id("prompt-writes")
                    .test_support()
                    .gap(Tokens::space(1.5))
                    .text_color(theme.warning)
                    .child(Icon::new(IconName::TriangleAlert).small())
                    .child("Writes data. It was not test-run; running it asks for confirmation."),
            );
        }
        if !o.issues.is_empty() {
            col = col.child(
                v_flex()
                    .id("prompt-issues")
                    .test_support()
                    .text_color(theme.danger)
                    .child(
                        div().child(format!("Not verified after {} repair attempts:", o.repairs)),
                    )
                    .children(
                        o.issues
                            .iter()
                            .map(|i| div().child(format!("\u{2022} {}", i.message))),
                    ),
            );
        }
        if let Some(graph) = &card.preview_graph {
            col = col.child(
                div()
                    .id("prompt-preview-graph")
                    .test_support()
                    .h(Tokens::size(200.0))
                    .child(graph.clone()),
            );
        } else if !o.preview.is_empty() {
            col = col.child(preview_table(&o.preview, cx));
        }
        let verified = o.verified;
        col.child(
            h_flex()
                .gap(Tokens::space(2.0))
                .child(
                    Button::new("prompt-insert")
                        .label(if verified { "Insert" } else { "Insert anyway" })
                        .small()
                        .when(verified, |b| b.primary())
                        .when(!verified, |b| b.outline())
                        .on_click(cx.listener(|this, _, _, cx| this.insert(cx))),
                )
                .child(
                    Button::new("prompt-run")
                        .label("Run")
                        .small()
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| this.run(cx))),
                )
                .child(
                    Button::new("prompt-copy")
                        .label("Copy")
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                )
                .child(div().flex_1().child(Input::new(&self.refine).small())),
        )
    }
}

/// The first rows of a dry run as a small table.
fn preview_table(items: &[gremlin_model::GValue], cx: &App) -> impl IntoElement + use<> {
    const ROWS: usize = 8;
    const COLUMNS: usize = 6;
    let theme = cx.theme();
    let model = normalize::table(&items[..items.len().min(ROWS)]);
    let Some(group) = model.groups.first() else {
        return div().id("prompt-preview").test_support();
    };
    let columns: Vec<String> = group.columns.iter().take(COLUMNS).cloned().collect();
    let header = h_flex().children(columns.iter().map(|c| {
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .px(Tokens::space(1.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.muted_foreground)
            .child(c.clone())
    }));
    let rows = group.rows.iter().take(ROWS).map(|row| {
        h_flex()
            .border_t(Tokens::BORDER)
            .border_color(theme.border)
            .children(row.iter().take(COLUMNS).map(|cell| {
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .px(Tokens::space(1.0))
                    .text_color(theme.foreground)
                    .child(cell.display().to_owned())
            }))
    });
    div()
        .id("prompt-preview")
        .test_support()
        .text_size(Tokens::text_xs())
        .child(v_flex().child(header).children(rows))
}

impl Render for PromptBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let card = self
            .card
            .as_ref()
            .map(|c| self.render_card(c, cx).into_any_element());
        v_flex()
            .id("prompt-bar-root")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &ClosePromptBar, _, cx| cx.emit(PromptBarEvent::Close)))
            .w_full()
            .child(self.render_bar(cx))
            .children(card)
    }
}
