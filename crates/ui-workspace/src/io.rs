//! The bridge from GPUI to the tokio runtime (ADR-0005).
//!
//! `main` creates one multi-threaded tokio runtime and installs its handle with [`install`]. UI code calls
//! [`spawn_io`] to run network, disk or LLM work there; the returned GPUI [`Task`] resolves on the UI
//! side, so results can update entities directly. [`spawn_io_tracked`] also registers the work with the
//! [`Activity`] tracker, which drives the status bar's activity indicator.

use std::future::Future;
use std::time::Instant;

use futures::channel::oneshot;
use gpui_kit::*;

/// The installed tokio runtime handle.
struct IoRuntime(tokio::runtime::Handle);

impl Global for IoRuntime {}

/// Install the tokio runtime used by [`spawn_io`]. The runtime itself must outlive the app.
pub fn install(handle: tokio::runtime::Handle, cx: &mut App) {
    cx.set_global(IoRuntime(handle));
    if !cx.has_global::<ActivityGlobal>() {
        let tracker = cx.new(|_| Activity::default());
        cx.set_global(ActivityGlobal(tracker));
    }
}

/// The work was dropped before it produced a value: the tokio task panicked or the runtime shut down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoCancelled;

impl std::fmt::Display for IoCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("background task was cancelled")
    }
}

impl std::error::Error for IoCancelled {}

/// Run `future` on the tokio runtime. Never blocks the UI thread.
///
/// Dropping the returned task does not cancel the tokio work; use a cancellation token inside `future`
/// for that (the Gremlin client's `ResultStream` does).
///
/// # Panics
/// If [`install`] was not called.
pub fn spawn_io<T, F>(cx: &App, future: F) -> Task<Result<T, IoCancelled>>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let handle = &cx
        .try_global::<IoRuntime>()
        .expect("ui_workspace::io::install was not called")
        .0;
    let (tx, rx) = oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(future.await);
    });
    cx.background_spawn(async move { rx.await.map_err(|_| IoCancelled) })
}

/// Like [`spawn_io`], and shows `label` in the status bar until the work finishes.
pub fn spawn_io_tracked<T, F>(
    label: impl Into<SharedString>,
    cx: &mut App,
    future: F,
) -> Task<Result<T, IoCancelled>>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let tracker = Activity::global(cx);
    let id = tracker.update(cx, |activity, cx| activity.begin(label.into(), cx));
    let work = spawn_io(cx, future);
    cx.spawn(async move |cx| {
        let result = work.await;
        tracker.update(cx, |activity, cx| activity.end(id, cx));
        result
    })
}

/// Identifies one tracked activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivityId(u64);

#[derive(Debug, Clone)]
pub struct RunningActivity {
    pub id: ActivityId,
    pub label: SharedString,
    pub started: Instant,
}

/// Background work currently in flight. Observe it to show progress.
#[derive(Default)]
pub struct Activity {
    next: u64,
    running: Vec<RunningActivity>,
}

struct ActivityGlobal(Entity<Activity>);

impl Global for ActivityGlobal {}

impl Activity {
    /// The app-wide tracker.
    ///
    /// # Panics
    /// If [`install`] was not called.
    pub fn global(cx: &App) -> Entity<Activity> {
        cx.global::<ActivityGlobal>().0.clone()
    }

    pub fn begin(&mut self, label: SharedString, cx: &mut Context<'_, Self>) -> ActivityId {
        self.next += 1;
        let id = ActivityId(self.next);
        self.running.push(RunningActivity {
            id,
            label,
            started: Instant::now(),
        });
        cx.notify();
        id
    }

    pub fn end(&mut self, id: ActivityId, cx: &mut Context<'_, Self>) {
        self.running.retain(|a| a.id != id);
        cx.notify();
    }

    pub fn is_busy(&self) -> bool {
        !self.running.is_empty()
    }

    pub fn running(&self) -> &[RunningActivity] {
        &self.running
    }

    /// The label to show: the most recent activity, with a count when several are in flight.
    pub fn summary(&self) -> Option<SharedString> {
        let latest = self.running.last()?;
        Some(match self.running.len() {
            1 => latest.label.clone(),
            n => format!("{} (+{} more)", latest.label, n - 1).into(),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::time::{Duration, Instant};

    use gpui_kit::TestAppContext;

    use super::{Activity, install, spawn_io, spawn_io_tracked};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[gpui_kit::test]
    async fn io_work_does_not_block_foreground(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let rt = runtime();
        cx.update(|cx| install(rt.handle().clone(), cx));
        let started = Instant::now();
        let io = cx.update(|cx| {
            spawn_io(cx, async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                42
            })
        });
        // A foreground task scheduled after the I/O completes first: the UI thread stays free.
        let foreground = cx.update(|cx| cx.spawn(async |_| Instant::now()));
        cx.run_until_parked();
        let foreground_done = foreground.await;
        assert!(foreground_done.duration_since(started) < Duration::from_millis(150));
        assert_eq!(io.await, Ok(42));
        assert!(started.elapsed() >= Duration::from_millis(200));
    }

    #[gpui_kit::test]
    async fn tracked_work_shows_activity_until_done(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let rt = runtime();
        cx.update(|cx| install(rt.handle().clone(), cx));
        let (release, wait) = tokio::sync::oneshot::channel::<()>();
        let task = cx.update(|cx| {
            spawn_io_tracked("Loading schema", cx, async move {
                let _ = wait.await;
                "done"
            })
        });
        cx.run_until_parked();
        let activity = cx.update(|cx| Activity::global(cx));
        cx.read(|cx| {
            let activity = activity.read(cx);
            assert!(activity.is_busy());
            assert_eq!(activity.summary().as_deref(), Some("Loading schema"));
        });
        release.send(()).unwrap();
        assert_eq!(task.await, Ok("done"));
        cx.read(|cx| assert!(!activity.read(cx).is_busy()));
    }
}
