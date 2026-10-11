//! Scripted render benchmark (`gremliner --render-bench N,E`): drives the camera along a fixed path
//! (pan sweep, deep zoom in/out, orbit) and reports frame statistics (ADR-0006 gate, ADR-0027 budgets).

use std::time::{Duration, Instant};

use graph_store::GraphStore;
use graph_store::view::{Camera, SceneParams, Vec2};

use crate::stats::Summary;

pub struct BenchRun {
    pub duration: Duration,
    pub quit_when_done: bool,
    /// Frames to skip before timing starts (shader compilation, atlas warm-up).
    pub warmup_frames: usize,
    start: Option<Instant>,
    frames_seen: usize,
}

impl BenchRun {
    pub fn new(duration: Duration, quit_when_done: bool) -> Self {
        Self {
            duration,
            quit_when_done,
            warmup_frames: 30,
            start: None,
            frames_seen: 0,
        }
    }

    /// Camera for the frame at `now`, or `None` when the run is over.
    pub fn camera_at(
        &mut self,
        now: Instant,
        store: &GraphStore,
        viewport: Vec2,
        params: &SceneParams,
    ) -> Option<Camera> {
        let b = store.bounds()?;
        let fit = Camera::fit(b, viewport, 32.0, 1.0);
        self.frames_seen += 1;
        if self.frames_seen <= self.warmup_frames {
            return Some(fit);
        }
        let start = *self.start.get_or_insert(now);
        let t = now.duration_since(start).as_secs_f32() / self.duration.as_secs_f32();
        if t >= 1.0 {
            return None;
        }
        Some(camera_path(t, fit, b, params))
    }

    pub fn report(&self, nodes: usize, edges: usize, s: &Summary) {
        println!(
            "render-bench nodes={nodes} edges={edges} frames={} fps={:.1}",
            s.frames, s.fps
        );
        println!(
            "  frame interval ms: p50={:.2} p95={:.2} p99={:.2} max={:.2}",
            s.interval.p50, s.interval.p95, s.interval.p99, s.interval.max
        );
        println!(
            "  canvas paint ms:   p50={:.2} p95={:.2} p99={:.2} max={:.2}",
            s.paint.p50, s.paint.p95, s.paint.p99, s.paint.max
        );
    }
}

/// The scripted path over normalised time `t` in [0, 1):
/// 0.00–0.30 pan sweep at fit zoom; 0.30–0.65 zoom in 8× and back out around the centre;
/// 0.65–1.00 orbit at 3× zoom.
pub fn camera_path(t: f32, fit: Camera, bounds: [f32; 4], _params: &SceneParams) -> Camera {
    let w = bounds[2] - bounds[0];
    let h = bounds[3] - bounds[1];
    let mut cam = fit;
    if t < 0.30 {
        let u = t / 0.30;
        cam.center[0] = fit.center[0] + (u * std::f32::consts::TAU).sin() * w * 0.25;
    } else if t < 0.65 {
        let u = (t - 0.30) / 0.35;
        let zoom_exp = (u * std::f32::consts::PI).sin() * 3.0; // up to 2^3 = 8×
        cam.zoom = fit.zoom * 2f32.powf(zoom_exp);
    } else {
        let u = (t - 0.65) / 0.35;
        let a = u * std::f32::consts::TAU;
        cam.zoom = fit.zoom * 3.0;
        cam.center = [
            fit.center[0] + a.cos() * w * 0.2,
            fit.center[1] + a.sin() * h * 0.2,
        ];
    }
    cam
}
