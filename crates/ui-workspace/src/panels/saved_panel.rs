//! Sidebar: saved queries in nested folders. Click opens a query in a tab; drag a query onto a folder
//! (or the list background) to move it.

use std::collections::HashSet;

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelControl};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::*;
use storage::{FolderRow, SavedQueryRow};

use super::{WorkspacePanel, empty_state, panel_basics};
use crate::connections::ConnectionManager;
use crate::saved::SavedLibrary;
use crate::theme::Tokens;

/// A saved query being dragged.
#[derive(Clone)]
pub struct DraggedQuery {
    pub id: String,
    pub name: String,
}

struct DragPreview(String);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .rounded(Tokens::RADIUS_SM)
            .bg(cx.theme().popover)
            .border(Tokens::BORDER)
            .border_color(cx.theme().border)
            .text_size(Tokens::text_sm())
            .text_color(cx.theme().foreground)
            .child(self.0.clone())
    }
}

pub struct SavedPanel {
    focus_handle: FocusHandle,
    collapsed: HashSet<String>,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(SavedPanel);

impl WorkspacePanel for SavedPanel {
    const NAME: &'static str = "gremliner.saved";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Left;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let mut subscriptions = Vec::new();
            if let Some(library) = SavedLibrary::try_global(cx) {
                subscriptions.push(cx.observe(&library, |_, _, cx| cx.notify()));
            }
            Self {
                focus_handle: cx.focus_handle(),
                collapsed: HashSet::new(),
                _subscriptions: subscriptions,
            }
        })
    }
}

/// Ask for a name, then call `done` with it.
pub fn ask_name(
    title: &'static str,
    initial: &str,
    window: &mut Window,
    cx: &mut App,
    done: impl Fn(String, &mut App) + 'static,
) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(initial.to_owned()));
    let done = std::rc::Rc::new(done);
    let focus = input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let input = input.clone();
        let ok_input = input.clone();
        let done = done.clone();
        dialog
            .title(title)
            .w(Tokens::SHORTCUTS_WIDTH)
            .margin_top(Tokens::DIALOG_TOP)
            .child(
                v_flex()
                    .id("name-dialog")
                    .test_support()
                    .gap(Tokens::space(3.0))
                    .child(Input::new(&input))
                    .child(
                        h_flex()
                            .justify_end()
                            .gap(Tokens::space(2.0))
                            .child(
                                Button::new("name-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(Button::new("name-ok").label("OK").primary().on_click(
                                move |_, window, cx| {
                                    let name = ok_input.read(cx).value().trim().to_owned();
                                    if !name.is_empty() {
                                        done(name, cx);
                                    }
                                    window.close_dialog(cx);
                                },
                            )),
                    ),
            )
    });
    window.defer(cx, move |window, cx| {
        focus.update(cx, |i, cx| i.focus(window, cx))
    });
}

impl SavedPanel {
    /// Open saved query `id` (focusing its tab if one is already open).
    pub fn open_query(id: &str, window: &mut Window, cx: &mut App) {
        let Some(row) = SavedLibrary::global(cx).read(cx).query(id).cloned() else {
            return;
        };
        window.defer(cx, move |window, cx| {
            if let Some(ws) = crate::Workspace::for_window(window, cx) {
                ws.update(cx, |ws, cx| ws.open_saved(&row, window, cx));
            }
        });
    }

