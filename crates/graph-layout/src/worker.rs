//! The layout worker (ADR-0009): a dedicated thread that runs one layout job at a time and publishes
//! position frames into a latest-wins slot the UI polls once per animation frame.
//!
//! Iterative layouts (ForceAtlas2) publish at most ~60 frames per second, with a few iterations per frame
//! for small graphs (so they visibly settle) and as many as fit in ~12 ms for large ones. One-shot layouts
//! compute their targets and publish a 320 ms ease-out tween from the current positions.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{ForceAtlas2, Layout, LayoutGraph, LayoutKind, LayoutOptions, StepResult, Vec2};

/// Frame pacing for published frames.
const FRAME: Duration = Duration::from_millis(16);
/// Compute budget per published frame for iterative layouts.
const SLICE: Duration = Duration::from_millis(12);
/// Iterations per frame for small graphs: enough to settle in about a second, few enough that the motion
/// is visible rather than instant.
const MIN_STEPS_PER_FRAME: usize = 12;
/// Tween duration for one-shot layouts (ADR-0023 motion token).
pub const TWEEN: Duration = Duration::from_millis(320);

/// One layout run.
#[derive(Clone, Debug)]
pub struct LayoutJob {
    /// Echoed in frames so the UI can drop frames of an outdated job.
    pub generation: u64,
    pub graph: Arc<LayoutGraph>,
    pub positions: Vec<Vec2>,
    pub pinned: Vec<bool>,
    pub kind: LayoutKind,
    pub options: LayoutOptions,
    /// Centre of the radial layout (the selection).
    pub focus: Vec<u32>,
    /// Incremental placement: only these nodes move (existing ones stay where they are).
    pub movable: Option<Vec<bool>>,
}

/// Positions published by the worker.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub generation: u64,
    pub positions: Vec<Vec2>,
    /// The last frame of the job.
    pub done: bool,
    pub iteration: usize,
}

enum Command {
    Run(Box<LayoutJob>),
    Stop,
    /// Pin a node at a position (`Some`) or release it (`None`).
    Pin(u32, Option<Vec2>),
    Shutdown,
}

type Slot = Arc<Mutex<Option<Frame>>>;

pub struct LayoutWorker {
    tx: Sender<Command>,
    slot: Slot,
    handle: Option<JoinHandle<()>>,
}

impl Default for LayoutWorker {
    fn default() -> Self {
        Self::spawn()
    }
}

impl LayoutWorker {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let slot: Slot = Arc::new(Mutex::new(None));
        let publish = slot.clone();
        let handle = std::thread::Builder::new()
            .name("graph-layout".into())
            .spawn(move || worker_loop(&rx, &publish))
            .ok();
        Self { tx, slot, handle }
    }

    pub fn run(&self, job: LayoutJob) {
        let _ = self.tx.send(Command::Run(Box::new(job)));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }

    pub fn pin(&self, node: u32, at: Option<Vec2>) {
        let _ = self.tx.send(Command::Pin(node, at));
    }

    /// The newest frame since the last call, if any.
    pub fn take_frame(&self) -> Option<Frame> {
        self.slot.lock().ok().and_then(|mut s| s.take())
    }
}

impl Drop for LayoutWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

enum Mode {
    Iterative(Box<ForceAtlas2>),
    Tween {
        from: Vec<Vec2>,
        to: Vec<Vec2>,
        started: Instant,
    },
}

struct Running {
    job: LayoutJob,
    /// Effective pins: user pins plus, in incremental runs, every node that must stay.
    fixed: Vec<bool>,
    mode: Mode,
    iteration: usize,
}

impl Running {
    fn new(job: LayoutJob) -> Self {
        let n = job.positions.len();
        let fixed: Vec<bool> = (0..n)
            .map(|i| {
                job.pinned.get(i).copied().unwrap_or(false)
                    || job
                        .movable
                        .as_ref()
                        .is_some_and(|m| !m.get(i).copied().unwrap_or(true))
            })
            .collect();
        let mode = if job.kind.is_iterative() || job.movable.is_some() {
            // Incremental placement always uses the force layout for the new nodes.
            Mode::Iterative(Box::new(ForceAtlas2::new(&job.graph, job.options.fa2)))
        } else {
            let to = crate::one_shot(
                job.kind,
                &job.graph,
                &job.positions,
                &fixed,
                &job.focus,
                &job.options,
            );
            Mode::Tween {
                from: job.positions.clone(),
                to,
                started: Instant::now(),
            }
        };
        Self {
            job,
            fixed,
            mode,
            iteration: 0,
        }
    }

