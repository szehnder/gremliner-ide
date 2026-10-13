//! The main window's root view: header bar, dock area, status bar, and every workspace action.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, panel_handle,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{
    ActiveTheme as _, IconName, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::*;
use storage::Storage;

use crate::actions::*;
use crate::connections::ConnectionManager;
use crate::palette::{self, CommandContext, RECENT_KEY, RecentCommands};
use crate::panels::{
    AssistantPanel, ConnectionsPanel, EditorPanel, EditorTabs, EditorTabsEvent, ExplorerPanel,
    HistoryPanel, InspectorPanel, ReportPanel, ResultsPanel, SavedPanel, SchemaGraphPanel,
    SettingsPanel, WelcomePanel, WorkspacePanel, confirm_reset_general,
};
use crate::perf_hud::PerfHud;
use crate::persist::{self, LAYOUT_VERSION, LayoutProblem, WindowGeometry};
use crate::run::{Runner, RunnerEvent};
use crate::settings::AppSettings;
use crate::status_bar::StatusBar;
use crate::theme::ButtonTip as _;
use crate::theme::{DEFAULT_UI_FONT_SIZE, FONT_SIZE_STEP, Tokens};
use crate::tour::{Tour, TourEvent};

/// How long layout changes settle before they are written.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Default)]
pub struct WorkspaceOptions {
    /// Show the frame-time overlay.
    pub perf_hud: bool,
    /// Offer the first-run tour unless it was finished or skipped before (the app sets this; tests
    /// opt in).
    pub tour: bool,
}

