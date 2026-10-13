//! Saved queries and query history (query-history-saved): the in-memory library of folders and saved
//! queries backed by storage, the history log the runner writes to, and the "Save query" dialog.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use storage::{FolderRow, HistoryEntry, SavedQueryRow, Storage};
use uuid::Uuid;

use crate::connections::ConnectionManager;
use crate::connections::form::field;
use crate::panels::EditorPanel;
use crate::settings::AppSettings;
use crate::theme::Tokens;

/// A tab's own settings, saved with it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSettings {
    /// Overrides the connection's request timeout.
    pub timeout_ms: Option<u64>,
    /// Fetch the edges between result vertices in the graph view.
    pub fetch_edges: Option<bool>,
    /// Results view to show first: `graph`, `table`, `json` or `console`.
    pub default_view: Option<String>,
}

impl TabSettings {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(text: Option<&str>) -> Self {
        text.and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The saved-query library.
pub struct SavedLibrary {
    storage: Storage,
    folders: Vec<FolderRow>,
    queries: Vec<SavedQueryRow>,
}

struct LibraryGlobal(Entity<SavedLibrary>);
impl Global for LibraryGlobal {}

impl SavedLibrary {
    /// Load the library (blocking: startup only).
    pub fn init(storage: Storage, cx: &mut App) {
        let folders = storage.list_folders_blocking().unwrap_or_else(|err| {
            log::warn!("could not load saved-query folders: {err}");
            Vec::new()
        });
        let queries = storage.list_saved_blocking().unwrap_or_else(|err| {
            log::warn!("could not load saved queries: {err}");
            Vec::new()
        });
        let entity = cx.new(|_| Self {
            storage,
            folders,
            queries,
        });
        cx.set_global(LibraryGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<LibraryGlobal>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<LibraryGlobal>().map(|g| g.0.clone())
    }

    pub fn folders(&self) -> &[FolderRow] {
        &self.folders
    }

    pub fn queries(&self) -> &[SavedQueryRow] {
        &self.queries
    }

    pub fn query(&self, id: &str) -> Option<&SavedQueryRow> {
        self.queries.iter().find(|q| q.id == id)
    }

    pub fn folder(&self, id: &str) -> Option<&FolderRow> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Whether a folder holds queries or subfolders.
    pub fn folder_has_contents(&self, id: &str) -> bool {
        self.queries
            .iter()
            .any(|q| q.folder_id.as_deref() == Some(id))
            || self
                .folders
                .iter()
                .any(|f| f.parent_id.as_deref() == Some(id))
    }

    fn write<F>(&self, what: &'static str, fut: F, cx: &mut Context<'_, Self>)
    where
        F: std::future::Future<Output = storage::Result<()>> + Send + 'static,
    {
        cx.background_spawn(async move {
            if let Err(err) = fut.await {
                log::warn!("could not save {what}: {err}");
            }
        })
        .detach();
        cx.notify();
    }

    pub fn create_folder(
        &mut self,
        name: &str,
        parent: Option<String>,
        cx: &mut Context<'_, Self>,
    ) -> String {
        let row = FolderRow {
            id: Uuid::new_v4().to_string(),
            parent_id: parent,
            connection_id: None,
            name: name.trim().to_owned(),
            sort: 0,
        };
        let id = row.id.clone();
        self.folders.push(row.clone());
        self.folders.sort_by_key(|x| x.name.to_lowercase());
        self.write("the folder", self.storage.save_folder(row), cx);
        id
    }

    pub fn rename_folder(&mut self, id: &str, name: &str, cx: &mut Context<'_, Self>) {
        let Some(f) = self.folders.iter_mut().find(|f| f.id == id) else {
            return;
        };
        f.name = name.trim().to_owned();
        let row = f.clone();
        self.write("the folder", self.storage.save_folder(row), cx);
    }

    /// Delete a folder with everything in it.
    pub fn delete_folder(&mut self, id: &str, cx: &mut Context<'_, Self>) {
        let mut doomed = vec![id.to_owned()];
        let mut i = 0;
        while i < doomed.len() {
            let parent = doomed[i].clone();
            doomed.extend(
                self.folders
                    .iter()
                    .filter(|f| f.parent_id.as_deref() == Some(parent.as_str()))
                    .map(|f| f.id.clone()),
            );
            i += 1;
        }
        self.folders.retain(|f| !doomed.contains(&f.id));
        self.queries
            .retain(|q| q.folder_id.as_ref().is_none_or(|f| !doomed.contains(f)));
        self.write("the folder", self.storage.delete_folder(id), cx);
    }

    /// Save a new query; returns its id.
    pub fn save_new(
        &mut self,
        name: &str,
        folder: Option<String>,
        connection: Option<Uuid>,
        text: &str,
        settings: &TabSettings,
        cx: &mut Context<'_, Self>,
    ) -> String {
        let row = SavedQueryRow {
            id: Uuid::new_v4().to_string(),
            folder_id: folder,
            connection_id: connection.map(|c| c.to_string()),
            name: name.trim().to_owned(),
            text: text.to_owned(),
            settings: settings.to_json(),
            updated_at: now_ms(),
        };
        let id = row.id.clone();
        self.queries.push(row.clone());
        self.queries.sort_by_key(|x| x.name.to_lowercase());
        self.write("the query", self.storage.save_query(row), cx);
        id
    }

    /// Update a saved query's text and settings (Ctrl/Cmd+S on a tab opened from it).
    pub fn update_query(
        &mut self,
        id: &str,
        text: &str,
        settings: &TabSettings,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let Some(q) = self.queries.iter_mut().find(|q| q.id == id) else {
            return false;
        };
        q.text = text.to_owned();
        q.settings = settings.to_json();
        q.updated_at = now_ms();
        let row = q.clone();
        self.write("the query", self.storage.save_query(row), cx);
        true
    }

    pub fn rename_query(&mut self, id: &str, name: &str, cx: &mut Context<'_, Self>) {
        let Some(q) = self.queries.iter_mut().find(|q| q.id == id) else {
            return;
        };
        q.name = name.trim().to_owned();
        let row = q.clone();
        self.write("the query", self.storage.save_query(row), cx);
    }

    /// Move a query into `folder` (`None`: top level).
    pub fn move_query(&mut self, id: &str, folder: Option<String>, cx: &mut Context<'_, Self>) {
        let Some(q) = self.queries.iter_mut().find(|q| q.id == id) else {
            return;
        };
        if q.folder_id == folder {
            return;
        }
        q.folder_id = folder;
        let row = q.clone();
        self.write("the query", self.storage.save_query(row), cx);
    }

    pub fn delete_query(&mut self, id: &str, cx: &mut Context<'_, Self>) {
        self.queries.retain(|q| q.id != id);
        self.write("the query", self.storage.delete_saved(id), cx);
    }
}

/// Bumps when history is written, so the history panel refreshes.
#[derive(Default)]
pub struct HistoryLog {
    pub version: u64,
}

struct HistoryGlobal(Entity<HistoryLog>);
impl Global for HistoryGlobal {}

impl HistoryLog {
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| HistoryLog::default());
        cx.set_global(HistoryGlobal(entity));
    }

    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<HistoryGlobal>().map(|g| g.0.clone())
    }

