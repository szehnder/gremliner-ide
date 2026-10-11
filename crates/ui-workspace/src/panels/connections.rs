//! Left sidebar: saved connections with status, search and actions.

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelControl};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use providers::ConnectionConfig;
use uuid::Uuid;

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::actions::AddConnection;
use crate::connections::style::{color_tag_color, environment_badge, status_dot};
use crate::connections::{ConnStatus, ConnectionManager, form};
use crate::theme::Tokens;

pub struct ConnectionsPanel {
    focus_handle: FocusHandle,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(ConnectionsPanel);

impl WorkspacePanel for ConnectionsPanel {
    const NAME: &'static str = "gremliner.connections";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Left;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search connections"));
            let manager = ConnectionManager::global(cx);
            let subscriptions = vec![
                cx.observe(&manager, |_, _, cx| cx.notify()),
                cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }),
            ];
            Self {
                focus_handle: cx.focus_handle(),
                search,
                _subscriptions: subscriptions,
            }
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

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

/// Ask, then delete the connection and its stored password.
pub fn confirm_delete(id: Uuid, name: String, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, cx| {
        let name = name.clone();
        alert
            .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
            .title(format!("Delete \u{201c}{name}\u{201d}?"))
            .description("Its saved password is removed from the system keychain too. This cannot be undone.")
            .ok_text("Delete")
            .ok_variant(ButtonVariant::Danger)
            .show_cancel(true)
            .on_ok(move |_, window, cx| {
                let manager = ConnectionManager::global(cx);
                let task = manager.update(cx, |m, cx| m.delete(id, cx));
                let handle = window.window_handle();
                cx.spawn(async move |cx| {
                    if let Err(err) = task.await {
                        let _ = handle.update(cx, |_, window, cx| {
                            window.push_notification(
                                Notification::error(err).title("Could not delete the connection"),
                                cx,
                            );
                        });
                    }
                })
                .detach();
                true
            })
    });
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

