//! The run pipeline: editor run request → read-only guard → implicit `.toList()` → connect if needed →
//! stream batches into a `ResultSet` on the I/O runtime, honouring the result cap and Cancel (ADR-0005,
//! ADR-0020).

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::{mpsc, oneshot};
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, WindowExt as _};
use gpui_kit::*;
use gremlin_client::{Error as ClientError, Request};
use gremlin_model::GValue;
use providers::read_only;
use ui_editor::RunMode;
use ui_results::graph_view::{LayoutSettings, MutationRequest};
use ui_results::{
    Analysis, FetchRequest, GraphView, GraphViewEvent, ResultSet, RunError, RunStatus, error_range,
};
use uuid::Uuid;

use crate::connections::{ConnStatus, ConnectionManager};
use crate::io::spawn_io;
use crate::panels::{EditorTabs, RunRequest};
use crate::settings::AppSettings;

enum Msg {
    Batch(Vec<GValue>, Option<String>),
    Done,
    Failed(ClientError),
    Cancelled,
}

struct Running {
    run_id: u64,
    cancel: Option<oneshot::Sender<()>>,
}

/// Items a graph follow-up query may return before it is cut off.
const FETCH_LIMIT: usize = 20_000;

/// Owns the current run, the latest result per editor tab, and each tab's graph (which runs its
/// follow-up queries here, against the tab's connection).
#[derive(Default)]
pub struct Runner {
    next_run: u64,
    running: Option<Running>,
    latest: Option<Entity<ResultSet>>,
    latest_tab: Option<Uuid>,
    per_tab: HashMap<Uuid, Entity<ResultSet>>,
    graphs: HashMap<Uuid, Entity<GraphView>>,
    /// The connection each tab last ran against (graph follow-ups use it).
    tab_connection: HashMap<Uuid, Uuid>,
    _graph_subscriptions: Vec<Subscription>,
}

