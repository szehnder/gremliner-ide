//! Right panel: properties of the selected element. `graph-inspector-editing` fills it.

use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::component::{ActiveTheme as _, IconName};
use gpui_kit::*;

use super::{WorkspacePanel, empty_state, panel_basics};

pub struct InspectorPanel {
    focus_handle: FocusHandle,
}

panel_basics!(InspectorPanel);

impl WorkspacePanel for InspectorPanel {
    const NAME: &'static str = "gremliner.inspector";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Right;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
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
}

impl Render for InspectorPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .id("inspector-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().sidebar)
            .child(empty_state(
                IconName::Info,
                "Nothing selected",
                "Select a vertex or edge to see its properties.",
                cx,
            ))
    }
}
