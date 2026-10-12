//! Right panel: the inspector for the selected element of the latest graph (`ui_results::inspector`).

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::*;
use ui_results::inspector::Inspector;

use super::{WorkspacePanel, panel_basics};
use crate::run::{Runner, RunnerEvent};

pub struct InspectorPanel {
    focus_handle: FocusHandle,
    inspector: Entity<Inspector>,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(InspectorPanel);

impl WorkspacePanel for InspectorPanel {
    const NAME: &'static str = "gremliner.inspector";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Right;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let inspector = cx.new(|cx| Inspector::new(window, cx));
            let runner = Runner::global(cx);
            // Follow the graph of the tab that ran last.
            if let Some(graph) = runner.read(cx).latest_graph().cloned() {
                inspector.update(cx, |i, cx| i.set_graph(graph, cx));
            }
            let target = inspector.clone();
            let subscriptions =
                vec![
                    cx.subscribe(&runner, move |_, runner, event: &RunnerEvent, cx| {
                        if *event == RunnerEvent::Started
                            && let Some(graph) = runner.read(cx).latest_graph().cloned()
                        {
                            target.update(cx, |i, cx| i.set_graph(graph, cx));
                        }
                    }),
                ];
            Self {
                focus_handle: cx.focus_handle(),
                inspector,
                _subscriptions: subscriptions,
            }
        })
    }
}

impl InspectorPanel {
    pub fn inspector(&self) -> &Entity<Inspector> {
        &self.inspector
    }
}

impl BasePanel for InspectorPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for InspectorPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Inspector"
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl Render for InspectorPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("inspector-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().sidebar)
            .child(self.inspector.clone())
    }
}
