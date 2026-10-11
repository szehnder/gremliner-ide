//! Editor tabs: a `QueryEditor` bound to a connection, persisted in `open_tabs`, restored through the
//! dock layout (which stores only the tab id).

use std::ops::Range;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, DockPlacement, Panel, PanelControl, PanelInfo, PanelState, panel_handle,
    register_panel,
};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use storage::{Storage, TabRow};
use ui_editor::{QueryEditor, QueryEditorEvent};
use uuid::Uuid;

use super::{WorkspacePanel, panel_basics};
use crate::actions::editor::{FormatQuery, RunQuery};
use crate::connections::ConnectionManager;
use crate::connections::style::{color_tag_color, environment_badge};
use crate::settings::AppSettings;
use crate::theme::Tokens;

/// How long edits settle before a tab is written.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// A request to run Gremlin, raised by an editor tab. `query-execution` consumes these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub tab_id: Uuid,
    pub connection_id: Option<Uuid>,
    pub text: String,
    pub range: Range<usize>,
}

#[derive(Debug, Clone)]
pub enum EditorTabsEvent {
    RunRequested(RunRequest),
    /// A tab opened or closed.
    TabsChanged,
}

/// Registry of open editor tabs.
#[derive(Default)]
pub struct EditorTabs {
    tabs: Vec<WeakEntity<EditorPanel>>,
    next_number: usize,
    last_run: Option<RunRequest>,
}

impl EventEmitter<EditorTabsEvent> for EditorTabs {}

struct EditorTabsGlobal(Entity<EditorTabs>);

impl Global for EditorTabsGlobal {}

impl EditorTabs {
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| EditorTabs {
            next_number: 1,
            ..Default::default()
        });
        cx.set_global(EditorTabsGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<EditorTabs> {
        cx.global::<EditorTabsGlobal>().0.clone()
    }

    /// Live tabs, in the order they were opened.
    pub fn tabs(&self) -> Vec<Entity<EditorPanel>> {
        self.tabs.iter().filter_map(WeakEntity::upgrade).collect()
    }

    pub fn any_open(&self) -> bool {
        self.tabs.iter().any(|t| t.upgrade().is_some())
    }

    pub fn last_run(&self) -> Option<&RunRequest> {
        self.last_run.as_ref()
    }

    fn add(&mut self, tab: &Entity<EditorPanel>, cx: &mut Context<'_, Self>) {
        self.tabs.retain(|t| t.upgrade().is_some());
        self.tabs.push(tab.downgrade());
        cx.emit(EditorTabsEvent::TabsChanged);
        cx.notify();
    }

    fn remove(&mut self, id: Uuid, cx: &mut Context<'_, Self>) {
        self.tabs
            .retain(|t| t.upgrade().is_some_and(|t| t.read(cx).id != id));
        cx.emit(EditorTabsEvent::TabsChanged);
        cx.notify();
    }

    fn take_number(&mut self) -> usize {
        let n = self.next_number;
        self.next_number += 1;
        n
    }
}

pub struct EditorPanel {
    id: Uuid,
    title: SharedString,
    editor: Entity<QueryEditor>,
    connection_id: Option<Uuid>,
    saved_text: String,
    dirty: bool,
    storage: Option<Storage>,
    save_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(EditorPanel);

impl WorkspacePanel for EditorPanel {
    const NAME: &'static str = "gremliner.editor";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Center;

    fn build(window: &mut Window, cx: &mut App) -> Entity<Self> {
        Self::new_tab(window, cx)
    }
}

impl EditorPanel {
    /// A fresh tab bound to the most recently used connection.
    pub fn new_tab(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let number = EditorTabs::global(cx).update(cx, |t, _| t.take_number());
        let connection = ConnectionManager::global(cx)
            .read(cx)
            .active()
            .map(|c| c.id);
        let row = TabRow {
            id: Uuid::new_v4().to_string(),
            title: format!("Query {number}"),
            content: String::new(),
            cursor: 0,
            connection_id: connection.map(|c| c.to_string()),
        };
        Self::from_row(row, window, cx)
    }