    /// Advances by one frame's worth of work; returns the frame and whether the job is done.
    fn advance(&mut self) -> (Vec<Vec2>, bool) {
        match &mut self.mode {
            Mode::Iterative(fa2) => {
                let started = Instant::now();
                let mut steps = 0;
                let mut done = false;
                while steps < MIN_STEPS_PER_FRAME || started.elapsed() < SLICE {
                    steps += 1;
                    self.iteration += 1;
                    if fa2.step(&self.job.graph, &mut self.job.positions, &self.fixed)
                        == StepResult::Converged
                    {
                        done = true;
                        break;
                    }
                    if steps >= MIN_STEPS_PER_FRAME && self.job.graph.node_count() < 2_000 {
                        break;
                    }
                }
                (self.job.positions.clone(), done)
            }
            Mode::Tween { from, to, started } => {
                let t = (started.elapsed().as_secs_f32() / TWEEN.as_secs_f32()).min(1.0);
                if t >= 1.0 {
                    self.iteration += 1;
                    self.job.positions = to.clone();
                    return (to.clone(), true);
                }
                let k = 1.0 - (1.0 - t).powi(3);
                let frame: Vec<Vec2> = from
                    .iter()
                    .zip(to.iter())
                    .map(|(a, b)| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k])
                    .collect();
                self.iteration += 1;
                (frame, false)
            }
        }
    }

    fn pin(&mut self, node: u32, at: Option<Vec2>) {
        let i = node as usize;
        if i >= self.fixed.len() {
            return;
        }
        match at {
            Some(p) => {
                self.fixed[i] = true;
                self.job.positions[i] = p;
                if let Mode::Tween { to, .. } = &mut self.mode {
                    to[i] = p;
                }
            }
            None => {
                let incremental_fixed = self
                    .job
                    .movable
                    .as_ref()
                    .is_some_and(|m| !m.get(i).copied().unwrap_or(true));
                self.fixed[i] = incremental_fixed;
            }
        }
    }
}

fn worker_loop(rx: &Receiver<Command>, slot: &Slot) {
    let mut current: Option<Running> = None;
    let mut last_publish = Instant::now();
    loop {
        // Wait for a command: indefinitely when idle, until the next frame is due when running.
        let next = if current.is_some() {
            let wait = FRAME.saturating_sub(last_publish.elapsed());
            match rx.recv_timeout(wait) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        };
        match next {
            Some(Command::Run(job)) => current = Some(Running::new(*job)),
            Some(Command::Stop) => current = None,
            Some(Command::Pin(node, at)) => {
                if let Some(r) = current.as_mut() {
                    r.pin(node, at);
                }
            }
            Some(Command::Shutdown) => return,
            None => {}
        }
        // Drain whatever else is queued before computing (a newer Run supersedes).
        while let Ok(c) = rx.try_recv() {
            match c {
                Command::Run(job) => current = Some(Running::new(*job)),
                Command::Stop => current = None,
                Command::Pin(node, at) => {
                    if let Some(r) = current.as_mut() {
                        r.pin(node, at);
                    }
                }
                Command::Shutdown => return,
            }
        }
        if last_publish.elapsed() < FRAME {
            continue;
        }
        if let Some(r) = current.as_mut() {
            let (positions, done) = r.advance();
            let frame = Frame {
                generation: r.job.generation,
                positions,
                done,
                iteration: r.iteration,
            };
            if let Ok(mut s) = slot.lock() {
                *s = Some(frame);
            }
            last_publish = Instant::now();
            if done {
                current = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_positions;

    fn star(n: u32) -> Arc<LayoutGraph> {
        Arc::new(LayoutGraph::new(n as usize, (1..n).map(|i| [0, i])))
    }

    fn job(g: Arc<LayoutGraph>, kind: LayoutKind) -> LayoutJob {
        let n = g.node_count();
        LayoutJob {
            generation: 3,
            positions: seeded_positions(n, 1),
            pinned: vec![false; n],
            graph: g,
            kind,
            options: LayoutOptions::default(),
            focus: vec![],
            movable: None,
        }
    }

    fn wait_done(w: &LayoutWorker) -> (Frame, usize) {
        let started = Instant::now();
        let mut frames = 0;
        loop {
            if let Some(f) = w.take_frame() {
                frames += 1;
                if f.done {
                    return (f, frames);
                }
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "layout never finished"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn streams_frames_until_done() {
        let w = LayoutWorker::spawn();
        w.run(job(star(30), LayoutKind::ForceAtlas2));
        let (last, frames) = wait_done(&w);
        assert_eq!(last.generation, 3);
        assert_eq!(last.positions.len(), 30);
        assert!(frames > 1, "the layout animates over several frames");
    }

    #[test]
    fn one_shot_layouts_tween_to_their_target() {
        let w = LayoutWorker::spawn();
        let g = star(12);
        let j = job(g.clone(), LayoutKind::Circular);
        let target = crate::one_shot(
            LayoutKind::Circular,
            &g,
            &j.positions,
            &j.pinned,
            &[],
            &j.options,
        );
        let started = Instant::now();
        w.run(j);
        let (last, frames) = wait_done(&w);
        assert!(started.elapsed() >= TWEEN, "tweened over {TWEEN:?}");
        assert!(frames >= 5);
        assert_eq!(last.positions, target);
    }

    #[test]
    fn scenario_graph_layouts_stop_layout() {
        let w = LayoutWorker::spawn();
        let g = Arc::new(LayoutGraph::new(3_000, (1..3_000u32).map(|i| [i / 4, i])));
        w.run(job(g, LayoutKind::ForceAtlas2));
        std::thread::sleep(Duration::from_millis(60));
        w.stop();
        // Nothing is published after stop returns (a frame already in flight may land once at most).
        std::thread::sleep(Duration::from_millis(50));
        let _ = w.take_frame();
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(w.take_frame(), None, "no frames after stopping");
    }

    #[test]
    fn incremental_runs_move_only_new_nodes() {
        let w = LayoutWorker::spawn();
        let g = star(20);
        let mut j = job(g, LayoutKind::ForceAtlas2);
        let before = j.positions.clone();
        let mut movable = vec![false; 20];
        movable[18] = true;
        movable[19] = true;
        j.movable = Some(movable);
        w.run(j);
        let (last, _) = wait_done(&w);
        assert_eq!(&last.positions[..18], &before[..18], "existing nodes stay");
        assert_ne!(last.positions[19], before[19]);
    }
}
