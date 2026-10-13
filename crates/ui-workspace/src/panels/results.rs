//! Bottom dock: query results. Shows the latest run's `ResultsView`, or an empty state before any run.

use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, IconName, h_flex};
use gpui_kit::*;
use ui_results::{ResultsView, ResultsViewEvent};

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::run::{Runner, RunnerEvent};
use crate::theme::Tokens;

pub struct ResultsPanel {
    focus_handle: FocusHandle,
    view: Entity<ResultsView>,
    has_result: bool,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(ResultsPanel);

impl WorkspacePanel for ResultsPanel {
    const NAME: &'static str = "gremliner.results";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Bottom;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let view = cx.new(|_| ResultsView::new());
            let runner = Runner::global(cx);
            let subscriptions = vec![
                cx.subscribe_in(
                    &runner,
                    window,
                    |this: &mut Self, runner, event: &RunnerEvent, window, cx| {
                        if *event == RunnerEvent::Started
                            && let Some(result) = runner.read(cx).latest().cloned()
                        {
                            this.has_result = true;
                            let graph = runner.read(cx).latest_graph().cloned();
                            this.view.update(cx, |v, cx| {
                                if let Some(graph) = graph {
                                    v.set_graph(graph, window, cx);
                                }
                                v.set_result(result, window, cx)
                            });
                            cx.notify();
                        }
                    },
                ),
                cx.subscribe_in(
                    &view,
                    window,
                    |_, _, event: &ResultsViewEvent, window, cx| match event {
                        ResultsViewEvent::CancelRequested => {
                            Runner::global(cx).update(cx, |r, cx| r.cancel(cx));
                        }
                        ResultsViewEvent::RetryRequested => {
                            Runner::global(cx).update(cx, |r, cx| r.retry(window, cx));
                        }
                        ResultsViewEvent::FixRequested => {
                            window.dispatch_action(Box::new(crate::actions::FixQueryError), cx);
                        }
                        // A profile step: show where it came from in the tab that ran.
                        ResultsViewEvent::HighlightSource(range) => {
                            let Some(tab_id) = Runner::global(cx).read(cx).latest_tab() else {
                                return;
                            };
                            let tabs = crate::panels::EditorTabs::global(cx).read(cx).tabs();
                            if let Some(tab) = tabs.into_iter().find(|t| t.read(cx).id() == tab_id)
                            {
                                let editor = tab.read(cx).editor().clone();
                                let ranges = range.iter().cloned().collect();
                                editor.update(cx, |e, cx| e.set_hint_ranges(ranges, cx));
                            }
                        }
                    },
                ),
            ];
            // A panel rebuilt (e.g. after a layout reset) shows the latest result right away.
            let latest = runner.read(cx).latest().cloned();
            let has_result = latest.is_some();
            let graph = runner.read(cx).latest_graph().cloned();
            if let Some(result) = latest {
                view.update(cx, |v, cx| {
                    if let Some(graph) = graph {
                        v.set_graph(graph, window, cx);
                    }
                    v.set_result(result, window, cx)
                });
            }
            Self {
                focus_handle: cx.focus_handle(),
                view,
                has_result,
                _subscriptions: subscriptions,
            }
        })
    }
}

impl ResultsPanel {
    pub fn view(&self) -> &Entity<ResultsView> {
        &self.view
    }
}

impl BasePanel for ResultsPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for ResultsPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Results"
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for ResultsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let content: AnyElement = if self.has_result {
            self.view.clone().into_any_element()
        } else {
            let keys = crate::shortcuts::keystrokes_for(&crate::actions::editor::RunQuery, cx);
            empty_state(
                IconName::LayoutDashboard,
                "No results yet",
                "Run a query to see vertices, edges and values here as a graph, table, JSON or console output.",
                cx,
            )
            .children(keys.map(|keys| {
                h_flex()
                    .id("results-empty-shortcut")
                    .test_support()
                    .gap(Tokens::space(1.0))
                    .items_center()
                    .text_size(Tokens::text_sm())
                    .text_color(cx.theme().muted_foreground)
                    .child("Run the query at the cursor with")
                    .children(keys.into_iter().map(Kbd::new))
            }))
            .into_any_element()
        };
        div()
            .id("results-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .child(content)
    }
}