    /// Rebuild a tab from storage (layout restore), or an empty tab if the row is gone.
    pub fn restore(id: &str, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let row = AppSettings::storage(cx)
            .and_then(|s| s.load_tab_blocking(id).ok().flatten())
            .unwrap_or_else(|| TabRow {
                id: id.to_owned(),
                title: "Query".into(),
                content: String::new(),
                cursor: 0,
                connection_id: None,
            });
        Self::from_row(row, window, cx)
    }

    pub fn from_row(row: TabRow, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let storage = AppSettings::storage(cx).cloned();
        let tab = cx.new(|cx| {
            let editor = cx.new(|cx| QueryEditor::new(&row.content, window, cx));
            let cursor = row.cursor.min(row.content.len());
            editor.update(cx, |e, cx| {
                e.state()
                    .update(cx, |s, cx| s.set_selected_range(cursor..cursor, cx));
            });
            let manager = ConnectionManager::global(cx);
            let subscriptions = vec![
                cx.subscribe(
                    &editor,
                    |this: &mut Self, _, event: &QueryEditorEvent, cx| match event {
                        QueryEditorEvent::Changed => this.on_changed(cx),
                        QueryEditorEvent::RunRequested { text, range } => {
                            this.on_run(text.clone(), range.clone(), cx);
                        }
                    },
                ),
                cx.observe(&manager, |_, _, cx| cx.notify()),
            ];
            let id = Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::new_v4());
            Self {
                id,
                title: row.title.clone().into(),
                editor,
                connection_id: row
                    .connection_id
                    .as_deref()
                    .and_then(|c| Uuid::parse_str(c).ok()),
                saved_text: row.content.clone(),
                dirty: false,
                storage,
                save_task: None,
                focus_handle: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        });
        EditorTabs::global(cx).update(cx, |t, cx| t.add(&tab, cx));
        // Make sure a brand-new tab exists in storage, so a layout referencing it can be restored.
        tab.update(cx, |t, cx| t.save_now(cx));
        tab
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn editor(&self) -> &Entity<QueryEditor> {
        &self.editor
    }

    pub fn connection_id(&self) -> Option<Uuid> {
        self.connection_id
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn title_text(&self) -> SharedString {
        self.title.clone()
    }

    pub fn set_connection(&mut self, id: Option<Uuid>, cx: &mut Context<'_, Self>) {
        self.connection_id = id;
        if let Some(id) = id {
            ConnectionManager::global(cx).update(cx, |m, cx| m.set_active(Some(id), cx));
        }
        self.schedule_save(cx);
        cx.notify();
    }

    /// Mark the current text as saved (clears the dirty marker).
    pub fn mark_saved(&mut self, cx: &mut Context<'_, Self>) {
        self.saved_text = self.editor.read(cx).text(cx);
        self.dirty = false;
        cx.notify();
    }

    fn on_changed(&mut self, cx: &mut Context<'_, Self>) {
        let dirty = self.editor.read(cx).text(cx) != self.saved_text;
        if dirty != self.dirty {
            self.dirty = dirty;
            cx.notify();
        }
        self.schedule_save(cx);
    }

    fn on_run(&mut self, text: String, range: Range<usize>, cx: &mut Context<'_, Self>) {
        let request = RunRequest {
            tab_id: self.id,
            connection_id: self.connection_id,
            text,
            range,
        };
        if let Some(id) = self.connection_id {
            ConnectionManager::global(cx).update(cx, |m, cx| m.set_active(Some(id), cx));
        }
        EditorTabs::global(cx).update(cx, |tabs, cx| {
            tabs.last_run = Some(request.clone());
            cx.emit(EditorTabsEvent::RunRequested(request));
        });
    }

    fn row(&self, cx: &App) -> TabRow {
        TabRow {
            id: self.id.to_string(),
            title: self.title.to_string(),
            content: self.editor.read(cx).text(cx),
            cursor: self.editor.read(cx).cursor(cx),
            connection_id: self.connection_id.map(|c| c.to_string()),
        }
    }

    fn schedule_save(&mut self, cx: &mut Context<'_, Self>) {
        if self.storage.is_none() {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save_now(cx));
        }));
    }

    /// Write the tab now (also used on quit).
    pub fn save_now(&mut self, cx: &mut Context<'_, Self>) {
        self.save_task = None;
        let Some(storage) = &self.storage else { return };
        let write = storage.save_tab(self.row(cx));
        cx.background_spawn(async move {
            if let Err(err) = write.await {
                log::warn!("failed to save editor tab: {err}");
            }
        })
        .detach();
    }