impl EventEmitter<RunnerEvent> for Runner {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerEvent {
    /// A new result set started; show it.
    Started,
    /// The running state changed (for Esc-to-cancel and toolbar state).
    RunningChanged,
}

struct RunnerGlobal(Entity<Runner>);

impl Global for RunnerGlobal {}

impl Runner {
    pub fn init(cx: &mut App) {
        let entity = cx.new(|_| Runner::default());
        cx.set_global(RunnerGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Runner> {
        cx.global::<RunnerGlobal>().0.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn latest(&self) -> Option<&Entity<ResultSet>> {
        self.latest.as_ref()
    }

    /// The tab that ran last.
    pub fn latest_tab(&self) -> Option<Uuid> {
        self.latest_tab
    }

    pub fn result_for_tab(&self, tab: Uuid) -> Option<&Entity<ResultSet>> {
        self.per_tab.get(&tab)
    }

    /// The graph of the tab that ran last.
    pub fn latest_graph(&self) -> Option<&Entity<GraphView>> {
        self.graphs.get(&self.latest_tab?)
    }

    pub fn graph_for_tab(&self, tab: Uuid) -> Option<&Entity<GraphView>> {
        self.graphs.get(&tab)
    }

    fn ensure_graph(&mut self, tab: Uuid, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.graphs.contains_key(&tab) {
            return;
        }
        let graph = cx.new(|cx| GraphView::new(window, cx));
        // The tab's saved layout choice (applied without running; the next result lays out with it).
        if let Some(storage) = AppSettings::storage(cx) {
            let read =
                storage.get_json::<serde_json::Value>(storage::Table::UiState, &layout_key(tab));
            let graph = graph.downgrade();
            cx.spawn(async move |_, cx| {
                if let Ok(Some(saved)) = read.await {
                    let settings = LayoutSettings::from_json(&saved);
                    let _ = graph.update(cx, |g, cx| g.set_layout_settings(settings, false, cx));
                }
            })
            .detach();
        }
        self._graph_subscriptions.push(cx.subscribe_in(
            &graph,
            window,
            move |this, graph, event: &GraphViewEvent, window, cx| match event {
                GraphViewEvent::Fetch(request) => {
                    this.fetch(tab, graph.clone(), request.clone(), cx)
                }
                GraphViewEvent::Mutate(request) => {
                    this.mutate(tab, graph.clone(), request.clone(), window, cx)
                }
                GraphViewEvent::LayoutChanged(settings) => {
                    if let Some(storage) = AppSettings::storage(cx) {
                        let write = storage.set_json(
                            storage::Table::UiState,
                            &layout_key(tab),
                            &settings.to_json(),
                        );
                        cx.background_spawn(async move {
                            if let Err(err) = write.await {
                                log::warn!("failed to save the graph layout: {err}");
                            }
                        })
                        .detach();
                    }
                }
                GraphViewEvent::ShowInTable(_) => {}
                GraphViewEvent::SearchPathFrom { id, name } => {
                    let connection = this.tab_connection.get(&tab).copied();
                    let start = (id.clone(), name.clone());
                    window.defer(cx, move |window, cx| {
                        if let Some(ws) = crate::Workspace::for_window(window, cx) {
                            ws.update(cx, |ws, cx| {
                                ws.open_explorer(connection, Some(start), window, cx)
                            });
                        }
                    });
                }
            },
        ));
        self.graphs.insert(tab, graph);
    }

    /// Run a confirmed data change from the graph. Production and read-only connections ask once more,
    /// naming the connection.
    fn mutate(
        &mut self,
        tab: Uuid,
        graph: Entity<GraphView>,
        request: MutationRequest,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let Some(config) = self
            .tab_connection
            .get(&tab)
            .and_then(|id| manager.read(cx).get(*id).cloned())
        else {
            graph.update(cx, |g, cx| {
                g.apply_mutation(request.id, Err("no connection".into()), cx)
            });
            return;
        };
        let production = config.environment == providers::Environment::Production;
        if !(production || config.read_only) {
            self.run_mutation(config.id, graph, request, cx);
            return;
        }
        let why = if production {
            "a production connection"
        } else {
            "a read-only connection"
        };
        let name = config.name.clone();
        let description = format!(
            "“{name}” is {why}. {} — this changes data in the database.",
            request.description
        );
        let runner = cx.weak_entity();
        let connection_id = config.id;
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let runner = runner.clone();
            let graph = graph.clone();
            let request = request.clone();
            let cancel_graph = graph.clone();
            let cancel_id = request.id;
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning))
                .title(format!("Change data on “{name}”?"))
                .description(description.clone())
                .ok_text(format!("Run on {name}"))
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let graph = graph.clone();
                    let request = request.clone();
                    let _ = runner.update(cx, |r, cx| {
                        r.run_mutation(connection_id, graph, request, cx)
                    });
                    true
                })
                .on_cancel(move |_, _, cx| {
                    cancel_graph.update(cx, |g, cx| {
                        g.apply_mutation(cancel_id, Err("cancelled".into()), cx)
                    });
                    true
                })
        });
    }

    fn run_mutation(
        &mut self,
        connection_id: Uuid,
        graph: Entity<GraphView>,
        request: MutationRequest,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let timeout = manager
            .read(cx)
            .get(connection_id)
            .map(|c| Duration::from_millis(c.timeout_ms))
            .unwrap_or(Duration::from_secs(30));
        cx.spawn(async move |_, cx| {
            let needs_connect = manager.read_with(cx, |m, _| m.session(connection_id).is_none());
            if needs_connect {
                manager
                    .update(cx, |m, cx| m.connect(connection_id, cx))
                    .await;
            }
            let client = manager.read_with(cx, |m, _| {
                m.session(connection_id).map(|s| s.client.clone())
            });
            let result = match client {
                None => Err("not connected".to_owned()),
                Some(client) => {
                    let statement = request.statement.clone();
                    let work = cx.update(|cx| {
                        spawn_io(cx, async move {
                            let mut req = Request::new(statement.script).timeout(timeout);
                            for (k, v) in statement.bindings {
                                req = req.binding(k, v);
                            }
                            client
                                .execute_request(req)
                                .await
                                .map(|r| r.items)
                                .map_err(|e| e.to_string())
                        })
                    });
                    match work.await {
                        Ok(r) => r,
                        Err(err) => Err(err.to_string()),
                    }
                }
            };
            graph.update(cx, |g, cx| g.apply_mutation(request.id, result, cx));
        })
        .detach();
    }

    /// Run a graph follow-up query on the tab's connection and hand the items back to the graph.
    fn fetch(
        &mut self,
        tab: Uuid,
        graph: Entity<GraphView>,
        request: FetchRequest,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let Some(connection_id) = self.tab_connection.get(&tab).copied() else {
            graph.update(cx, |g, cx| {
                g.apply_fetch(&request, Err("no connection".into()), cx)
            });
            return;
        };
        let timeout = manager
            .read(cx)
            .get(connection_id)
            .map(|c| Duration::from_millis(c.timeout_ms))
            .unwrap_or(Duration::from_secs(30));
        cx.spawn(async move |_, cx| {
            let needs_connect = manager.read_with(cx, |m, _| m.session(connection_id).is_none());
            if needs_connect {
                manager
                    .update(cx, |m, cx| m.connect(connection_id, cx))
                    .await;
            }
            let client = manager.read_with(cx, |m, _| {
                m.session(connection_id).map(|s| s.client.clone())
            });
            let result = match client {
                None => Err("not connected".to_owned()),
                Some(client) => {
                    let script = request.script.clone();
                    let work = cx.update(|cx| {
                        spawn_io(cx, async move {
                            collect(client, script, timeout, FETCH_LIMIT).await
                        })
                    });
                    match work.await {
                        Ok(r) => r,
                        Err(err) => Err(err.to_string()),
                    }
                }
            };
            graph.update(cx, |g, cx| g.apply_fetch(&request, result, cx));
        })
        .detach();
    }

    /// Stop the running query; results so far stay, marked cancelled.
    pub fn cancel(&mut self, cx: &mut Context<'_, Self>) {
        self.stop(RunStatus::Cancelled, cx);
    }

    /// Stop streaming; the result keeps what arrived and is marked with `status` right away (queued
    /// batches are ignored).
    fn stop(&mut self, status: RunStatus, cx: &mut Context<'_, Self>) {
        if let Some(running) = self.running.as_mut()
            && let Some(cancel) = running.cancel.take()
        {
            let _ = cancel.send(());
            if let Some(result) = &self.latest {
                result.update(cx, |r, cx| r.finish(status, cx));
            }
        }
        cx.notify();
    }

    /// Handle a run request: guard, confirm if needed, then execute.
    pub fn request(
        &mut self,
        request: RunRequest,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let Some(config) = request
            .connection_id
            .and_then(|id| manager.read(cx).get(id).cloned())
        else {
            window.push_notification(
                Notification::warning("Choose a connection for this tab, then run again.")
                    .title("No connection"),
                cx,
            );
            return;
        };
        // Explain plans without executing, so nothing can change.
        if request.mode == RunMode::Explain {
            self.execute(request, window, cx);
            return;
        }
        // Profiling executes the traversal: a mutation would really run.
        if request.mode == RunMode::Profile && gremlin_syntax::is_mutating(&request.text) {
            let mut description = format!(
                "Profiling runs the traversal to measure it, so this statement will modify data on “{}”.",
                config.name
            );
            if config.read_only {
                description.push_str(" The connection is read-only.");
            }
            let runner = cx.weak_entity();
            window.open_alert_dialog(cx, move |alert, _, cx| {
                let request = request.clone();
                let runner = runner.clone();
                alert
                    .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning))
                    .title("Profile a query that changes data?")
                    .description(description.clone())
                    .ok_text("Profile and modify data")
                    .ok_variant(ButtonVariant::Danger)
                    .show_cancel(true)
                    .on_ok(move |_, window, cx| {
                        let request = request.clone();
                        let _ = runner.update(cx, |runner, cx| runner.execute(request, window, cx));
                        true
                    })
            });
            return;
        }
        if let Err(blocked) = read_only::check(&config.name, config.read_only, &request.text, false)
        {
            let message = blocked.to_string();
            let runner = cx.weak_entity();
            window.open_alert_dialog(cx, move |alert, _, cx| {
                let request = request.clone();
                let runner = runner.clone();
                alert
                    .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().warning))
                    .title("Read-only connection")
                    .description(message.clone())
                    .ok_text("Run anyway")
                    .ok_variant(ButtonVariant::Danger)
                    .show_cancel(true)
                    .on_ok(move |_, window, cx| {
                        let request = request.clone();
                        let _ = runner.update(cx, |runner, cx| runner.execute(request, window, cx));
                        true
                    })
            });
            return;
        }
        self.execute(request, window, cx);
    }

    /// Run without the read-only check (the user confirmed, or the connection is writable).
    pub fn execute(
        &mut self,
        request: RunRequest,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let Some(connection_id) = request.connection_id else {
            return;
        };
        let Some(config) = manager.read(cx).get(connection_id).cloned() else {
            return;
        };
        // One run at a time: a new run cancels the previous one.
        self.cancel(cx);
        self.next_run += 1;
        let run_id = self.next_run;
        let sent = match request.mode {
            RunMode::Run => gremlin_syntax::ensure_terminal(&request.text),
            RunMode::Profile => format!(
                "{}.profile()",
                providers::analysis::traversal_part(&request.text)
            ),
            RunMode::Explain => format!(
                "{}.explain()",
                providers::analysis::traversal_part(&request.text)
            ),
        };
        let cap = AppSettings::general(cx).result_limit.max(1);
        let result = cx.new(|_| {
            ResultSet::new(
                run_id,
                request.text.clone(),
                sent.clone(),
                config.name.clone(),
                cap,
            )
        });
        self.latest = Some(result.clone());
        self.latest_tab = Some(request.tab_id);
        self.per_tab.insert(request.tab_id, result.clone());
        self.tab_connection.insert(request.tab_id, connection_id);
        self.ensure_graph(request.tab_id, window, cx);
        if let Some(graph) = self.graphs.get(&request.tab_id) {
            let conn = connection_id.to_string();
            graph.update(cx, |g, cx| g.set_connection(&conn, cx));
        }
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.running = Some(Running {
            run_id,
            cancel: Some(cancel_tx),
        });
        cx.emit(RunnerEvent::Started);
        cx.emit(RunnerEvent::RunningChanged);
        cx.notify();

        let timeout = Duration::from_millis(config.timeout_ms);
        if request.mode != RunMode::Run {
            drop(cancel_rx);
            self.run_analysis(run_id, result, request, connection_id, timeout, window, cx);
            return;
        }
        let base = request.range.start;
        let tab_id = request.tab_id;
        let generated = request.generated;
        let bindings = request.bindings.clone();
        let script = request.text.clone();
        cx.spawn_in(window, async move |this, cx| {
            // Connect first if needed.
            let needs_connect = manager.read_with(cx, |m, _| m.session(connection_id).is_none());
            if needs_connect {
                manager
                    .update(cx, |m, cx| m.connect(connection_id, cx))
                    .await;
            }
            let session = manager.read_with(cx, |m, _| {
                (m.session(connection_id).cloned(), m.status(connection_id))
            });
            let client = match session {
                (Some(session), _) => session.client,
                (None, status) => {
                    let message = match status {
                        ConnStatus::Error(err) => err.reason,
                        _ => "Not connected.".to_owned(),
                    };
                    result.update(cx, |r, cx| {
                        r.finish(
                            RunStatus::Failed(RunError {
                                message,
                                code: None,
                                exceptions: Vec::new(),
                                range: None,
                            }),
                            cx,
                        );
                    });
                    let _ = this.update(cx, |this, cx| this.clear_running(run_id, cx));
                    return;
                }
            };

            // Stream on the I/O runtime; batches cross back over a channel.
            let (tx, mut rx) = mpsc::unbounded::<Msg>();
            let io = cx.update(|_, cx| {
                spawn_io(cx, stream(client, sent, bindings, timeout, cancel_rx, tx))
            });
            let Ok(io) = io else { return };
            let mut cap_reached = false;
            while let Some(msg) = rx.next().await {
                result.update(cx, |r, cx| match msg {
                    Msg::Batch(items, raw) => {
                        if !r.append(items, raw, cx) {
                            cap_reached = true;
                        }
                    }
                    Msg::Done => r.finish(RunStatus::Succeeded, cx),
                    Msg::Cancelled if cap_reached => r.finish(RunStatus::Succeeded, cx),
                    Msg::Cancelled => r.finish(RunStatus::Cancelled, cx),
                    Msg::Failed(err) => {
                        r.finish(RunStatus::Failed(run_error(&err, &script, base)), cx)
                    }
                });
                if cap_reached {
                    // Stop streaming; the result is complete at the cap.
                    let _ = this.update(cx, |this, cx| this.stop(RunStatus::Succeeded, cx));
                }
            }
            drop(io);
            result.update(cx, |r, cx| r.finish(RunStatus::Cancelled, cx));
            let _ = this.update(cx, |this, cx| this.clear_running(run_id, cx));
            let labels = result.read_with(cx, |r, _| result_labels(&r.items));
            if !labels.is_empty() {
                let _ = cx.update(|_, cx| {
                    if let Some(registry) = crate::schema::SchemaRegistry::try_global(cx) {
                        registry.update(cx, |reg, cx| {
                            reg.note_result_labels(
                                connection_id,
                                labels.iter().map(String::as_str),
                                cx,
                            );
                        });
                    }
                });
            }
            let error_range = result.read_with(cx, |r, _| match &r.status {
                RunStatus::Failed(e) if !generated => e.range.clone(),
                _ => None,
            });
            let _ = cx.update(|window, cx| {
                let tabs = EditorTabs::global(cx).read(cx).tabs();
                if let Some(tab) = tabs.into_iter().find(|t| t.read(cx).id() == tab_id) {
                    let editor = tab.read(cx).editor().clone();
                    editor.update(cx, |e, cx| e.set_error_range(error_range, cx));
                    let _ = window;
                }
            });
        })
        .detach();
    }

    /// Profile or explain the statement and attach the report to `result`.
    fn run_analysis(
        &mut self,
        run_id: u64,
        result: Entity<ResultSet>,
        request: RunRequest,
        connection_id: Uuid,
        timeout: Duration,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let manager = ConnectionManager::global(cx);
        let mode = request.mode;
        let script = request.text.clone();
        let base = request.range.start;
        cx.spawn_in(window, async move |this, cx| {
            let fail = |message: String, cx: &mut AsyncWindowContext| {
                result.update(cx, |r, cx| {
                    r.finish(
                        RunStatus::Failed(RunError {
                            message,
                            code: None,
                            exceptions: Vec::new(),
                            range: None,
                        }),
                        cx,
                    )
                });
            };
            if manager.read_with(cx, |m, _| m.session(connection_id).is_none()) {
                manager
                    .update(cx, |m, cx| m.connect(connection_id, cx))
                    .await;
            }
            let session = manager.read_with(cx, |m, _| {
                (m.session(connection_id).cloned(), m.status(connection_id))
            });
            let session = match session {
                (Some(s), _) => s,
                (None, status) => {
                    let message = match status {
                        ConnStatus::Error(err) => err.reason,
                        _ => "Not connected.".to_owned(),
                    };
                    fail(message, cx);
                    let _ = this.update(cx, |this, cx| this.clear_running(run_id, cx));
                    return;
                }
            };
            let text = script.clone();
            let work = cx.update(|_, cx| {
                spawn_io(cx, async move {
                    match mode {
                        RunMode::Explain => providers::analysis::explain(&session, &text, timeout)
                            .await
                            .map(Analysis::Explain),
                        _ => providers::analysis::profile(&session, &text, timeout)
                            .await
                            .map(|report| {
                                let names: Vec<&str> =
                                    report.steps.iter().map(|s| s.name.as_str()).collect();
                                let ranges = gremlin_syntax::profile_match::profile_step_ranges(
                                    &text, &names,
                                )
                                .into_iter()
                                .map(|r| r.map(|r| base + r.start..base + r.end))
                                .collect();
                                Analysis::Profile { report, ranges }
                            }),
                    }
                })
            });
            let Ok(work) = work else { return };
            match work.await {
                Ok(Ok(analysis)) => result.update(cx, |r, cx| {
                    if r.is_running() {
                        r.set_analysis(analysis, cx);
                    }
                }),
                Ok(Err(err)) => fail(err.reason, cx),
                Err(_) => result.update(cx, |r, cx| r.finish(RunStatus::Cancelled, cx)),
            }
            let _ = this.update(cx, |this, cx| this.clear_running(run_id, cx));
        })
        .detach();
    }

    fn clear_running(&mut self, run_id: u64, cx: &mut Context<'_, Self>) {
        if self.running.as_ref().is_some_and(|r| r.run_id == run_id) {
            self.running = None;
            cx.emit(RunnerEvent::RunningChanged);
            cx.notify();
        }
    }
}

