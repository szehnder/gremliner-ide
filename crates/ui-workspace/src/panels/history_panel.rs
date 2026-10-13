//! Sidebar: the query history, searchable by substring and filterable by connection and outcome. Each
//! entry can be opened in a new tab or run again on its connection.

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelControl};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use storage::{HistoryEntry, HistoryQuery, Outcome};
use uuid::Uuid;

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::connections::ConnectionManager;
use crate::saved::HistoryLog;
use crate::theme::Tokens;

/// Entries listed at once.
const SHOWN: usize = 300;

pub struct HistoryPanel {
    focus_handle: FocusHandle,
    pub search: Entity<InputState>,
    pub connection: Option<Uuid>,
    pub outcome: Option<Outcome>,
    entries: Vec<HistoryEntry>,
    /// The search the entries are for (stale answers are dropped).
    generation: u64,
    _load: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(HistoryPanel);

impl WorkspacePanel for HistoryPanel {
    const NAME: &'static str = "gremliner.history";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Left;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search history"));
            let mut subscriptions =
                vec![
                    cx.subscribe(&search, |this: &mut Self, _, e: &InputEvent, cx| {
                        if matches!(e, InputEvent::Change) {
                            this.reload(cx);
                        }
                    }),
                ];
            if let Some(log) = HistoryLog::global(cx) {
                subscriptions.push(cx.observe(&log, |this, _, cx| this.reload(cx)));
            }
            let mut this = Self {
                focus_handle: cx.focus_handle(),
                search,
                connection: None,
                outcome: None,
                entries: Vec::new(),
                generation: 0,
                _load: None,
                _subscriptions: subscriptions,
            };
            this.reload(cx);
            this
        })
    }
}

fn ago(ms: i64) -> String {
    let secs = (HistoryLog::now_ms() - ms).max(0) / 1000;
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

impl HistoryPanel {
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Run the search on the storage thread.
    pub fn reload(&mut self, cx: &mut Context<'_, Self>) {
        let Some(storage) = HistoryLog::storage(cx) else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let query = HistoryQuery {
            text: self.search.read(cx).value().to_string(),
            connection_id: self.connection.map(|c| c.to_string()),
            outcome: self.outcome,
            limit: SHOWN,
        };
        let read = storage.search_history(query);
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(entries) => this.entries = entries,
                    Err(err) => log::warn!("history search failed: {err}"),
                }
                cx.notify();
            });
        }));
    }

    pub fn set_outcome(&mut self, outcome: Option<Outcome>, cx: &mut Context<'_, Self>) {
        self.outcome = outcome;
        self.reload(cx);
    }

    pub fn set_connection(&mut self, connection: Option<Uuid>, cx: &mut Context<'_, Self>) {
        self.connection = connection;
        self.reload(cx);
    }

    pub fn focus_search(&self, window: &mut Window, cx: &mut App) {
        self.search.update(cx, |s, cx| s.focus(window, cx));
    }

    fn open(entry: &HistoryEntry, run: bool, window: &mut Window, cx: &mut App) {
        let text = entry.text.clone();
        let connection = entry
            .connection_id
            .as_deref()
            .and_then(|c| Uuid::parse_str(c).ok());
        window.defer(cx, move |window, cx| {
            if let Some(ws) = crate::Workspace::for_window(window, cx) {
                ws.update(cx, |ws, cx| {
                    if run {
                        ws.open_and_run(&text, connection, window, cx);
                    } else {
                        ws.open_query(&text, connection, window, cx);
                    }
                });
            }
        });
    }

    pub fn clear(window: &mut Window, cx: &mut App) {
        window.open_alert_dialog(cx, |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title("Clear the query history?")
                .description("Every recorded query is removed. Saved queries are kept.")
                .ok_text("Clear history")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(|_, _, cx| {
                    if let (Some(storage), Some(log)) =
                        (HistoryLog::storage(cx), HistoryLog::global(cx))
                    {
                        let clear = storage.clear_history();
                        cx.spawn(async move |cx| {
                            if clear.await.is_ok() {
                                log.update(cx, |l, cx| {
                                    l.version += 1;
                                    cx.notify();
                                });
                            }
                        })
                        .detach();
                    }
                    true
                })
        });
    }
}

impl BasePanel for HistoryPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }
}

