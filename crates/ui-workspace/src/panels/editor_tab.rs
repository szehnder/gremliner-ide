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
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::*;
use storage::{Storage, TabRow};
use ui_editor::{QueryEditor, QueryEditorEvent};
use uuid::Uuid;

use super::debug_view::{DebugView, DebugViewEvent};
use super::{WorkspacePanel, panel_basics};
use crate::actions::debug::{StartDebug, StepBack, StepInto, StepOut, StepOver, StopDebug};
use crate::actions::editor::{ExplainQuery, FormatQuery, ProfileQuery, RunQuery};
use crate::connections::ConnectionManager;
use crate::connections::style::{color_tag_color, environment_badge};
use crate::settings::AppSettings;
use crate::theme::Tokens;
use gpui_kit::component::notification::Notification;
use ui_assistant::{Disclosure, OpenPromptBar, PromptBar, PromptBarEvent};

/// How long edits settle before a tab is written.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// A request to run Gremlin, raised by an editor tab. `query-execution` consumes these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub tab_id: Uuid,
    pub connection_id: Option<Uuid>,
    pub text: String,
    pub range: Range<usize>,
    /// Run, profile or explain.
    pub mode: ui_editor::RunMode,
    /// A query the app generated (the debugger's prefixes): errors are not mapped onto the editor text.
    pub generated: bool,
}

#[derive(Debug, Clone)]
pub enum EditorTabsEvent {
    RunRequested(RunRequest),
    /// A tab opened or closed.
    TabsChanged,
    /// Another tab became active.
    ActiveChanged,
}

/// Registry of open editor tabs.
#[derive(Default)]
pub struct EditorTabs {
    tabs: Vec<WeakEntity<EditorPanel>>,
    next_number: usize,
    last_run: Option<RunRequest>,
    /// The tab most recently shown or focused (the assistant follows it).
    active: Option<Uuid>,
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

    /// The active tab: the one last shown or focused, else the newest.
    pub fn active(&self, cx: &App) -> Option<Entity<EditorPanel>> {
        let tabs = self.tabs();
        self.active
            .and_then(|id| tabs.iter().find(|t| t.read(cx).id == id).cloned())
            .or_else(|| tabs.last().cloned())
    }

    pub fn set_active(&mut self, id: Uuid, cx: &mut Context<'_, Self>) {
        if self.active != Some(id) {
            self.active = Some(id);
            cx.emit(EditorTabsEvent::ActiveChanged);
            cx.notify();
        }
    }

