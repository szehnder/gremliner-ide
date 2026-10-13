//! The assistant panel (right dock): one conversation per editor tab, following the active tab. It
//! gathers each turn's context from the tab (statement at the cursor, last error and result, schema,
//! privacy), runs the turn on the I/O runtime, and applies Insert/Run/highlight requests to the tab.

use std::collections::HashMap;

use assistant::chat::{Mode, ResultSummary, Session, TurnEvent, TurnInput};
use futures::StreamExt as _;
use gpui_kit::component::dock::{BasePanel, DockPlacement, Panel, PanelControl};
use gpui_kit::component::{ActiveTheme as _, IconName};
use gpui_kit::*;
use ui_assistant::{Available, ChatView, ChatViewEvent, ChipKind};
use ui_results::RunStatus;
use uuid::Uuid;

use super::{EditorPanel, EditorTabs, EditorTabsEvent, WorkspacePanel, empty_state, panel_basics};
use crate::ai::LlmProfiles;
use crate::connections::ConnectionManager;
use crate::run::{Runner, RunnerEvent};
use crate::schema::SchemaRegistry;

pub struct AssistantPanel {
    focus_handle: FocusHandle,
    views: HashMap<Uuid, Entity<ChatView>>,
    /// `None` while a turn is running (the session is on the I/O runtime).
    sessions: HashMap<Uuid, Option<Session>>,
    /// Where the explained statement starts in the editor, per tab.
    explained_at: HashMap<Uuid, usize>,
    tasks: HashMap<Uuid, Task<()>>,
    _subscriptions: Vec<Subscription>,
}

panel_basics!(AssistantPanel);

impl WorkspacePanel for AssistantPanel {
    const NAME: &'static str = "gremliner.assistant";
    const DEFAULT_PLACEMENT: DockPlacement = DockPlacement::Right;

    fn build(_: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let tabs = EditorTabs::global(cx);
            let mut subscriptions = vec![
                cx.subscribe(&tabs, |this: &mut Self, _, e: &EditorTabsEvent, cx| {
                    if matches!(
                        e,
                        EditorTabsEvent::ActiveChanged | EditorTabsEvent::TabsChanged
                    ) {
                        this.refresh(cx);
                        cx.notify();
                    }
                }),
                cx.subscribe(
                    &Runner::global(cx),
                    |this: &mut Self, _, _: &RunnerEvent, cx| this.refresh(cx),
                ),
            ];
            if let Some(r) = SchemaRegistry::try_global(cx) {
                subscriptions.push(cx.observe(&r, |this, _, cx| this.refresh(cx)));
            }
            if let Some(p) = LlmProfiles::try_global(cx) {
                subscriptions.push(cx.observe(&p, |this, _, cx| this.refresh(cx)));
            }
            Self {
                focus_handle: cx.focus_handle(),
                views: HashMap::new(),
                sessions: HashMap::new(),
                explained_at: HashMap::new(),
                tasks: HashMap::new(),
                _subscriptions: subscriptions,
            }
        })
    }
}

impl BasePanel for AssistantPanel {
    fn panel_name(&self) -> &'static str {
        Self::NAME
    }
}

impl Panel for AssistantPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        "Assistant"
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

fn tab_by_id(id: Uuid, cx: &App) -> Option<Entity<EditorPanel>> {
    EditorTabs::global(cx)
        .read(cx)
        .tabs()
        .into_iter()
        .find(|t| t.read(cx).id() == id)
}

