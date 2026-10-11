//! Bottom dock: query results. `query-execution` fills it with the table/graph/JSON views.

use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::component::{ActiveTheme as _, IconName};
use gpui_kit::*;

use super::{WorkspacePanel, empty_state, panel_basics};

pub struct ResultsPanel {
    focus_handle: FocusHandle,
}

panel_basics!(ResultsPanel);

impl WorkspacePanel for ResultsPanel {
    const NAME: &'static str = "gremliner.results";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Bottom;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
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
}

impl Render for ResultsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("results-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .child(empty_state(
                IconName::LayoutDashboard,
                "No results yet",
                "Run a query to see vertices, edges and values here as a graph, table or JSON.",
                cx,
            ))
    }
}