    /// The database history lives in.
    pub fn storage(cx: &App) -> Option<Storage> {
        ConnectionManager::try_global(cx).map(|m| m.read(cx).storage().clone())
    }

    /// Record an execution in the background.
    pub fn record(entry: HistoryEntry, cx: &mut App) {
        let (Some(log), Some(storage)) = (Self::global(cx), Self::storage(cx)) else {
            return;
        };
        let keep = AppSettings::general(cx).history_limit;
        let write = storage.record_history(entry, keep);
        cx.spawn(async move |cx| {
            if let Err(err) = write.await {
                log::warn!("could not record history: {err}");
                return;
            }
            log.update(cx, |l, cx| {
                l.version += 1;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn now_ms() -> i64 {
        now_ms()
    }
}

/// The "Save query" dialog for a tab not yet in the library.
pub struct SaveQueryForm {
    tab: WeakEntity<EditorPanel>,
    pub name: Entity<InputState>,
    pub folder: Option<String>,
    /// Save under the tab's connection instead of globally.
    pub under_connection: bool,
    pub error: Option<String>,
}

impl SaveQueryForm {
    pub fn open(tab: &Entity<EditorPanel>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title = tab.read(cx).title().to_string();
        let weak = tab.downgrade();
        let form = cx.new(|cx| Self {
            tab: weak,
            name: cx.new(|cx| InputState::new(window, cx).default_value(title)),
            folder: None,
            under_connection: false,
            error: None,
        });
        let view = form.clone();
        let focus = form.read(cx).name.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Save query")
                .w(Tokens::CONNECTION_DIALOG_WIDTH)
                .margin_top(Tokens::DIALOG_TOP)
                .child(view.clone())
        });
        window.defer(cx, move |window, cx| {
            focus.update(cx, |input, cx| input.focus(window, cx))
        });
        form
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            self.error = Some("Enter a name.".into());
            cx.notify();
            return;
        }
        let Some(tab) = self.tab.upgrade() else {
            window.close_dialog(cx);
            return;
        };
        let folder = self.folder.clone();
        let under = self.under_connection;
        tab.update(cx, |t, cx| t.save_as(&name, folder, under, cx));
        window.close_dialog(cx);
    }
}

impl Render for SaveQueryForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let library = SavedLibrary::global(cx).read(cx);
        let folders: Vec<(String, String)> = library
            .folders()
            .iter()
            .map(|f| (f.id.clone(), f.name.clone()))
            .collect();
        let chip = |id: SharedString,
                    label: String,
                    value: Option<String>,
                    selected: bool,
                    cx: &mut Context<'_, Self>| {
            Button::new(id)
                .label(label)
                .xsmall()
                .when(selected, |b| b.primary())
                .when(!selected, |b| b.outline())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.folder = value.clone();
                    cx.notify();
                }))
        };
        let mut chips = vec![chip(
            "save-folder-none".into(),
            "No folder".into(),
            None,
            self.folder.is_none(),
            cx,
        )];
        for (id, name) in folders {
            let selected = self.folder.as_deref() == Some(id.as_str());
            chips.push(chip(
                SharedString::from(format!("save-folder-{id}")),
                name,
                Some(id),
                selected,
                cx,
            ));
        }
        v_flex()
            .id("save-query-form")
            .test_support()
            .gap(Tokens::space(3.0))
            .child(field(
                "Name",
                Input::new(&self.name),
                self.error.as_ref(),
                cx,
            ))
            .child(field(
                "Folder",
                h_flex().flex_wrap().gap(Tokens::space(1.0)).children(chips),
                None,
                cx,
            ))
            .child(
                Switch::new("save-under-connection")
                    .label("Save under this tab's connection")
                    .checked(self.under_connection)
                    .small()
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.under_connection = *on;
                        cx.notify();
                    })),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap(Tokens::space(2.0))
                    .child(
                        Button::new("save-query-cancel")
                            .label("Cancel")
                            .ghost()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save-query-ok")
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
            .text_color(cx.theme().foreground)
    }
}