impl AssistantPanel {
    /// The conversation view of `tab` (created on first use).
    pub fn view_for(
        &mut self,
        tab: &Entity<EditorPanel>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Entity<ChatView> {
        let id = tab.read(cx).id();
        if let Some(v) = self.views.get(&id) {
            return v.clone();
        }
        let view = cx.new(|cx| ChatView::new(window, cx));
        let sub = cx.subscribe_in(
            &view,
            window,
            move |this, _, e: &ChatViewEvent, window, cx| {
                this.on_view_event(id, e.clone(), window, cx);
            },
        );
        self._subscriptions.push(sub);
        self.views.insert(id, view.clone());
        self.sessions.insert(id, Some(Session::default()));
        self.refresh(cx);
        view
    }

    /// The conversation of `tab`, if it has one.
    pub fn view(&self, tab: Uuid) -> Option<&Entity<ChatView>> {
        self.views.get(&tab)
    }

    /// Update each view's available context.
    fn refresh(&mut self, cx: &mut Context<'_, Self>) {
        let ids: Vec<Uuid> = self.views.keys().copied().collect();
        for id in ids {
            let Some(tab) = tab_by_id(id, cx) else {
                continue;
            };
            let available = available(&tab, cx);
            if let Some(v) = self.views.get(&id) {
                v.update(cx, |v, cx| v.set_available(available, cx));
            }
        }
    }

    fn on_view_event(
        &mut self,
        id: Uuid,
        event: ChatViewEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab) = tab_by_id(id, cx) else { return };
        match event {
            ChatViewEvent::Send {
                mode,
                question,
                include,
            } => self.run_turn(id, &tab, mode, question, &include, window, cx),
            ChatViewEvent::Insert(q) => tab.update(cx, |t, cx| t.insert_statement(&q, window, cx)),
            ChatViewEvent::Run { query, writes } => {
                tab.update(cx, |t, cx| t.run_suggestion(&query, writes, window, cx))
            }
            ChatViewEvent::HighlightSteps(ranges) => {
                let offset = self.explained_at.get(&id).copied().unwrap_or(0);
                let shifted = ranges
                    .into_iter()
                    .map(|r| r.start + offset..r.end + offset)
                    .collect();
                let editor = tab.read(cx).editor().clone();
                editor.update(cx, |e, cx| e.set_hint_ranges(shifted, cx));
            }
            ChatViewEvent::Clear => {
                self.sessions.insert(id, Some(Session::default()));
                self.tasks.remove(&id);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_turn(
        &mut self,
        id: Uuid,
        tab: &Entity<EditorPanel>,
        mode: Mode,
        question: String,
        include: &[ChipKind],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(view) = self.views.get(&id).cloned() else {
            return;
        };
        let fail = |view: &Entity<ChatView>, msg: &str, cx: &mut Context<'_, Self>| {
            view.update(cx, |v, cx| {
                v.apply(TurnEvent::Error(msg.into()), cx);
                v.apply(TurnEvent::Done, cx);
            });
        };
        let Some(connection) = tab.read(cx).connection_id() else {
            fail(&view, "Choose a connection for this tab first.", cx);
            return;
        };
        let Some(profiles) = LlmProfiles::try_global(cx) else {
            return;
        };
        let Some(profile) = profiles.read(cx).resolve(Some(connection)).cloned() else {
            fail(
                &view,
                "No language model is configured. Add one in Settings \u{2192} AI.",
                cx,
            );
            return;
        };
        let Some(session) = self.sessions.get_mut(&id).and_then(Option::take) else {
            return;
        };
        let has = |k: ChipKind| include.contains(&k);
        let share_samples = has(ChipKind::Samples) && profiles.read(cx).share_samples(connection);
        let target = tab.read(cx).editor().read(cx).run_target(cx);
        if let Some((_, range)) = &target {
            self.explained_at.insert(id, range.start);
        }
        let last = Runner::global(cx).read(cx).result_for_tab(id).cloned();
        let (error, items) = match &last {
            Some(r) => {
                let r = r.read(cx);
                match &r.status {
                    RunStatus::Failed(e) => (Some(e.message.clone()), None),
                    RunStatus::Succeeded => (None, Some(r.items.clone())),
                    _ => (None, None),
                }
            }
            None => (None, None),
        };
        let input = TurnInput {
            mode: Some(mode),
            question,
            query: target
                .map(|(t, _)| t.trim().to_owned())
                .filter(|_| has(ChipKind::Query)),
            error: error.filter(|_| has(ChipKind::Error)),
            result: items
                .filter(|_| has(ChipKind::Result))
                .map(|i| ResultSummary::of(&i, share_samples)),
            tools: profile.capabilities().tools,
        };
        let manager = ConnectionManager::global(cx);
        let schema = SchemaRegistry::try_global(cx)
            .and_then(|r| r.read(cx).schema(connection))
            .filter(|_| has(ChipKind::Schema));
        let neptune = manager
            .read(cx)
            .get(connection)
            .is_some_and(|c| c.provider.kind().is_neptune());
        let client_task = profiles.read(cx).client(profile, None, cx);
        let weak_view = view.downgrade();
        let task = cx.spawn_in(window, async move |this, cx| {
            if manager.read_with(cx, |m, _| m.session(connection).is_none()) {
                manager.update(cx, |m, cx| m.connect(connection, cx)).await;
            }
            let db = manager.read_with(cx, |m, _| m.session(connection).cloned());
            let finish = |session: Session, cx: &mut AsyncWindowContext| {
                let _ = this.update(cx, |this, _| {
                    this.sessions.insert(id, Some(session));
                });
            };
            let client = match client_task.await {
                Ok(c) => c,
                Err(e) => {
                    let _ = weak_view.update(cx, |v, cx| {
                        v.apply(TurnEvent::Error(e.to_string()), cx);
                        v.apply(TurnEvent::Done, cx);
                    });
                    finish(session, cx);
                    return;
                }
            };
            let ctx = assistant::Context {
                schema,
                dialect: assistant::Dialect {
                    tinkerpop_version: db.as_ref().and_then(|s| s.info.tinkerpop_version.clone()),
                    neptune,
                },
                current_query: None,
                share_samples,
            };
            let (tx, mut rx) = futures::channel::mpsc::unbounded::<TurnEvent>();
            let work = cx.update(|_, cx| {
                crate::io::spawn_io(cx, async move {
                    let mut session = session;
                    let emit = move |e: TurnEvent| {
                        let _ = tx.unbounded_send(e);
                    };
                    let runner = db.as_ref().map(|s| &s.client as &dyn assistant::DryRunner);
                    assistant::chat::turn(&mut session, &input, &ctx, &client, runner, true, &emit)
                        .await;
                    session
                })
            });
            let Ok(work) = work else { return };
            while let Some(event) = rx.next().await {
                if weak_view.update(cx, |v, cx| v.apply(event, cx)).is_err() {
                    break;
                }
            }
            match work.await {
                Ok(session) => finish(session, cx),
                Err(_) => finish(Session::default(), cx),
            }
        });
        self.tasks.insert(id, task);
    }
}

/// What context exists for `tab` right now.
fn available(tab: &Entity<EditorPanel>, cx: &App) -> Available {
    let t = tab.read(cx);
    let id = t.id();
    let query = t.editor().read(cx).run_target(cx).is_some();
    let result = Runner::global(cx)
        .read(cx)
        .result_for_tab(id)
        .map(|r| r.read(cx).status.clone());
    let connection = t.connection_id();
    Available {
        query,
        error: matches!(result, Some(RunStatus::Failed(_))),
        result: matches!(result, Some(RunStatus::Succeeded)),
        schema: connection.is_some_and(|c| {
            SchemaRegistry::try_global(cx).is_some_and(|r| r.read(cx).schema(c).is_some())
        }),
        samples: connection.is_some_and(|c| {
            LlmProfiles::try_global(cx).is_some_and(|p| p.read(cx).share_samples(c))
        }),
    }
}

impl Render for AssistantPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let active = EditorTabs::global(cx).read(cx).active(cx);
        let base = div()
            .id("assistant-panel")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background);
        match active {
            Some(tab) => {
                let view = self.view_for(&tab, window, cx);
                let available = available(&tab, cx);
                view.update(cx, |v, cx| v.set_available(available, cx));
                base.child(view)
            }
            None => base.child(empty_state(
                IconName::Bot,
                "No query tab",
                "Open a query tab to ask the assistant about it.",
                cx,
            )),
        }
    }
}
