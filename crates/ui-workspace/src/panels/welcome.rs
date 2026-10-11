//! The editor area's empty state: shown while no editor tab is open.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelControl};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, h_flex, v_flex};
use gpui_kit::*;

use super::{WorkspacePanel, panel_basics};
use crate::actions::{
    AddConnection, OpenCommandPalette, OpenSettings, ShowKeyboardShortcuts, ToggleResultsDock,
};
use crate::assets::icons::IconName as Lucide;
use crate::theme::Tokens;

pub struct WelcomePanel {
    focus_handle: FocusHandle,
}

panel_basics!(WelcomePanel);

impl WorkspacePanel for WelcomePanel {
    const NAME: &'static str = "gremliner.welcome";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus_handle: cx.focus_handle(),
        })
    }
}

impl BasePanel for WelcomePanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn zoomable(&self, _: &App) -> bool {
        false
    }
}

impl Panel for WelcomePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Welcome"
    }

    fn title_bar(&self, _: &App) -> bool {
        false
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

const SHORTCUTS: &[(&str, &str)] = &[
    ("Show all commands", "palette"),
    ("Toggle results", "results"),
    ("Open settings", "settings"),
    ("Keyboard shortcuts", "shortcuts"),
];

fn shortcut_action(key: &str) -> Box<dyn Action> {
    match key {
        "palette" => Box::new(OpenCommandPalette),
        "results" => Box::new(ToggleResultsDock),
        "settings" => Box::new(OpenSettings),
        _ => Box::new(ShowKeyboardShortcuts),
    }
}

impl Render for WelcomePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let hero = div()
            .flex()
            .items_center()
            .justify_center()
            .size(Tokens::hero_size())
            .rounded(Tokens::RADIUS_LG)
            .bg(theme.primary.opacity(0.12))
            .border_1()
            .border_color(theme.primary.opacity(0.35))
            .child(
                Icon::new(Lucide::Waypoints)
                    .size(Tokens::size(28.0))
                    .text_color(theme.primary),
            );

        let heading = v_flex()
            .gap(Tokens::space(1.0))
            .child(
                div()
                    .text_size(Tokens::text_display())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child("Gremliner"),
            )
            .child(
                div()
                    .text_size(Tokens::text_lg())
                    .text_color(theme.muted_foreground)
                    .child("Write, run, visualise and debug Gremlin traversals."),
            );

        let actions = h_flex()
            .gap(Tokens::space(2.0))
            .child(
                Button::new("welcome-add-connection")
                    .primary()
                    .icon(IconName::Plus)
                    .label("Add connection")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(AddConnection), cx)),
            )
            .child(
                Button::new("welcome-open-palette")
                    .outline()
                    .icon(Lucide::Command)
                    .label("Command palette")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(OpenCommandPalette), cx)
                    }),
            );

        let recent = section("Recent connections", cx).child(
            div()
                .id("welcome-recent-empty")
                .text_size(Tokens::text_sm())
                .text_color(theme.muted_foreground)
                .child("Connections you open will appear here."),
        );

        let shortcuts =
            section("Get started", cx).child(v_flex().gap(Tokens::space(0.5)).children(
                SHORTCUTS.iter().map(|(label, key)| {
                    let action = shortcut_action(key);
                    let binding = Kbd::binding_for_action(action.as_ref(), None, window);
                    h_flex()
                        .id(SharedString::from(format!("welcome-shortcut-{key}")))
                        .justify_between()
                        .px(Tokens::space(2.0))
                        .py(Tokens::space(1.5))
                        .rounded(Tokens::RADIUS_MD)
                        .cursor_pointer()
                        .hover(|s| s.bg(cx.theme().accent))
                        .text_color(cx.theme().foreground)
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(action.boxed_clone(), cx)
                        })
                        .child(*label)
                        .children(binding)
                }),
            ));

        div()
            .id("welcome")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(Tokens::welcome_width())
                    .px(Tokens::space(6.0))
                    .pt(Tokens::space(16.0))
                    .pb(Tokens::space(8.0))
                    .gap(Tokens::space(8.0))
                    .child(v_flex().gap(Tokens::space(5.0)).child(hero).child(heading))
                    .child(actions)
                    .child(
                        h_flex()
                            .items_start()
                            .gap(Tokens::space(8.0))
                            .child(recent.flex_1())
                            .child(shortcuts.flex_1()),
                    ),
            )
    }
}

fn section(title: &'static str, cx: &App) -> Div {
    v_flex().gap(Tokens::space(2.0)).child(
        div()
            .text_size(Tokens::text_xs())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title.to_uppercase()),
    )
}
