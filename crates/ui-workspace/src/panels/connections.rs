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

use std::collections::HashSet;

use schema::{LabelInfo, Schema};

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::actions::AddConnection;
use crate::actions::{OpenSampleQuery, OpenSchemaGraph};
use crate::connections::style::{color_tag_color, environment_badge, status_dot};
use crate::connections::{ConnStatus, ConnectionManager, form};
use crate::schema::SchemaRegistry;
use crate::theme::Tokens;

pub struct ConnectionsPanel {
    focus_handle: FocusHandle,
    search: Entity<InputState>,
    /// Schema tree nodes the user opened (`<connection>/v`, `<connection>/v/<label>`, …).
    expanded: HashSet<String>,
    /// Connections whose schema tree the user collapsed (open by default).
    schema_collapsed: HashSet<Uuid>,
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
            let mut subscriptions = vec![
                cx.observe(&manager, |_, _, cx| cx.notify()),
                cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }),
            ];
            if let Some(registry) = SchemaRegistry::try_global(cx) {
                subscriptions.push(cx.observe(&registry, |_, _, cx| cx.notify()));
            }
            Self {
                focus_handle: cx.focus_handle(),
                search,
                expanded: HashSet::new(),
                schema_collapsed: HashSet::new(),
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
                    let connected = matches!(status, ConnStatus::Connected(_));
                    v_flex()
                        .child(self.row(&config, status, is_active, cx))
                        .when(connected, |col| col.children(self.schema_tree(&config, cx)))
                        .into_any_element()
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

/// How many elements "Sample" opens.
pub const SAMPLE_SIZE: usize = 25;

/// `g.V().hasLabel('airport').limit(25)` (or `g.E()` for edge labels).
pub fn sample_query(label: &str, vertices: bool) -> String {
    let escaped = label.replace('\\', "\\\\").replace('\'', "\\'");
    let source = if vertices { "V" } else { "E" };
    format!("g.{source}().hasLabel('{escaped}').limit({SAMPLE_SIZE})")
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
fn age_text(discovered_at_ms: i64) -> String {
    let now = providers::schema::now_ms();
    let secs = (now - discovered_at_ms).max(0) / 1000;
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86_400),
    }
}

impl ConnectionsPanel {
    fn toggle(&mut self, key: String, cx: &mut Context<'_, Self>) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
        cx.notify();
    }