/// The I/O side: submit, forward batches, stop on cancel.
async fn stream(
    client: gremlin_client::Client,
    script: String,
    bindings: Vec<(String, GValue)>,
    timeout: Duration,
    mut cancel: oneshot::Receiver<()>,
    tx: mpsc::UnboundedSender<Msg>,
) {
    let mut request = Request::new(script).timeout(timeout);
    for (name, value) in bindings {
        request = request.binding(name, value);
    }
    let mut results = match client.submit(request).await {
        Ok(stream) => stream,
        Err(err) => {
            let _ = tx.unbounded_send(Msg::Failed(err));
            return;
        }
    };
    loop {
        tokio::select! {
            _ = &mut cancel => {
                results.cancel();
                let _ = tx.unbounded_send(Msg::Cancelled);
                return;
            }
            next = results.next() => match next {
                Some(Ok(batch)) => {
                    let last = batch.status != 206;
                    if tx.unbounded_send(Msg::Batch(batch.items, batch.raw)).is_err() {
                        results.cancel();
                        return;
                    }
                    if last {
                        let _ = tx.unbounded_send(Msg::Done);
                        return;
                    }
                }
                Some(Err(err)) => {
                    let _ = tx.unbounded_send(Msg::Failed(err));
                    return;
                }
                None => {
                    let _ = tx.unbounded_send(Msg::Done);
                    return;
                }
            },
        }
    }
}