    fn add(&mut self, tab: &Entity<EditorPanel>, cx: &mut Context<'_, Self>) {
        // Restored "Query N" tabs push numbering past N, so new tabs never repeat a name.
        if let Some(n) = tab
            .read(cx)
            .title
            .strip_prefix("Query ")
            .and_then(|n| n.parse::<usize>().ok())
        {
            self.next_number = self.next_number.max(n + 1);
        }
        self.tabs.retain(|t| t.upgrade().is_some());
        self.tabs.push(tab.downgrade());
        self.active = Some(tab.read(cx).id);
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
    /// The assistant prompt bar, while open.
    prompt: Option<Entity<PromptBar>>,
    prompt_task: Option<Task<()>>,
    /// The debug session on this tab, while one runs.
    debug: Option<Entity<DebugView>>,
    _debug_subscription: Option<Subscription>,
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
            let mut subscriptions = vec![
                cx.subscribe(
                    &editor,
                    |this: &mut Self, _, event: &QueryEditorEvent, cx| match event {
                        QueryEditorEvent::Changed => this.on_changed(cx),
                        QueryEditorEvent::RunRequested { text, range, mode } => {
                            this.on_run(text.clone(), range.clone(), *mode, cx);
                        }
                    },
                ),
                cx.observe(&manager, |_, _, cx| cx.notify()),
            ];
            if let Some(registry) = crate::schema::SchemaRegistry::try_global(cx) {
                subscriptions
                    .push(cx.observe(&registry, |this: &mut Self, _, cx| this.sync_schema(cx)));
            }
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
                prompt: None,
                prompt_task: None,
                debug: None,
                _debug_subscription: None,
                focus_handle: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        });
        tab.update(cx, |t, cx| t.sync_schema(cx));
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

    pub fn debug_view(&self) -> Option<&Entity<DebugView>> {
        self.debug.as_ref()
    }

    /// Start debugging the statement at the cursor (Ctrl+F5).
    pub fn start_debug(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.debug.is_some() {
            return;
        }
        let Some(connection_id) = self.connection_id else {
            window.push_notification(
                Notification::warning("Choose a connection for this tab, then debug again.")
                    .title("No connection"),
                cx,
            );
            return;
        };
        let Some((text, range)) = self.editor.read(cx).run_target(cx) else {
            return;
        };
        // The statement's own offset: leading whitespace in a selection is trimmed by the session.
        let base = range.start + (text.len() - text.trim_start().len());
        let session = match debugger::Session::start(text.trim_start()) {
            Ok(s) => s,
            Err(err) => {
                window.push_notification(
                    Notification::warning(err.message()).title("Cannot debug this query"),
                    cx,
                );
                return;
            }
        };
        let tab_id = self.id;
        let view = cx.new(|cx| DebugView::new(session, base, tab_id, connection_id, cx));
        self._debug_subscription = Some(cx.subscribe_in(
            &view,
            window,
            |this, view, event: &DebugViewEvent, window, cx| match event {
                DebugViewEvent::CurrentChanged => {
                    let range = view.read(cx).current_range();
                    this.editor
                        .update(cx, |e, cx| e.set_debug_step(Some(range), cx));
                }
                DebugViewEvent::Stop => this.stop_debug(window, cx),
            },
        ));
        self.debug = Some(view.clone());
        self.editor.update(cx, |e, cx| e.set_read_only(true, cx));
        view.update(cx, |v, cx| v.show_current(window, cx));
        // Keys go to the tab now that the text is locked.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub fn stop_debug(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.debug.take().is_none() {
            return;
        }
        self._debug_subscription = None;
        self.editor.update(cx, |e, cx| {
            e.set_debug_step(None, cx);
            e.set_read_only(false, cx);
        });
        let state = self.editor.read(cx).state().clone();
        state.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn with_debug(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        f: fn(&mut DebugView, &mut Window, &mut Context<'_, DebugView>),
    ) {
        if let Some(view) = self.debug.clone() {
            view.update(cx, |v, cx| f(v, window, cx));
        }
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

    /// Give the editor the schema of this tab's connection (for completions and warnings).
    fn sync_schema(&mut self, cx: &mut Context<'_, Self>) {
        let schema = self.connection_id.and_then(|id| {
            crate::schema::SchemaRegistry::try_global(cx)?
                .read(cx)
                .schema(id)
        });
        let manager = ConnectionManager::global(cx);
        let restrictions = self
            .connection_id
            .map(|id| {
                let m = manager.read(cx);
                match m.session(id) {
                    Some(session) => session.capabilities.restrictions.clone(),
                    None => m
                        .get(id)
                        .map(providers::restrictions_for)
                        .unwrap_or_else(gremlin_syntax::dialect::Restrictions::none),
                }
            })
            .unwrap_or_else(gremlin_syntax::dialect::Restrictions::none);
        self.editor.update(cx, |e, cx| {
            e.set_schema(schema, cx);
            e.set_restrictions(restrictions, cx);
        });
    }

    pub fn set_connection(&mut self, id: Option<Uuid>, cx: &mut Context<'_, Self>) {
        self.connection_id = id;
        self.sync_schema(cx);
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

    fn on_run(
        &mut self,
        text: String,
        range: Range<usize>,
        mode: ui_editor::RunMode,
        cx: &mut Context<'_, Self>,
    ) {
        let request = RunRequest {
            tab_id: self.id,
            connection_id: self.connection_id,
            text,
            range,
            mode,
            generated: false,
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

    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<'_, Self>) {
        if active {
            let id = self.id;
            EditorTabs::global(cx).update(cx, |t, cx| t.set_active(id, cx));
        }
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
                Button::new("editor-ask")
                    .label("Ask")
                    .icon(crate::assets::icons::IconName::Sparkles)
                    .ghost()
                    .xsmall()
                    .tooltip("Describe what you want in plain English (Ctrl+K)")
                    .on_click(cx.listener(|this, _, window, cx| this.open_prompt_bar(window, cx))),
            )
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
                Button::new("editor-debug")
                    .label("Debug")
                    .ghost()
                    .xsmall()
                    .tooltip("Step through the statement (Ctrl+F5)")
                    .on_click(cx.listener(|this, _, window, cx| this.start_debug(window, cx))),
            )
            .child(
                Button::new("editor-explain")
                    .label("Explain")
                    .ghost()
                    .xsmall()
                    .tooltip("Show how the database plans the statement")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.editor
                            .update(cx, |e, cx| e.explain(&ExplainQuery, window, cx));
                    })),
            )
            .child(
                Button::new("editor-profile")
                    .label("Profile")
                    .ghost()
                    .xsmall()
                    .tooltip("Run with profiling (Ctrl+Shift+Enter)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.editor
                            .update(cx, |e, cx| e.profile(&ProfileQuery, window, cx));
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
            .key_context("EditorPanel")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme.background)
            .on_action(
                cx.listener(|this, _: &OpenPromptBar, window, cx| this.open_prompt_bar(window, cx)),
            )
            .on_action(cx.listener(|this, _: &StartDebug, window, cx| this.start_debug(window, cx)))
            .on_action(cx.listener(|this, _: &StopDebug, window, cx| this.stop_debug(window, cx)))
            .on_action(cx.listener(|this, _: &StepOver, window, cx| {
                this.with_debug(window, cx, DebugView::step_over)
            }))
            .on_action(cx.listener(|this, _: &StepInto, window, cx| {
                this.with_debug(window, cx, DebugView::step_into)
            }))
            .on_action(cx.listener(|this, _: &StepOut, window, cx| {
                this.with_debug(window, cx, DebugView::step_out)
            }))
            .on_action(cx.listener(|this, _: &StepBack, window, cx| {
                this.with_debug(window, cx, DebugView::step_back)
            }))
            .child(toolbar)
            .children(self.prompt.clone())
            .child(div().flex_1().min_h_0().child(self.editor.clone()))
            .children(
                self.debug
                    .clone()
                    .map(|d| div().flex_none().h(Tokens::size(300.0)).child(d)),
            )
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

// ---- assistant prompt bar (nl-to-gremlin) ---------------------------------------------------------

impl EditorPanel {
    pub fn prompt_bar(&self) -> Option<&Entity<PromptBar>> {
        self.prompt.as_ref()
    }

    /// The model and data disclosure for this tab's connection; `None` without an LLM profile.
    fn disclosure(&self, cx: &App) -> Option<Disclosure> {
        let profiles = crate::ai::LlmProfiles::try_global(cx)?;
        let p = profiles.read(cx);
        let profile = p.resolve(self.connection_id)?;
        Some(Disclosure {
            profile: profile.name.clone(),
            local: profile.is_local(),
            share_samples: self.connection_id.is_some_and(|c| p.share_samples(c)),
        })
    }

    /// Show (or focus) the prompt bar.
    pub fn open_prompt_bar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let disclosure = self.disclosure(cx);
        let bar = match &self.prompt {
            Some(bar) => bar.clone(),
            None => {
                let bar = cx.new(|cx| PromptBar::new(window, cx));
                let sub =
                    cx.subscribe_in(&bar, window, |this, _, e: &PromptBarEvent, window, cx| {
                        this.on_prompt_event(e.clone(), window, cx);
                    });
                self._subscriptions.push(sub);
                self.prompt = Some(bar.clone());
                bar
            }
        };
        bar.update(cx, |b, cx| {
            b.set_disclosure(disclosure, cx);
            b.focus_input(window, cx);
        });
        cx.notify();
    }

    pub fn close_prompt_bar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.prompt = None;
        self.prompt_task = None;
        self.editor.update(cx, |e, cx| e.focus(window, cx));
        cx.notify();
    }

    fn on_prompt_event(
        &mut self,
        event: PromptBarEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            PromptBarEvent::Submit { prompt, history } => {
                self.run_assistant(prompt, history, window, cx)
            }
            PromptBarEvent::Insert(query) => {
                self.insert_statement(&query, window, cx);
                self.close_prompt_bar(window, cx);
            }
            PromptBarEvent::Run { query, writes } => {
                if writes {
                    let this = cx.entity().downgrade();
                    window.open_alert_dialog(cx, move |alert, _, cx| {
                        let this = this.clone();
                        let query = query.clone();
                        alert
                            .icon(gpui_kit::component::Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning))
                            .title("Run a query that writes data?")
                            .description("The assistant wrote a query that changes the graph. It was not test-run.")
                            .ok_text("Run it")
                            .show_cancel(true)
                            .on_ok(move |_, window, cx| {
                                let _ = this.update(cx, |t, cx| t.insert_and_run(&query, window, cx));
                                true
                            })
                    });
                } else {
                    self.insert_and_run(&query, window, cx);
                }
            }
            PromptBarEvent::OpenSettings => {
                // Directly rather than as an action: from a click, nothing in this tab may hold focus.
                window.defer(cx, |window, cx| {
                    if let Some(ws) = crate::Workspace::for_window(window, cx) {
                        ws.update(cx, |ws, cx| {
                            ws.open_settings(&crate::actions::OpenSettings, window, cx)
                        });
                    }
                });
            }
            PromptBarEvent::Close => self.close_prompt_bar(window, cx),
        }
    }

    /// Insert `query` on its own line after the cursor's line, selecting nothing, cursor inside it.
    pub fn insert_statement(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let query = query.trim().to_owned();
        self.editor.update(cx, |e, cx| {
            let text = e.text(cx);
            let cursor = e.cursor(cx).min(text.len());
            let line_end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
            let line_start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
            let line_blank = text[line_start..line_end].trim().is_empty();
            let (at, insert) = if line_blank {
                (line_start, query.clone())
            } else {
                (line_end, format!("\n{query}"))
            };
            let end = if line_blank { line_end } else { at };
            let caret = at + insert.len();
            e.state().update(cx, |s, cx| {
                s.set_selected_range(at..end, cx);
                s.replace(insert, window, cx);
                let pos = gpui_kit::component::input::RopeExt::offset_to_position(s.text(), caret);
                s.set_cursor_position(pos, window, cx);
            });
        });
    }

    /// Run a suggested query: insert it, then run it; queries that write ask first.
    pub fn run_suggestion(
        &mut self,
        query: &str,
        writes: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.on_prompt_event(
            PromptBarEvent::Run {
                query: query.to_owned(),
                writes,
            },
            window,
            cx,
        );
    }

    fn insert_and_run(&mut self, query: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.insert_statement(query, window, cx);
        self.close_prompt_bar(window, cx);
        // The editor re-analyses on its (deferred) change event; run once it has.
        cx.defer_in(window, |this, window, cx| {
            this.editor.update(cx, |e, cx| e.run(&RunQuery, window, cx));
        });
    }

    fn run_assistant(
        &mut self,
        prompt: String,
        history: Vec<llm::Message>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(bar) = self.prompt.clone() else {
            return;
        };
        let run_id = bar.read(cx).run_id();
        let fail = |bar: &Entity<PromptBar>,
                    msg: String,
                    window: &mut Window,
                    cx: &mut Context<'_, Self>| {
            bar.update(cx, |b, cx| {
                b.apply(run_id, assistant::PipelineEvent::Failed(msg), window, cx)
            });
        };
        let Some(connection) = self.connection_id else {
            fail(
                &bar,
                "Choose a connection for this tab first.".into(),
                window,
                cx,
            );
            return;
        };
        let Some(profiles) = crate::ai::LlmProfiles::try_global(cx) else {
            return;
        };
        let Some(profile) = profiles.read(cx).resolve(Some(connection)).cloned() else {
            fail(&bar, "No language model is configured.".into(), window, cx);
            return;
        };
        let share_samples = profiles.read(cx).share_samples(connection);
        let client_task = profiles.read(cx).client(profile, None, cx);
        let manager = ConnectionManager::global(cx);
        let schema = crate::schema::SchemaRegistry::try_global(cx)
            .and_then(|r| r.read(cx).schema(connection));
        let neptune = manager
            .read(cx)
            .get(connection)
            .is_some_and(|c| c.provider.kind().is_neptune());
        let current_query = Some(self.editor.read(cx).text(cx)).filter(|t| !t.trim().is_empty());
        let weak_bar = bar.downgrade();
        self.prompt_task = Some(cx.spawn_in(window, async move |_, cx| {
            if manager.read_with(cx, |m, _| m.session(connection).is_none()) {
                manager.update(cx, |m, cx| m.connect(connection, cx)).await;
            }
            let Some(session) = manager.read_with(cx, |m, _| m.session(connection).cloned()) else {
                let _ = weak_bar.update_in(cx, |b, window, cx| {
                    b.apply(
                        run_id,
                        assistant::PipelineEvent::Failed(
                            "Could not connect to the database.".into(),
                        ),
                        window,
                        cx,
                    )
                });
                return;
            };
            let client = match client_task.await {
                Ok(c) => c,
                Err(e) => {
                    let _ = weak_bar.update_in(cx, |b, window, cx| {
                        b.apply(
                            run_id,
                            assistant::PipelineEvent::Failed(e.to_string()),
                            window,
                            cx,
                        )
                    });
                    return;
                }
            };
            let ctx = assistant::Context {
                schema,
                dialect: assistant::Dialect {
                    tinkerpop_version: session.info.tinkerpop_version.clone(),
                    neptune,
                },
                current_query,
                share_samples,
            };
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<assistant::PipelineEvent>();
            let work = cx.update(|_, cx| {
                crate::io::spawn_io(cx, async move {
                    let emit = move |e: assistant::PipelineEvent| {
                        let _ = tx.unbounded_send(e);
                    };
                    assistant::run(
                        &prompt,
                        &history,
                        &ctx,
                        &client,
                        Some(&session.client),
                        &assistant::Options::default(),
                        &emit,
                    )
                    .await
                })
            });
            let Ok(work) = work else { return };
            use futures::StreamExt as _;
            while let Some(event) = rx.next().await {
                if weak_bar
                    .update_in(cx, |b, window, cx| b.apply(run_id, event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
            let _ = work.await;
        }));
    }
}