pub struct Workspace {
    focus_handle: FocusHandle,
    dock: Entity<DockArea>,
    status_bar: Entity<StatusBar>,
    perf_hud: Option<PerfHud>,
    storage: Option<Storage>,
    save_task: Option<Task<()>>,
    tour: Option<Entity<Tour>>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(options: WorkspaceOptions, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let storage = AppSettings::storage(cx).cloned();
        let (dock, skin) = DockSkin::dock_area("workspace", Some(LAYOUT_VERSION), window, cx);
        // Editor tabs can be closed; fixed panels opt out through `closable()`.
        skin.set_close_button_visible(true, cx);

        let restored = storage
            .as_ref()
            .map(persist::load_dock)
            .transpose()
            .unwrap_or_else(|problem| {
                match problem {
                    LayoutProblem::Unreadable(err) => {
                        log::warn!("saved layout is unreadable, using the default: {err}");
                    }
                    LayoutProblem::OldVersion(v) => {
                        log::info!("saved layout version {v:?} is outdated, using the default");
                    }
                }
                None
            })
            .flatten();
        let loaded = restored.is_some_and(|state| {
            dock.update(cx, |dock, cx| dock.load(state, window, cx))
                .inspect_err(|err| log::warn!("could not restore layout, using the default: {err}"))
                .is_ok()
        });
        if !loaded {
            Self::install_default_layout(&dock, window, cx);
            // Tabs survive an unreadable layout: reopen whatever is stored.
            let rows = storage
                .as_ref()
                .and_then(|s| s.list_tabs_blocking().ok())
                .unwrap_or_default();
            for row in rows {
                let tab = EditorPanel::from_row(row, window, cx);
                dock.update(cx, |dock, cx| {
                    dock.add_panel_view(panel_handle(tab), DockPlacement::Center, None, window, cx);
                });
            }
        }

        let subscriptions = vec![
            cx.subscribe_in(&dock, window, |this, _, event: &DockEvent, window, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    this.clamp_sidebar(window, cx);
                    this.schedule_save(window, cx);
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                AppSettings::set_appearance(window.appearance(), cx);
            }),
            cx.observe_window_bounds(window, |this, window, cx| this.schedule_save(window, cx)),
            cx.subscribe_in(
                &EditorTabs::global(cx),
                window,
                |_, _, event: &EditorTabsEvent, window, cx| {
                    if let EditorTabsEvent::RunRequested(request) = event {
                        let request = request.clone();
                        Runner::global(cx).update(cx, |r, cx| r.request(request, window, cx));
                    }
                },
            ),
            cx.subscribe_in(
                &Runner::global(cx),
                window,
                |this, _, event: &RunnerEvent, window, cx| {
                    if *event == RunnerEvent::Started
                        && !this.dock.read(cx).is_dock_open(DockPlacement::Bottom)
                    {
                        this.toggle(DockPlacement::Bottom, window, cx);
                    }
                    cx.notify();
                },
            ),
            // The welcome view hides while tabs are open; re-render the dock when that changes.
            cx.observe(&EditorTabs::global(cx), |this, _, cx| {
                this.dock.update(cx, |_, cx| cx.notify());
                cx.notify();
            }),
        ];
        // Closing the window saves immediately; the debounce may still be pending.
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            if let Some(this) = weak.upgrade() {
                this.update(cx, |this, cx| this.save_now(window, cx));
            }
            true
        });
        AppSettings::set_appearance(window.appearance(), cx);

        let status_bar = cx.new(StatusBar::new);
        let mut subscriptions = subscriptions;
        let tour = (options.tour && crate::tour::should_show(storage.as_ref())).then(|| {
            let tour = cx.new(|_| Tour::new(storage.clone()));
            subscriptions.push(cx.subscribe(&tour, |this, _, _: &TourEvent, cx| {
                this.tour = None;
                cx.notify();
            }));
            tour
        });
        Self {
            focus_handle: cx.focus_handle(),
            dock,
            status_bar,
            perf_hud: options.perf_hud.then(PerfHud::default),
            storage,
            save_task: None,
            tour,
            _subscriptions: subscriptions,
        }
    }

    /// The first-run tour, while it shows.
    pub fn tour(&self) -> Option<&Entity<Tour>> {
        self.tour.as_ref()
    }

    pub fn dock_area(&self) -> &Entity<DockArea> {
        &self.dock
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    fn install_default_layout(dock: &Entity<DockArea>, window: &mut Window, cx: &mut App) {
        let welcome = WelcomePanel::build(window, cx);
        let connections = ConnectionsPanel::build(window, cx);
        let results = ResultsPanel::build(window, cx);
        let inspector = InspectorPanel::build(window, cx);
        let assistant = AssistantPanel::build(window, cx);
        let saved = SavedPanel::build(window, cx);
        let history = HistoryPanel::build(window, cx);
        dock.update(cx, |dock, cx| {
            dock.set_center(
                DockLayout::tabs().panel_view(panel_handle(welcome), cx),
                window,
                cx,
            );
            for (placement, panel, size) in [
                (
                    DockPlacement::Left,
                    panel_handle(connections),
                    Tokens::SIDEBAR_WIDTH,
                ),
                (
                    DockPlacement::Bottom,
                    panel_handle(results),
                    Tokens::RESULTS_HEIGHT,
                ),
                (
                    DockPlacement::Right,
                    panel_handle(inspector),
                    Tokens::RIGHT_PANEL_WIDTH,
                ),
            ] {
                dock.set_dock(
                    placement,
                    DockLayout::tabs().panel_view(panel, cx),
                    window,
                    cx,
                );
                dock.set_dock_size(placement, size, window, cx);
            }
            // Saved queries and history share the left dock, behind the connections.
            for panel in [panel_handle(saved), panel_handle(history)] {
                dock.add_panel_view(panel, DockPlacement::Left, None, window, cx);
            }
            if let Some(id) = dock
                .layout(DockPlacement::Left)
                .and_then(|t| t.panels().next())
            {
                dock.select_panel(id, window, cx);
            }
            // The assistant shares the right dock, behind the inspector.
            dock.add_panel_view(
                panel_handle(assistant),
                DockPlacement::Right,
                None,
                window,
                cx,
            );
            if let Some(id) = dock
                .layout(DockPlacement::Right)
                .and_then(|t| t.panels().next())
            {
                dock.select_panel(id, window, cx);
            }
        });
    }

    /// Show the assistant panel (adding it to the right dock if a saved layout lacks it), or hide the
    /// right dock when the assistant is already showing.
    pub fn toggle_assistant(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<Entity<AssistantPanel>> {
        let found = [
            DockPlacement::Right,
            DockPlacement::Left,
            DockPlacement::Bottom,
            DockPlacement::Center,
        ]
        .into_iter()
        .find_map(|placement| {
            let dock = self.dock.read(cx);
            dock.layout(placement)?.panels().find_map(|id| {
                let view = dock.panel(id)?.view().downcast::<AssistantPanel>().ok()?;
                Some((id, view))
            })
        });
        let (id, view) = match found {
            Some(f) => f,
            None => {
                let panel = AssistantPanel::build(window, cx);
                let handle = panel_handle(panel.clone());
                let id = handle.panel_id(cx);
                self.dock.update(cx, |dock, cx| {
                    dock.add_panel_view(handle, DockPlacement::Right, None, window, cx);
                });
                (id, panel)
            }
        };
        self.dock.update(cx, |dock, cx| {
            if !dock.is_dock_open(DockPlacement::Right) {
                dock.toggle_dock(DockPlacement::Right, window, cx);
            }
            dock.select_panel(id, window, cx);
        });
        Some(view)
    }

    /// Open the assistant on the tab that ran last and ask it to fix that run's error.
    fn fix_query_error(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let tab_id = Runner::global(cx).read(cx).latest_tab();
        let tab = tab_id.and_then(|id| {
            EditorTabs::global(cx)
                .read(cx)
                .tabs()
                .into_iter()
                .find(|t| t.read(cx).id() == id)
        });
        let Some(tab) = tab.or_else(|| self.current_tab(window, cx)) else {
            return;
        };
        let Some(panel) = self.toggle_assistant(window, cx) else {
            return;
        };
        let chat = panel.update(cx, |p, cx| p.view_for(&tab, window, cx));
        chat.update(cx, |c, cx| {
            c.send(assistant::chat::Mode::Fix, String::new(), cx)
        });
    }

    /// Keep the sidebar within its designed range; the dock itself only enforces a global minimum.
    fn clamp_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(size) = self.dock.read(cx).dock_size(DockPlacement::Left) else {
            return;
        };
        let clamped = size.clamp(Tokens::SIDEBAR_MIN_WIDTH, Tokens::SIDEBAR_MAX_WIDTH);
        if clamped != size {
            self.dock.update(cx, |dock, cx| {
                dock.set_dock_size(DockPlacement::Left, clamped, window, cx);
            });
        }
    }

    /// Save after changes settle. Replacing the task cancels the previous one.
    fn schedule_save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.storage.is_none() {
            return;
        }
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            let _ = this.update_in(cx, |this, window, cx| this.save_now(window, cx));
        }));
    }

    /// Write the layout and window geometry now.
    pub fn save_now(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.save_task = None;
        for tab in EditorTabs::global(cx).read(cx).tabs() {
            tab.update(cx, |tab, cx| tab.save_now(cx));
        }
        let Some(storage) = &self.storage else { return };
        persist::save_dock(storage, &self.dock.read(cx).dump(cx), cx);
        persist::save_window(
            storage,
            WindowGeometry::from_bounds(window.window_bounds()),
            cx,
        );
    }

    fn toggle(
        &mut self,
        placement: DockPlacement,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.dock
            .update(cx, |dock, cx| dock.toggle_dock(placement, window, cx));
    }

    fn command_context(&self, window: &Window, cx: &App) -> CommandContext {
        let manager = crate::connections::ConnectionManager::global(cx);
        let connected = manager.read(cx).active().is_some_and(|c| {
            matches!(
                manager.read(cx).status(c.id),
                crate::connections::ConnStatus::Connected(_)
            )
        });
        CommandContext {
            editor_open: EditorTabs::global(cx).read(cx).any_open(),
            connected,
            graph_focused: window.is_action_available(&graph::HideSelection, cx),
        }
    }

    fn open_palette(
        &mut self,
        _: &OpenCommandPalette,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let focus = self.focus_handle.clone();
        let storage = self.storage.clone();
        let on_run: palette::OnRun = Rc::new(move |action, window, cx| {
            if let Some(storage) = &storage
                && let Some(recent) = cx.try_global::<RecentCommands>()
            {
                let write = storage.set_json(storage::Table::UiState, RECENT_KEY, &recent.0);
                cx.background_spawn(async move {
                    if let Err(err) = write.await {
                        log::warn!("failed to save recent commands: {err}");
                    }
                })
                .detach();
            }
            // The dialog has closed; route the action through the workspace's handlers.
            let focus = focus.clone();
            window.defer(cx, move |window, cx| {
                if !focus.contains_focused(window, cx) {
                    window.focus(&focus, cx);
                }
                window.dispatch_action(action, cx);
            });
        });
        palette::open(self.command_context(window, cx), on_run, window, cx);
    }

    /// The workspace of `window`, if its root view is one.
    pub fn for_window(window: &Window, cx: &App) -> Option<Entity<Workspace>> {
        let root = window.root::<gpui_kit::component::Root>()??;
        root.read(cx).view().clone().downcast::<Workspace>().ok()
    }

    pub fn open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let existing = self
            .dock
            .read(cx)
            .layout(DockPlacement::Center)
            .and_then(|tree| {
                tree.panels().find(|id| {
                    self.dock
                        .read(cx)
                        .panel(*id)
                        .is_some_and(|p| p.panel_name(cx) == SettingsPanel::NAME)
                })
            });
        self.dock.update(cx, |dock, cx| match existing {
            Some(id) => dock.select_panel(id, window, cx),
            None => {
                let panel = SettingsPanel::build(window, cx);
                dock.add_panel_view(panel_handle(panel), DockPlacement::Center, None, window, cx);
            }
        });
    }

    fn show_shortcuts(
        &mut self,
        _: &ShowKeyboardShortcuts,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        crate::shortcuts::open(window, cx);
    }

    /// Show the schema graph of `connection` (default: the active connection), reusing its open tab.
    pub fn open_schema_graph(
        &mut self,
        connection: Option<uuid::Uuid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<Entity<SchemaGraphPanel>> {
        let connection = connection.or_else(|| {
            ConnectionManager::try_global(cx).and_then(|m| m.read(cx).active().map(|c| c.id))
        })?;
        let existing = self
            .dock
            .read(cx)
            .layout(DockPlacement::Center)
            .and_then(|tree| {
                tree.panels().find_map(|id| {
                    let panel = self.dock.read(cx).panel(id)?;
                    let view = panel.view().downcast::<SchemaGraphPanel>().ok()?;
                    (view.read(cx).connection() == Some(connection)).then_some((id, view))
                })
            });
        if let Some((id, view)) = existing {
            self.dock
                .update(cx, |dock, cx| dock.select_panel(id, window, cx));
            return Some(view);
        }
        let panel = SchemaGraphPanel::new(Some(connection), window, cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
        });
        Some(panel)
    }

    /// Open a graph explorer tab on `connection` (default: the active one), optionally starting from a
    /// fixed vertex `(id, name)`.
    pub fn open_explorer(
        &mut self,
        connection: Option<uuid::Uuid>,
        start: Option<(gremlin_model::GValue, String)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<Entity<ExplorerPanel>> {
        let connection = connection.or_else(|| {
            ConnectionManager::try_global(cx).and_then(|m| m.read(cx).active().map(|c| c.id))
        })?;
        let panel = ExplorerPanel::new(Some(connection), start, window, cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
        });
        Some(panel)
    }

    /// Show a sidebar panel of type `P`, adding it to the left dock when a saved layout lacks it.
    fn show_left<P: WorkspacePanel + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<P> {
        let found = [
            DockPlacement::Left,
            DockPlacement::Right,
            DockPlacement::Bottom,
            DockPlacement::Center,
        ]
        .into_iter()
        .find_map(|placement| {
            let dock = self.dock.read(cx);
            dock.layout(placement)?
                .panels()
                .find_map(|id| Some((placement, id, dock.panel(id)?.view().downcast::<P>().ok()?)))
        });
        if let Some((placement, id, view)) = found {
            if placement != DockPlacement::Center && !self.dock.read(cx).is_dock_open(placement) {
                self.toggle(placement, window, cx);
            }
            self.dock
                .update(cx, |dock, cx| dock.select_panel(id, window, cx));
            return view;
        }
        let panel = P::build(window, cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Left,
                None,
                window,
                cx,
            );
        });
        panel
    }

    pub fn show_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<HistoryPanel> {
        let panel = self.show_left::<HistoryPanel>(window, cx);
        panel.update(cx, |p, cx| p.reload(cx));
        let search = panel.read(cx).search.clone();
        search.update(cx, |s, cx| s.focus(window, cx));
        panel
    }

    pub fn show_saved(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<SavedPanel> {
        self.show_left::<SavedPanel>(window, cx)
    }

    /// Open a new report tab for saved query `row` (several per query are fine).
    pub fn open_report(
        &mut self,
        row: &storage::SavedQueryRow,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<ReportPanel> {
        let panel = ReportPanel::new(row.clone(), window, cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
        });
        panel
    }

    /// Open saved query `row`: focus the tab already showing it, or open a new one.
    pub fn open_saved(
        &mut self,
        row: &storage::SavedQueryRow,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<EditorPanel> {
        let open = EditorTabs::global(cx)
            .read(cx)
            .tabs()
            .into_iter()
            .find(|t| t.read(cx).saved_query_id() == Some(row.id.as_str()));
        if let Some(tab) = open {
            let found = self
                .dock
                .read(cx)
                .layout(DockPlacement::Center)
                .and_then(|tree| {
                    tree.panels().find(|id| {
                        self.dock
                            .read(cx)
                            .panel(*id)
                            .and_then(|p| p.view().downcast::<EditorPanel>().ok())
                            .is_some_and(|v| v == tab)
                    })
                });
            if let Some(id) = found {
                self.dock
                    .update(cx, |dock, cx| dock.select_panel(id, window, cx));
            }
            return tab;
        }
        let tab = self.new_tab(window, cx);
        let row = row.clone();
        tab.update(cx, |t, cx| t.load_saved(&row, window, cx));
        tab
    }

    /// Open a new tab with `text` bound to `connection`, without running it.
    pub fn open_query(
        &mut self,
        text: &str,
        connection: Option<uuid::Uuid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<EditorPanel> {
        let tab = self.new_tab(window, cx);
        let text = text.to_owned();
        tab.update(cx, |tab, cx| {
            if connection.is_some() {
                tab.set_connection(connection, cx);
            }
            tab.set_text(&text, window, cx);
        });
        tab
    }

    /// Open a tab with `text` bound to `connection`, and run it (the `--run` flag).
    pub fn open_and_run(
        &mut self,
        text: &str,
        connection: Option<uuid::Uuid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<EditorPanel> {
        let tab = self.new_tab(window, cx);
        let text = text.to_owned();
        tab.update(cx, |tab, cx| {
            if connection.is_some() {
                tab.set_connection(connection, cx);
            }
            let editor = tab.editor().clone();
            editor.update(cx, |e, cx| {
                e.set_text(&text, window, cx);
                e.state().update(cx, |s, cx| s.set_selected_range(0..0, cx));
            });
            tab.refresh_params(cx);
            editor.update(cx, |e, cx| {
                e.run(&crate::actions::editor::RunQuery, window, cx)
            });
        });
        tab
    }

    /// Open a new editor tab in the editor area and focus it.
    pub fn new_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<EditorPanel> {
        let tab = EditorPanel::new_tab(window, cx);
        let handle = panel_handle(tab.clone());
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(handle, DockPlacement::Center, None, window, cx);
        });
        let editor = tab.read(cx).editor().clone();
        editor.update(cx, |e, cx| e.focus(window, cx));
        tab
    }

    /// The editor tab holding focus, or the most recently opened one.
    pub fn current_tab(&self, window: &Window, cx: &App) -> Option<Entity<EditorPanel>> {
        let tabs = EditorTabs::global(cx).read(cx).tabs();
        tabs.iter()
            .find(|t| {
                let editor = t.read(cx).editor().read(cx);
                editor
                    .state()
                    .read(cx)
                    .focus_handle(cx)
                    .contains_focused(window, cx)
            })
            .cloned()
            .or_else(|| tabs.last().cloned())
    }

    /// Move the current tab into a new column to the right.
    pub fn split_right(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab) = self.current_tab(window, cx) else {
            return;
        };
        let panel = gpui_kit::component::dock::PanelId::from(tab.entity_id());
        let node = self
            .dock
            .read(cx)
            .layout(DockPlacement::Center)
            .and_then(|tree| tree.find_panel_node(panel));
        let Some(node) = node else { return };
        self.dock.update(cx, |dock, cx| {
            dock.move_panel(
                panel,
                gpui_kit::component::dock::InsertTarget::Split {
                    node,
                    placement: gpui_kit::component::Placement::Right,
                    size: None,
                },
                window,
                cx,
            );
        });
    }

    fn zoom(&mut self, delta: Option<f32>, cx: &mut Context<'_, Self>) {
        AppSettings::update_general(cx, |g| {
            g.ui_font_size = match delta {
                Some(d) => g.ui_font_size + d,
                None => DEFAULT_UI_FONT_SIZE,
            };
        });
    }

    fn render_header(&self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dock = self.dock.read(cx);
        let toggle = |id: &'static str,
                      icon: IconName,
                      open: bool,
                      tip: &'static str,
                      action: Box<dyn Action>| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .small()
                .selected(open)
                .tip(tip)
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
        };
        let palette_hint = Kbd::binding_for_action(&OpenCommandPalette, None, window);
        h_flex()
            .id("header")
            .flex_none()
            .h(Tokens::title_bar_height())
            .px(Tokens::space(2.0))
            .gap(Tokens::space(2.0))
            .bg(theme.title_bar)
            .border_b(Tokens::BORDER)
            .border_color(theme.title_bar_border)
            .child(toggle(
                "header-toggle-sidebar",
                IconName::PanelLeft,
                dock.is_dock_open(DockPlacement::Left),
                "Toggle sidebar",
                Box::new(ToggleSidebar),
            ))
            .child(
                div().flex_1().flex().justify_center().child(
                    h_flex()
                        .id("header-command-center")
                        .w(Tokens::size(420.0))
                        .h(Tokens::size(24.0))
                        .px(Tokens::space(3.0))
                        .gap(Tokens::space(2.0))
                        .rounded(Tokens::RADIUS_MD)
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.input)
                        .cursor_pointer()
                        .hover(|s| s.border_color(theme.ring.opacity(0.6)))
                        .text_size(Tokens::text_sm())
                        .text_color(theme.muted_foreground)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(OpenCommandPalette), cx)
                        })
                        .child(gpui_kit::component::Icon::new(IconName::Search).xsmall())
                        .child(div().flex_1().child("Search commands"))
                        .children(palette_hint),
                ),
            )
            .child(toggle(
                "header-toggle-results",
                IconName::PanelBottom,
                dock.is_dock_open(DockPlacement::Bottom),
                "Toggle results",
                Box::new(ToggleResultsDock),
            ))
            .child(toggle(
                "header-toggle-right",
                IconName::PanelRight,
                dock.is_dock_open(DockPlacement::Right),
                "Toggle inspector",
                Box::new(ToggleRightPanel),
            ))
            .child(
                Button::new("header-settings")
                    .icon(IconName::Settings)
                    .ghost()
                    .small()
                    .tip("Settings")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
            )
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let hud = self.perf_hud.as_mut().map(|hud| hud.frame(window, cx));
        let theme = cx.theme();
        let mut context = KeyContext::new_with_defaults();
        context.add("Workspace");
        if Runner::global(cx).read(cx).is_running() {
            context.add("QueryRunning");
        }
        v_flex()
            .id("workspace")
            .key_context(context)
            .track_focus(&self.focus_handle)
            .size_full()
            .relative()
            .bg(theme.background)
            .text_color(theme.foreground)
            .font_family(theme.font_family.clone())
            .on_action(cx.listener(Self::open_palette))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::show_shortcuts))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.toggle(DockPlacement::Left, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleResultsDock, window, cx| {
                this.toggle(DockPlacement::Bottom, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleRightPanel, window, cx| {
                this.toggle(DockPlacement::Right, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ResetLayout, window, cx| {
                Self::install_default_layout(&this.dock, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewQueryTab, window, cx| {
                this.new_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| this.split_right(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleAssistant, window, cx| {
                this.toggle_assistant(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FixQueryError, window, cx| {
                this.fix_query_error(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowHistory, window, cx| {
                this.show_history(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowSavedQueries, window, cx| {
                this.show_saved(window, cx);
            }))
            .on_action(cx.listener(|this, a: &OpenGraphExplorer, window, cx| {
                this.open_explorer(a.connection, None, window, cx);
            }))
            .on_action(cx.listener(|this, a: &OpenSchemaGraph, window, cx| {
                this.open_schema_graph(a.connection, window, cx);
            }))
            .on_action(cx.listener(|this, a: &OpenSampleQuery, window, cx| {
                this.open_and_run(&a.query, Some(a.connection), window, cx);
            }))
            .on_action(cx.listener(|_, a: &RefreshSchema, _, cx| {
                let id = a.connection.or_else(|| {
                    ConnectionManager::try_global(cx)
                        .and_then(|m| m.read(cx).active().map(|c| c.id))
                });
                if let (Some(id), Some(registry)) =
                    (id, crate::schema::SchemaRegistry::try_global(cx))
                {
                    registry.update(cx, |r, cx| r.refresh(id, cx));
                }
            }))
            .on_action(cx.listener(|_, _: &CancelQuery, _, cx| {
                Runner::global(cx).update(cx, |r, cx| r.cancel(cx));
            }))
            .on_action(cx.listener(|this, _: &SaveQuery, window, cx| {
                if let Some(tab) = this.current_tab(window, cx) {
                    EditorPanel::save(&tab, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom(Some(FONT_SIZE_STEP), cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom(Some(-FONT_SIZE_STEP), cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.zoom(None, cx)))
            .on_action(cx.listener(|_, _: &ResetGeneralSettings, window, cx| {
                confirm_reset_general(window, cx);
            }))
            .on_action(cx.listener(|_, _: &AddConnection, window, cx| {
                crate::connections::form::open(None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                this.save_now(window, cx);
                if let Some(storage) = &this.storage {
                    // Quitting is the one place we wait for the storage thread.
                    let _ = storage.flush_blocking();
                }
                cx.quit();
            }))
            .child(self.render_header(window, cx))
            .child(div().flex_1().min_h_0().child(self.dock.clone()))
            .child(self.status_bar.clone())
            .children(hud)
            .children(self.tour.clone().map(|tour| {
                div()
                    .absolute()
                    .right(Tokens::space(4.0))
                    .bottom(Tokens::space(10.0))
                    .child(tour)
            }))
    }
}