    fn connection_picker(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let manager = ConnectionManager::global(cx).read(cx);
        let current = self.connection_id.and_then(|id| manager.get(id)).cloned();
        let label: SharedString = current
            .as_ref()
            .map(|c| c.name.clone().into())
            .unwrap_or_else(|| "No connection".into());
        let options: Vec<(Uuid, String)> = manager
            .connections()
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let selected = self.connection_id;
        let this = cx.entity().downgrade();
        Button::new("tab-connection")
            .label(label)
            .icon(IconName::ChevronDown)
            .ghost()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                for (id, name) in &options {
                    let id = *id;
                    let this = this.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(selected == Some(id))
                            .on_click(move |_, _, cx| {
                                let _ = this.update(cx, |tab, cx| tab.set_connection(Some(id), cx));
                            }),
                    );
                }
                if options.is_empty() {
                    menu = menu.label("No saved connections");
                }
                menu
            })
    }
}

impl BasePanel for EditorPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }

    fn dump(&self, _: &App) -> PanelState {
        let mut state = PanelState::new(Self::NAME);
        state.info = PanelInfo::panel(serde_json::json!({ "tab": self.id.to_string() }));
        state
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) {
        // Closed (not merely moved): forget the tab.
        self.save_task = None;
        if let Some(storage) = &self.storage {
            let delete = storage.delete_tab(&self.id.to_string());
            cx.background_spawn(async move {
                let _ = delete.await;
            })
            .detach();
        }
        let id = self.id;
        EditorTabs::global(cx).update(cx, |t, cx| t.remove(id, cx));
    }
}

impl Panel for EditorPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let manager = ConnectionManager::global(cx).read(cx);
        let connection = self.connection_id.and_then(|id| manager.get(id)).cloned();
        h_flex()
            .id(SharedString::from(format!("tab-title-{}", self.id)))
            .test_support()
            .gap(Tokens::space(1.5))
            .children(
                connection
                    .as_ref()
                    .and_then(|c| color_tag_color(c.color, cx))
                    .map(|color| div().size(Tokens::size(7.0)).rounded_full().bg(color)),
            )
            .child(self.title.clone())
            .children(connection.and_then(|c| environment_badge(c.environment, cx)))
            .children(self.dirty.then(|| {
                div()
                    .id(SharedString::from(format!("tab-dirty-{}", self.id)))
                    .test_support()
                    .text_color(cx.theme().muted_foreground)
                    .child("\u{2022}")
            }))
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Menu)
    }
}

impl Render for EditorPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let toolbar = h_flex()
            .id("editor-toolbar")
            .flex_none()
            .gap(Tokens::space(1.0))
            .px(Tokens::space(2.0))
            .py(Tokens::space(1.0))
            .border_b(Tokens::BORDER)
            .border_color(theme.border)
            .child(self.connection_picker(cx))
            .child(div().flex_1())
            .child(
                Button::new("editor-format")
                    .label("Format")
                    .ghost()
                    .xsmall()
                    .tooltip("Format (Shift+Alt+F)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.editor
                            .update(cx, |e, cx| e.format(&FormatQuery, window, cx));
                    })),
            )
            .child(
                Button::new("editor-run")
                    .label("Run")
                    .icon(IconName::Play)
                    .primary()
                    .xsmall()
                    .tooltip("Run the statement at the cursor (Ctrl+Enter)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.editor.update(cx, |e, cx| e.run(&RunQuery, window, cx));
                    })),
            );
        v_flex()
            .id(SharedString::from(format!("editor-tab-{}", self.id)))
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.background)
            .child(toolbar)
            .child(div().flex_1().min_h_0().child(self.editor.clone()))
    }
}

/// Register the editor panel so saved layouts restore tabs by id.
pub fn register(cx: &mut App) {
    register_panel(cx, EditorPanel::NAME, |ctx, window, cx| {
        let id = match ctx.info() {
            PanelInfo::Panel(value) => value.get("tab").and_then(|v| v.as_str()).map(str::to_owned),
            _ => None,
        };
        let tab = match id {
            Some(id) => EditorPanel::restore(&id, window, cx),
            None => EditorPanel::new_tab(window, cx),
        };
        panel_handle(tab)
    });
}