    fn delete_folder(id: String, name: String, window: &mut Window, cx: &mut App) {
        let library = SavedLibrary::global(cx);
        if !library.read(cx).folder_has_contents(&id) {
            library.update(cx, |l, cx| l.delete_folder(&id, cx));
            return;
        }
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let id = id.clone();
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Delete the folder \u{201c}{name}\u{201d}?"))
                .description(
                    "Its saved queries and subfolders are deleted too. This cannot be undone.",
                )
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let id = id.clone();
                    SavedLibrary::global(cx).update(cx, |l, cx| l.delete_folder(&id, cx));
                    true
                })
        });
    }

    fn query_row(&self, q: &SavedQueryRow, depth: usize, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let connection = q
            .connection_id
            .as_deref()
            .and_then(|c| uuid::Uuid::parse_str(c).ok())
            .and_then(|c| {
                ConnectionManager::try_global(cx)?
                    .read(cx)
                    .get(c)
                    .map(|c| c.name.clone())
            });
        let id = q.id.clone();
        let open_id = id.clone();
        let rename = (id.clone(), q.name.clone());
        let delete = id.clone();
        h_flex()
            .id(SharedString::from(format!("saved-q-{id}")))
            .test_support()
            .group("saved-row")
            .gap(Tokens::space(1.5))
            .pl(Tokens::space(2.0 + 4.0 * depth as f32))
            .pr(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .text_size(Tokens::text_sm())
            .cursor_pointer()
            .hover(|s| s.bg(theme.accent))
            .on_drag(
                DraggedQuery {
                    id: id.clone(),
                    name: q.name.clone(),
                },
                |d, _, _, cx| cx.new(|_| DragPreview(d.name.clone())),
            )
            .on_click(move |_, window, cx| SavedPanel::open_query(&open_id, window, cx))
            .child(
                Icon::new(IconName::File)
                    .xsmall()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(q.name.clone()),
            )
            .children(connection.map(|c| {
                div()
                    .flex_none()
                    .text_size(Tokens::text_xs())
                    .text_color(theme.muted_foreground)
                    .child(c)
            }))
            .child(
                h_flex()
                    .invisible()
                    .group_hover("saved-row", |s| s.visible())
                    .child(
                        Button::new(SharedString::from(format!("saved-rename-q-{id}")))
                            .icon(IconName::Replace)
                            .ghost()
                            .xsmall()
                            .tooltip("Rename")
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let (id, name) = rename.clone();
                                ask_name("Rename query", &name, window, cx, move |new, cx| {
                                    SavedLibrary::global(cx)
                                        .update(cx, |l, cx| l.rename_query(&id, &new, cx));
                                });
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("saved-delete-q-{id}")))
                            .icon(IconName::Delete)
                            .ghost()
                            .xsmall()
                            .tooltip("Delete")
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                SavedLibrary::global(cx)
                                    .update(cx, |l, cx| l.delete_query(&delete, cx));
                            }),
                    ),
            )
            .into_any_element()
    }

    fn folder_rows(
        &self,
        parent: Option<&str>,
        depth: usize,
        folders: &[FolderRow],
        queries: &[SavedQueryRow],
        out: &mut Vec<AnyElement>,
        cx: &mut Context<'_, Self>,
    ) {
        let theme = cx.theme().clone();
        for f in folders.iter().filter(|f| f.parent_id.as_deref() == parent) {
            let id = f.id.clone();
            let collapsed = self.collapsed.contains(&id);
            let toggle = id.clone();
            let drop_into = id.clone();
            let rename = (id.clone(), f.name.clone());
            let delete = (id.clone(), f.name.clone());
            out.push(
                h_flex()
                    .id(SharedString::from(format!("saved-folder-{id}")))
                    .test_support()
                    .group("saved-row")
                    .gap(Tokens::space(1.5))
                    .pl(Tokens::space(2.0 + 4.0 * depth as f32))
                    .pr(Tokens::space(2.0))
                    .py(Tokens::space(1.0))
                    .text_size(Tokens::text_sm())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent))
                    .drag_over::<DraggedQuery>(|s, _, _, cx| s.bg(cx.theme().accent))
                    .on_drop(cx.listener(move |_, d: &DraggedQuery, _, cx| {
                        // The list background accepts drops too; this one wins.
                        cx.stop_propagation();
                        let folder = Some(drop_into.clone());
                        SavedLibrary::global(cx)
                            .update(cx, |l, cx| l.move_query(&d.id, folder, cx));
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&toggle) {
                            this.collapsed.insert(toggle.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .xsmall()
                        .text_color(theme.muted_foreground),
                    )
                    .child(
                        Icon::new(IconName::Folder)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child(f.name.clone()),
                    )
                    .child(
                        h_flex()
                            .invisible()
                            .group_hover("saved-row", |s| s.visible())
                            .child(
                                Button::new(SharedString::from(format!("saved-new-sub-{id}")))
                                    .icon(IconName::Plus)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("New folder inside")
                                    .on_click({
                                        let parent = id.clone();
                                        move |_, window, cx| {
                                            cx.stop_propagation();
                                            let parent = parent.clone();
                                            ask_name(
                                                "New folder",
                                                "New folder",
                                                window,
                                                cx,
                                                move |name, cx| {
                                                    let parent = Some(parent.clone());
                                                    SavedLibrary::global(cx).update(cx, |l, cx| {
                                                        l.create_folder(&name, parent, cx)
                                                    });
                                                },
                                            );
                                        }
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "saved-rename-folder-{id}"
                                )))
                                .icon(IconName::Replace)
                                .ghost()
                                .xsmall()
                                .tooltip("Rename")
                                .on_click(
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        let (id, name) = rename.clone();
                                        ask_name(
                                            "Rename folder",
                                            &name,
                                            window,
                                            cx,
                                            move |new, cx| {
                                                SavedLibrary::global(cx).update(cx, |l, cx| {
                                                    l.rename_folder(&id, &new, cx)
                                                });
                                            },
                                        );
                                    },
                                ),
                            )
                            .child(
                                Button::new(SharedString::from(format!(
                                    "saved-delete-folder-{id}"
                                )))
                                .icon(IconName::Delete)
                                .ghost()
                                .xsmall()
                                .tooltip("Delete")
                                .on_click(
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        let (id, name) = delete.clone();
                                        SavedPanel::delete_folder(id, name, window, cx);
                                    },
                                ),
                            ),
                    )
                    .into_any_element(),
            );
            if collapsed {
                continue;
            }
            self.folder_rows(Some(&id), depth + 1, folders, queries, out, cx);
            for q in queries
                .iter()
                .filter(|q| q.folder_id.as_deref() == Some(id.as_str()))
            {
                out.push(self.query_row(q, depth + 1, cx));
            }
        }
    }
}

