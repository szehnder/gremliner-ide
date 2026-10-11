//! Left sidebar: the connection tree. The `connections` change fills it; for now it shows the empty state.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, v_flex};
use gpui_kit::*;

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::actions::AddConnection;
use crate::theme::Tokens;

pub struct ConnectionsPanel {
    focus_handle: FocusHandle,
}

panel_basics!(ConnectionsPanel);

impl WorkspacePanel for ConnectionsPanel {
    const NAME: &'static str = "gremliner.connections";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Left;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl BasePanel for ConnectionsPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for ConnectionsPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Connections"
    }

    fn toolbar_buttons(
        &mut self,
        _: &mut Window,
        _: &mut Context<'_, Self>,
    ) -> Option<Vec<Button>> {
        Some(vec![
            Button::new("connections-add")
                .icon(IconName::Plus)
                .ghost()
                .xsmall()
                .tooltip("Add connection")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(AddConnection), cx)),
        ])
    }

    fn zoom_control(&self, _: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        None
    }
}

impl Render for ConnectionsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        v_flex()
            .id("connections-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().sidebar)
            .child(
                empty_state(
                    crate::assets::icons::IconName::Database,
                    "No connections",
                    "Connect to Gremlin Server or Amazon Neptune to browse its graph.",
                    cx,
                )
                .child(
                    div().mt(Tokens::space(2.0)).child(
                        Button::new("sidebar-add-connection")
                            .label("Add connection")
                            .icon(IconName::Plus)
                            .small()
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(AddConnection), cx)
                            }),
                    ),
                ),
            )
    }
}