impl Panel for HistoryPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "History"
    }

    fn toolbar_buttons(
        &mut self,
        _: &mut Window,
        _: &mut Context<'_, Self>,
    ) -> Option<Vec<Button>> {
        Some(vec![
            Button::new("history-clear")
                .icon(IconName::Delete)
                .ghost()
                .xsmall()
                .tooltip("Clear history")
                .on_click(|_, window, cx| HistoryPanel::clear(window, cx)),
        ])
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

impl Render for HistoryPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let manager = ConnectionManager::try_global(cx);
        let connections: Vec<(Uuid, String)> = manager
            .as_ref()
            .map(|m| {
                m.read(cx)
                    .connections()
                    .iter()
                    .map(|c| (c.id, c.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let current_conn = self
            .connection
            .and_then(|id| {
                connections
                    .iter()
                    .find(|(c, _)| *c == id)
                    .map(|(_, n)| n.clone())
            })
            .unwrap_or_else(|| "All connections".into());
        let this = cx.entity().downgrade();
        let conn_menu = Button::new("history-connection")
            .label(current_conn)
            .icon(IconName::ChevronDown)
            .ghost()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                let pick = |c: Option<Uuid>, this: WeakEntity<HistoryPanel>| {
                    move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let _ = this.update(cx, |p, cx| p.set_connection(c, cx));
                    }
                };
                menu = menu
                    .item(PopupMenuItem::new("All connections").on_click(pick(None, this.clone())));
                for (id, name) in &connections {
                    menu = menu.item(
                        PopupMenuItem::new(name.clone()).on_click(pick(Some(*id), this.clone())),
                    );
                }
                menu
            });
        let outcomes = h_flex().gap(Tokens::space(0.5)).children(
            [
                (None, "all", "All"),
                (Some(Outcome::Success), "success", "OK"),
                (Some(Outcome::Error), "error", "Errors"),
                (Some(Outcome::Cancelled), "cancelled", "Cancelled"),
            ]
            .into_iter()
            .map(|(o, id, label)| {
                Button::new(SharedString::from(format!("history-outcome-{id}")))
                    .label(label)
                    .xsmall()
                    .when(o == self.outcome, |b| b.primary())
                    .when(o != self.outcome, |b| b.ghost())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_outcome(o, cx)))
            }),
        );
        let rows = self.entries.iter().map(|e| {
            let (icon, color) = match e.outcome {
                Outcome::Success => (IconName::CircleCheck, theme.success),
                Outcome::Error => (IconName::CircleX, theme.danger),
                Outcome::Cancelled => (IconName::Ban, theme.warning),
            };
            let open = e.clone();
            let run = e.clone();
            let id = e.id;
            v_flex()
                .id(SharedString::from(format!("history-row-{id}")))
                .test_support()
                .group("history-row")
                .gap(Tokens::space(0.5))
                .px(Tokens::space(2.0))
                .py(Tokens::space(1.5))
                .border_b(Tokens::BORDER)
                .border_color(theme.border)
                .hover(|s| s.bg(theme.accent))
                .child(
                    div()
                        .font_family(ui_theme::fonts::MONO_FONT_FAMILY)
                        .text_size(Tokens::text_sm())
                        .text_color(theme.foreground)
                        .line_clamp(2)
                        .child(e.text.clone()),
                )
                .child(
                    h_flex()
                        .gap(Tokens::space(1.5))
                        .text_size(Tokens::text_xs())
                        .text_color(theme.muted_foreground)
                        .child(Icon::new(icon).xsmall().text_color(color))
                        .child(div().flex_1().min_w_0().truncate().child(format!(
                            "{} · {} · {} ms · {} items",
                            e.connection_name,
                            ago(e.started_at),
                            e.duration_ms,
                            e.count
                        )))
                        .child(
                            h_flex()
                                .invisible()
                                .group_hover("history-row", |s| s.visible())
                                .child(
                                    Button::new(SharedString::from(format!("history-open-{id}")))
                                        .label("Open")
                                        .ghost()
                                        .xsmall()
                                        .on_click(move |_, window, cx| {
                                            HistoryPanel::open(&open, false, window, cx)
                                        }),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("history-run-{id}")))
                                        .label("Run again")
                                        .ghost()
                                        .xsmall()
                                        .on_click(move |_, window, cx| {
                                            HistoryPanel::open(&run, true, window, cx)
                                        }),
                                ),
                        ),
                )
        });
        let empty = self.entries.is_empty();
        let searching = !self.search.read(cx).value().is_empty()
            || self.outcome.is_some()
            || self.connection.is_some();
        v_flex()
            .id("history-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.sidebar)
            .child(
                v_flex()
                    .gap(Tokens::space(1.5))
                    .p(Tokens::space(2.0))
                    .border_b(Tokens::BORDER)
                    .border_color(theme.border)
                    .child(Input::new(&self.search).small())
                    .child(h_flex().justify_between().child(outcomes).child(conn_menu)),
            )
            .when(empty, |el| {
                el.child(empty_state(
                    IconName::Inbox,
                    if searching {
                        "No matching queries"
                    } else {
                        "No history yet"
                    },
                    if searching {
                        "Try a shorter search or another filter."
                    } else {
                        "Every query you run is recorded here."
                    },
                    cx,
                ))
            })
            .when(!empty, |el| {
                el.child(
                    v_flex()
                        .id("history-list")
                        .flex_1()
                        .overflow_y_scroll()
                        .children(rows),
                )
            })
    }
}