/// `ui_state` key of a tab's graph layout settings.
/// Distinct element labels in (the first part of) a result, for the schema's unknown-label check.
fn result_labels(items: &[GValue]) -> Vec<String> {
    const SCAN: usize = 5_000;
    let items = &items[..items.len().min(SCAN)];
    if !gremlin_model::elements::has_graph_elements(items) {
        return Vec::new();
    }
    let set = gremlin_model::elements::elements(items);
    let mut labels: Vec<String> = set
        .vertices
        .iter()
        .map(|v| v.label.clone())
        .chain(set.edges.iter().map(|e| e.label.clone()))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

pub fn layout_key(tab: Uuid) -> String {
    format!("graph.layout.{tab}")
}

/// Run `script` to completion (up to `limit` items) and return the items.
async fn collect(
    client: gremlin_client::Client,
    script: String,
    timeout: Duration,
    limit: usize,
) -> Result<Vec<GValue>, String> {
    let mut results = client
        .submit(Request::new(script).timeout(timeout))
        .await
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    while let Some(batch) = results.next().await {
        let batch = batch.map_err(|e| e.to_string())?;
        let last = batch.status != 206;
        items.extend(batch.items);
        if items.len() >= limit {
            results.cancel();
            items.truncate(limit);
            break;
        }
        if last {
            break;
        }
    }
    Ok(items)
}

fn run_error(err: &ClientError, script: &str, base: usize) -> RunError {
    match err {
        ClientError::Server {
            code,
            message,
            exceptions,
            ..
        } => RunError {
            message: message.clone(),
            code: Some(*code),
            exceptions: exceptions.clone(),
            range: error_range(message, script, base),
        },
        other => RunError {
            message: other.to_string(),
            code: None,
            exceptions: Vec::new(),
            range: None,
        },
    }
}
