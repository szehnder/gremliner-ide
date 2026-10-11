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
use ui_results::{ResultSet, RunError, RunStatus, error_range};
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

/// Owns the current run and the latest result per editor tab.
#[derive(Default)]
pub struct Runner {
    next_run: u64,
    running: Option<Running>,
    latest: Option<Entity<ResultSet>>,
    per_tab: HashMap<Uuid, Entity<ResultSet>>,
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

    pub fn result_for_tab(&self, tab: Uuid) -> Option<&Entity<ResultSet>> {
        self.per_tab.get(&tab)
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
        let sent = gremlin_syntax::ensure_terminal(&request.text);
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
        self.per_tab.insert(request.tab_id, result.clone());
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.running = Some(Running {
            run_id,
            cancel: Some(cancel_tx),
        });
        cx.emit(RunnerEvent::Started);
        cx.emit(RunnerEvent::RunningChanged);
        cx.notify();

        let timeout = Duration::from_millis(config.timeout_ms);
        let base = request.range.start;
        let tab_id = request.tab_id;
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
            let io = cx.update(|_, cx| spawn_io(cx, stream(client, sent, timeout, cancel_rx, tx)));
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
            let error_range = result.read_with(cx, |r, _| match &r.status {
                RunStatus::Failed(e) => e.range.clone(),
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
    timeout: Duration,
    mut cancel: oneshot::Receiver<()>,
    tx: mpsc::UnboundedSender<Msg>,
) {
    let request = Request::new(script).timeout(timeout);
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
