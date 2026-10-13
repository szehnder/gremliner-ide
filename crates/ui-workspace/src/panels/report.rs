//! "Run as report" (query-parameters): a saved query as a compact form of its parameters (labels and
//! defaults) with a Run button and the results below. Several reports of one query can be open.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelState};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use storage::SavedQueryRow;
use ui_results::ResultsView;
use uuid::Uuid;

use super::{RunRequest, WorkspacePanel, panel_basics};
use crate::run::{Runner, RunnerEvent};
use crate::saved::TabSettings;
use crate::theme::Tokens;

pub struct ReportPanel {
    /// Also the runner's tab id for this report's runs.
    id: Uuid,
    query: SavedQueryRow,
    settings: TabSettings,
    fields: Vec<(String, Entity<InputState>)>,
    results: Entity<ResultsView>,
    has_result: bool,
    pub error: Option<String>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(ReportPanel);

impl WorkspacePanel for ReportPanel {
    const NAME: &'static str = "gremliner.report";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        // Reports are not restored across restarts; an empty one stands in for a stale layout entry.
        Self::new(
            SavedQueryRow {
                id: String::new(),
                folder_id: None,
                connection_id: None,
                name: "Report".into(),
                text: String::new(),
                settings: "{}".into(),
                updated_at: 0,
            },
            window,
            cx,
        )
    }
}

impl ReportPanel {
    pub fn new(query: SavedQueryRow, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let settings = TabSettings::from_json(Some(&query.settings));
            let fields = gremlin_syntax::params::parameters(&query.text)
                .into_iter()
                .map(|name| {
                    let def = settings.param(&name).cloned().unwrap_or_default();
                    let value = def.default.clone();
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder(def.display().to_owned())
                            .default_value(value)
                    });
                    (name, input)
                })
                .collect();
            let runner = Runner::global(cx);
            let subscriptions = vec![cx.subscribe_in(
                &runner,
                window,
                |this: &mut Self, runner, event: &RunnerEvent, window, cx| {
                    if *event != RunnerEvent::Started
                        || runner.read(cx).latest_tab() != Some(this.id)
                    {
                        return;
                    }
                    let Some(result) = runner.read(cx).latest().cloned() else {
                        return;
                    };
                    let graph = runner.read(cx).latest_graph().cloned();
                    this.has_result = true;
                    this.results.update(cx, |v, cx| {
                        if let Some(g) = graph {
                            v.set_graph(g, window, cx);
                        }
                        v.set_result(result, window, cx);
                    });
                    cx.notify();
                },
            )];
            Self {
                id: Uuid::new_v4(),
                query,
                settings,
                fields,
                results: cx.new(|_| ResultsView::new()),
                has_result: false,
                error: None,
                focus_handle: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn saved_query_id(&self) -> &str {
        &self.query.id
    }

    pub fn results(&self) -> &Entity<ResultsView> {
        &self.results
    }

    /// Field labels, in parameter order.
    pub fn labels(&self) -> Vec<String> {
        self.fields
            .iter()
            .map(|(name, _)| {
                self.settings
                    .param(name)
                    .map(|p| p.display().to_owned())
                    .unwrap_or_else(|| name.clone())
            })
            .collect()
    }

    pub fn set_field(
        &mut self,
        name: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some((_, input)) = self.fields.iter().find(|(n, _)| n == name) {
            let v = value.to_owned();
            input.update(cx, |s, cx| s.set_value(v, window, cx));
        }
    }

    pub fn run(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let mut settings = self.settings.clone();
        for (name, input) in &self.fields {
            settings.param_mut(name).value = input.read(cx).value().to_string();
        }
        let connection = self
            .query
            .connection_id
            .as_deref()
            .and_then(|c| Uuid::parse_str(c).ok())
            .or_else(|| {
                crate::connections::ConnectionManager::try_global(cx)
                    .and_then(|m| m.read(cx).active().map(|c| c.id))
            });
        let schema = connection.and_then(|id| {
            crate::schema::SchemaRegistry::try_global(cx)?
                .read(cx)
                .schema(id)
        });
        let bindings = match settings.bindings(&self.query.text, schema.as_deref()) {
            Ok(b) => b,
            Err(missing) => {
                self.error = Some(format!("Enter a value for {}.", missing.join(", ")));
                cx.notify();
                return;
            }
        };
        let Some(connection) = connection else {
            self.error = Some("Choose a connection first.".into());
            cx.notify();
            return;
        };
        self.error = None;
        let request = RunRequest {
            tab_id: self.id,
            connection_id: Some(connection),
            text: self.query.text.clone(),
            range: 0..0,
            mode: ui_editor::RunMode::Run,
            generated: true,
            bindings,
            settings,
        };
        Runner::global(cx).update(cx, |r, cx| r.execute(request, window, cx));
        cx.notify();
    }
}

impl BasePanel for ReportPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn dump(&self, _: &App) -> PanelState {
        PanelState::new(Self::NAME)
    }
}

impl Panel for ReportPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        format!("Report \u{b7} {}", self.query.name)
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for ReportPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let labels = self.labels();
        let form = h_flex()
            .id("report-form")
            .test_support()
            .flex_wrap()
            .items_end()
            .gap(Tokens::space(3.0))
            .px(Tokens::space(3.0))
            .py(Tokens::space(2.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .children(
                self.fields
                    .iter()
                    .zip(labels)
                    .map(|((name, input), label)| {
                        v_flex()
                            .id(SharedString::from(format!("report-field-{name}")))
                            .test_support()
                            .gap(Tokens::space(1.0))
                            .w(Tokens::size(200.0))
                            .child(
                                div()
                                    .text_size(Tokens::text_sm())
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .child(label),
                            )
                            .child(Input::new(input).small())
                    }),
            )
            .child(
                Button::new("report-run")
                    .label("Run")
                    .icon(IconName::Play)
                    .primary()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| this.run(window, cx))),
            )
            .children(self.error.clone().map(|e| {
                div()
                    .id("report-error")
                    .test_support()
                    .text_size(Tokens::text_sm())
                    .text_color(theme.danger)
                    .child(e)
            }));
        v_flex()
            .id("report-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.background)
            .child(
                div()
                    .px(Tokens::space(3.0))
                    .pt(Tokens::space(2.0))
                    .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(self.query.text.clone()),
            )
            .child(form)
            .child(div().flex_1().min_h_0().when_else(
                self.has_result,
                |el| el.child(self.results.clone()),
                |el| {
                    el.child(
                        div()
                            .p(Tokens::space(4.0))
                            .text_size(Tokens::text_sm())
                            .text_color(theme.muted_foreground)
                            .child("Fill in the values and press Run."),
                    )
                },
            ))
    }
}