impl ConnectionsPanel {
    fn row(
        &self,
        config: &ConnectionConfig,
        status: ConnStatus,
        active: bool,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = config.id;
        let slug = slug(&config.name);
        let connected = matches!(status, ConnStatus::Connected(_) | ConnStatus::Connecting);
        let action = |name: &str, icon: IconName, tip: &'static str| {
            Button::new(SharedString::from(format!("conn-{name}-{slug}")))
                .icon(icon)
                .ghost()
                .xsmall()
                .tooltip(tip)
        };
        let group = SharedString::from(format!("conn-row-group-{slug}"));
        let edit_config = config.clone();
        let error_hint = match &status {
            ConnStatus::Error(err) => Some(err.reason.clone()),
            _ => None,
        };
        h_flex()
            .id(SharedString::from(format!("conn-row-{slug}")))
            .test_support()
            .group(group.clone())
            .relative()
            .gap(Tokens::space(2.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.5))
            .mx(Tokens::space(1.0))
            .rounded(Tokens::RADIUS_MD)
            .cursor_pointer()
            .when(active, |row| row.bg(theme.sidebar_accent))
            .hover(|s| s.bg(theme.sidebar_accent))
            .on_click(cx.listener(move |_, event: &ClickEvent, window, cx| {
                if event.click_count() >= 2 {
                    let config = ConnectionManager::global(cx).read(cx).get(id).cloned();
                    form::open(config, window, cx);
                } else {
                    ConnectionManager::global(cx)
                        .update(cx, |m, cx| m.connect(id, cx))
                        .detach();
                }
            }))
            .child(
                div()
                    .id(SharedString::from(format!("conn-status-{slug}")))
                    .test_support()
                    .child(status_dot(&status, cx)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap(Tokens::space(1.5))
                            .children(color_tag_color(config.color, cx).map(|c| {
                                div()
                                    .flex_none()
                                    .w(Tokens::size(3.0))
                                    .h(Tokens::size(12.0))
                                    .rounded_full()
                                    .bg(c)
                            }))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(theme.sidebar_foreground)
                                    .child(config.name.clone()),
                            )
                            .children(environment_badge(config.environment, cx)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(Tokens::text_xs())
                            .text_color(if error_hint.is_some() {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .child(error_hint.unwrap_or_else(|| config.endpoint_label())),
                    ),
            )
            .child(
                // Overlaid on hover, so names keep the full row width otherwise.
                h_flex()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(Tokens::space(1.0))
                    .pl(Tokens::space(2.0))
                    .bg(theme.sidebar_accent)
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .child(if connected {
                        action("disconnect", IconName::CircleX, "Disconnect").on_click(cx.listener(
                            move |_, _, _, cx| {
                                cx.stop_propagation();
                                ConnectionManager::global(cx)
                                    .update(cx, |m, cx| m.disconnect(id, cx));
                            },
                        ))
                    } else {
                        action("connect", IconName::Play, "Connect").on_click(cx.listener(
                            move |_, _, _, cx| {
                                cx.stop_propagation();
                                ConnectionManager::global(cx)
                                    .update(cx, |m, cx| m.connect(id, cx))
                                    .detach();
                            },
                        ))
                    })
                    .child(
                        action("edit", IconName::Settings2, "Edit").on_click(cx.listener(
                            move |_, _, window, cx| {
                                cx.stop_propagation();
                                form::open(Some(edit_config.clone()), window, cx);
                            },
                        )),
                    )
                    .child(
                        action("duplicate", IconName::Copy, "Duplicate").on_click(cx.listener(
                            move |_, _, _, cx| {
                                cx.stop_propagation();
                                ConnectionManager::global(cx)
                                    .update(cx, |m, cx| m.duplicate(id, cx))
                                    .detach();
                            },
                        )),
                    )
                    .child({
                        let name = config.name.clone();
                        action("delete", IconName::Delete, "Delete").on_click(cx.listener(
                            move |_, _, window, cx| {
                                cx.stop_propagation();
                                confirm_delete(id, name.clone(), window, cx);
                            },
                        ))
                    }),
            )
    }
}

impl Render for ConnectionsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let manager = ConnectionManager::global(cx);
        let (connections, active) = {
            let m = manager.read(cx);
            let rows: Vec<(ConnectionConfig, ConnStatus)> = m
                .connections()
                .iter()
                .map(|c| (c.clone(), m.status(c.id)))
                .collect();
            (rows, m.active().map(|c| c.id))
        };
        let base = v_flex()
            .id("connections-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().sidebar);

        if connections.is_empty() {
            return base.child(
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
            );
        }

        let query = self.search.read(cx).value().trim().to_lowercase();
        let visible: Vec<_> = connections
            .into_iter()
            .filter(|(c, _)| query.is_empty() || c.name.to_lowercase().contains(&query))
            .collect();
        let none_match = visible.is_empty();
        base.child(
            div()
                .px(Tokens::space(2.0))
                .pt(Tokens::space(2.0))
                .pb(Tokens::space(1.0))
                .child(
                    Input::new(&self.search)
                        .small()
                        .prefix(Icon::new(IconName::Search).xsmall()),
                ),
        )
        .child(
            v_flex()
                .id("connections-list")
                .flex_1()
                .overflow_y_scroll()
                .gap(Tokens::space(0.5))
                .py(Tokens::space(1.0))
                .children(visible.into_iter().map(|(config, status)| {
                    let is_active = active == Some(config.id);
                    self.row(&config, status, is_active, cx).into_any_element()
                }))
                .when(none_match, |list| {
                    list.child(
                        div()
                            .p(Tokens::space(3.0))
                            .text_size(Tokens::text_sm())
                            .text_color(cx.theme().muted_foreground)
                            .child("No connections match."),
                    )
                }),
        )
    }
}