impl BasePanel for SavedPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }
}

impl Panel for SavedPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Saved"
    }

    fn toolbar_buttons(
        &mut self,
        _: &mut Window,
        _: &mut Context<'_, Self>,
    ) -> Option<Vec<Button>> {
        Some(vec![
            Button::new("saved-new-folder")
                .icon(IconName::Folder)
                .ghost()
                .xsmall()
                .tooltip("New folder")
                .on_click(|_, window, cx| {
                    ask_name("New folder", "New folder", window, cx, |name, cx| {
                        SavedLibrary::global(cx)
                            .update(cx, |l, cx| l.create_folder(&name, None, cx));
                    });
                }),
        ])
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

impl Render for SavedPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (folders, queries) = SavedLibrary::try_global(cx)
            .map(|l| {
                let l = l.read(cx);
                (l.folders().to_vec(), l.queries().to_vec())
            })
            .unwrap_or_default();
        let base = v_flex()
            .id("saved-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.sidebar);
        if folders.is_empty() && queries.is_empty() {
            return base.child(empty_state(
                IconName::BookOpen,
                "No saved queries",
                "Press Ctrl+S in a query tab to save it here, with its settings.",
                cx,
            ));
        }
        let mut rows = Vec::new();
        self.folder_rows(None, 0, &folders, &queries, &mut rows, cx);
        for q in queries.iter().filter(|q| q.folder_id.is_none()) {
            rows.push(self.query_row(q, 0, cx));
        }
        base.child(
            v_flex()
                .id("saved-root")
                .test_support()
                .flex_1()
                .overflow_y_scroll()
                .py(Tokens::space(1.0))
                .drag_over::<DraggedQuery>(|s, _, _, cx| s.bg(cx.theme().sidebar_accent))
                .on_drop(cx.listener(|_, d: &DraggedQuery, _, cx| {
                    SavedLibrary::global(cx).update(cx, |l, cx| l.move_query(&d.id, None, cx));
                }))
                .children(rows),
        )
    }
}