    /// The schema under a connected connection: a header with status and actions, then vertex and edge
    /// labels with counts, each opening to its properties.
    fn schema_tree(
        &self,
        config: &ConnectionConfig,
        cx: &mut Context<'_, Self>,
    ) -> Option<AnyElement> {
        let registry = SchemaRegistry::try_global(cx)?;
        let entry = registry
            .read(cx)
            .entry(config.id)
            .cloned()
            .unwrap_or_default();
        let theme = cx.theme().clone();
        let id = config.id;
        let slug = slug(&config.name);
        let open = !self.schema_collapsed.contains(&id);
        let status = match (&entry.schema, entry.refreshing) {
            (_, true) => "Discovering\u{2026}".to_owned(),
            (Some(s), false) if s.partial.any() => {
                format!("Partial \u{b7} {}", age_text(s.discovered_at_ms))
            }
            (Some(s), false) => age_text(s.discovered_at_ms),
            (None, false) => "Not discovered".to_owned(),
        };
        let small_button = |name: String, icon: IconName, tip: &'static str| {
            Button::new(SharedString::from(name))
                .icon(icon)
                .ghost()
                .xsmall()
                .tooltip(tip)
        };
        let header = h_flex()
            .id(SharedString::from(format!("schema-header-{slug}")))
            .test_support()
            .gap(Tokens::space(1.0))
            .pl(Tokens::space(5.0))
            .pr(Tokens::space(2.0))
            .py(Tokens::space(0.5))
            .mx(Tokens::space(1.0))
            .rounded(Tokens::RADIUS_MD)
            .cursor_pointer()
            .hover(|s| s.bg(theme.sidebar_accent))
            .text_size(Tokens::text_xs())
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.schema_collapsed.remove(&id) {
                    this.schema_collapsed.insert(id);
                }
                cx.notify();
            }))
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .xsmall()
                .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.sidebar_foreground)
                    .child("Schema"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("schema-status-{slug}")))
                    .test_support()
                    .flex_1()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .child(status),
            )
            .child(
                small_button(
                    format!("schema-refresh-{slug}"),
                    IconName::RefreshCw,
                    "Refresh schema",
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(registry) = SchemaRegistry::try_global(cx) {
                        registry.update(cx, |r, cx| r.refresh(id, cx));
                    }
                })),
            )
            .child(
                small_button(
                    format!("schema-graph-{slug}"),
                    IconName::Network,
                    "Open schema graph",
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(
                        Box::new(OpenSchemaGraph {
                            connection: Some(id),
                        }),
                        cx,
                    );
                }),
            );
        let mut col = v_flex()
            .id(SharedString::from(format!("schema-tree-{slug}")))
            .child(header);
        if open && let Some(schema) = &entry.schema {
            col = col
                .child(self.label_group(config, schema, true, cx))
                .child(self.label_group(config, schema, false, cx));
        }
        Some(col.into_any_element())
    }

    fn label_group(
        &self,
        config: &ConnectionConfig,
        schema: &Schema,
        vertices: bool,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let slug = slug(&config.name);
        let kind = if vertices { "v" } else { "e" };
        let key = format!("{}/{kind}", config.id);
        // Groups are open unless the user closed them.
        let open = !self.expanded.contains(&format!("{key}/closed"));
        let labels: &[LabelInfo] = if vertices {
            &schema.vertex_labels
        } else {
            &schema.edge_labels
        };
        let title = if vertices {
            "Vertex labels"
        } else {
            "Edge labels"
        };
        let toggle_key = format!("{key}/closed");
        let mut col = v_flex().child(
            h_flex()
                .id(SharedString::from(format!("schema-group-{slug}-{kind}")))
                .gap(Tokens::space(1.0))
                .pl(Tokens::space(7.0))
                .py(Tokens::space(0.5))
                .mx(Tokens::space(1.0))
                .rounded(Tokens::RADIUS_MD)
                .cursor_pointer()
                .hover(|s| s.bg(theme.sidebar_accent))
                .text_size(Tokens::text_xs())
                .text_color(theme.muted_foreground)
                .on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle_key.clone(), cx)))
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .xsmall(),
                )
                .child(format!("{title} ({})", labels.len())),
        );
        if !open {
            return col;
        }
        for label in labels {
            col = col.child(self.label_row(config, label, vertices, cx));
        }
        col
    }

    fn label_row(
        &self,
        config: &ConnectionConfig,
        label: &LabelInfo,
        vertices: bool,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = config.id;
        let slug = slug(&config.name);
        let kind = if vertices { "v" } else { "e" };
        let label_slug = self::slug(&label.name);
        let key = format!("{id}/{kind}/{}", label.name);
        let open = self.expanded.contains(&key);
        let group = SharedString::from(format!("schema-label-group-{slug}-{kind}-{label_slug}"));
        let query = sample_query(&label.name, vertices);
        let toggle_key = key.clone();
        let row = h_flex()
            .id(SharedString::from(format!(
                "schema-label-{slug}-{kind}-{label_slug}"
            )))
            .test_support()
            .group(group.clone())
            .relative()
            .gap(Tokens::space(1.0))
            .pl(Tokens::space(9.0))
            .pr(Tokens::space(2.0))
            .py(Tokens::space(0.5))
            .mx(Tokens::space(1.0))
            .rounded(Tokens::RADIUS_MD)
            .cursor_pointer()
            .hover(|s| s.bg(theme.sidebar_accent))
            .text_size(Tokens::text_sm())
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(toggle_key.clone(), cx)))
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .xsmall()
                .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(theme.sidebar_foreground)
                    .child(label.name.clone()),
            )
            .children(label.count.map(|c| {
                div()
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(c.text())
            }))
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(Tokens::space(1.0))
                    .pl(Tokens::space(2.0))
                    .bg(theme.sidebar_accent)
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .child(
                        Button::new(SharedString::from(format!(
                            "schema-sample-{slug}-{kind}-{label_slug}"
                        )))
                        .label(format!("Sample {SAMPLE_SIZE}"))
                        .ghost()
                        .xsmall()
                        .tooltip("Open a sample query in a new tab")
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(
                                Box::new(OpenSampleQuery {
                                    connection: id,
                                    query: query.clone(),
                                }),
                                cx,
                            );
                        }),
                    ),
            );
        let mut col = v_flex().child(row);
        if open {
            for p in &label.properties {
                let multi = if p.cardinality == schema::Cardinality::Multi {
                    " \u{b7} multi"
                } else {
                    ""
                };
                let samples = if p.samples.is_empty() {
                    None
                } else {
                    Some(format!("e.g. {}", p.samples.join(", ")))
                };
                col = col.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "schema-prop-{slug}-{kind}-{label_slug}-{}",
                            self::slug(&p.key)
                        )))
                        .test_support()
                        .gap(Tokens::space(2.0))
                        .pl(Tokens::space(13.0))
                        .pr(Tokens::space(2.0))
                        .py(Tokens::space(0.25))
                        .text_size(Tokens::text_xs())
                        .child(
                            div()
                                .truncate()
                                .text_color(theme.sidebar_foreground)
                                .child(p.key.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(theme.muted_foreground)
                                .child(format!("{}{multi}", p.type_text())),
                        )
                        .children(samples.map(|s| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme.muted_foreground)
                                .child(s)
                        })),
                );
            }
            if label.properties.is_empty() {
                col = col.child(
                    div()
                        .pl(Tokens::space(13.0))
                        .py(Tokens::space(0.25))
                        .text_size(Tokens::text_xs())
                        .text_color(theme.muted_foreground)
                        .child("No properties"),
                );
            }
        }
        col
    }
}
