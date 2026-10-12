//! GPUI graph canvas (ADR-0006): an interactive view over a [`GraphStore`] with pan/zoom, hover,
//! selection, rectangle selection, node dragging with pinning, a minimap, a perf HUD, and a scripted
//! render benchmark.

pub mod bench;
pub mod raster_cache;
pub mod renderer;
pub mod stats;
pub mod theme;

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{
    App, Bounds, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Render, ScrollDelta, ScrollWheelEvent, SharedString, Styled, Window, canvas, div, fill,
    outline, point, px, size,
};
use graph_layout::worker::{LayoutJob, LayoutWorker};
use graph_layout::{LayoutGraph, LayoutKind, LayoutOptions};
use graph_store::view::minimap::MinimapTransform;
use graph_store::view::{
    Camera, Lod, SceneParams, SceneStats, SpatialGrid, Subset, Vec2, rect_from_points,
};
use graph_store::{GraphStore, NodeIx, flags};

use crate::bench::BenchRun;
use crate::raster_cache::{RasterCache, raster_colors, spawn_raster};
use crate::renderer::{FrameInput, GpuiPrimitives, GraphRenderer, paint_labels};
use crate::stats::FrameStats;
use crate::theme::CanvasStyle;

const MINIMAP_SIZE: Vec2 = [220.0, 150.0];
/// Views with more elements than this use the raster level of detail when zoomed out (ADR-0032).
const RASTER_MIN_ELEMENTS: usize = 6_000;
/// Minimum time between raster refreshes while the camera keeps moving (the cached image is scaled in
/// between, so motion stays smooth and detail catches up within this interval).
const RASTER_REFRESH_MS: u128 = 120;
const MINIMAP_MARGIN: f32 = 12.0;

/// What the canvas tells its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasEvent {
    /// A node was double-clicked (expand its neighbours).
    NodeDoubleClicked(NodeIx),
}

/// A camera tween (find-in-graph focus, fit).
#[derive(Clone, Copy)]
struct CameraAnim {
    from: Camera,
    to: Camera,
    started: Instant,
    duration_ms: f32,
    /// Keep the target centred on this node (it may move while a layout runs).
    follow: Option<NodeIx>,
}

/// Duration of camera tweens (ADR-0023 motion token).
const CAMERA_TWEEN_MS: f32 = 320.0;

enum Drag {
    None,
    Pan { last: Vec2, last_time: Instant },
    Node { node: NodeIx, grab_offset: Vec2 },
    Rect { start: Vec2, current: Vec2 },
    Minimap,
}

pub struct GraphCanvas {
    store: Arc<GraphStore>,
    camera: Camera,
    params: SceneParams,
    style: CanvasStyle,
    renderer: Box<dyn GraphRenderer>,
    grid: Option<SpatialGrid>,
    bounds: Bounds<Pixels>,
    hovered: Option<NodeIx>,
    drag: Drag,
    velocity: Vec2,
    show_minimap: bool,
    perf_hud: bool,
    stats: FrameStats,
    last_scene: SceneStats,
    needs_fit: bool,
    bench: Option<BenchRun>,
    focus: FocusHandle,
    raster: RasterCache,
    raster_enabled: bool,
    style_version: u64,
    minimap_image: Option<(u64, Arc<gpui_kit::RenderImage>)>,
    last_raster_request: Option<Instant>,
    retired_images: Vec<Arc<gpui_kit::RenderImage>>,
    context_node: Option<NodeIx>,
    camera_anim: Option<CameraAnim>,
    /// The user panned or zoomed since the last fit (hosts stop auto-fitting then).
    camera_touched: bool,
    layout: Option<LayoutWorker>,
    /// Stylesheet looks per label.
    looks: graph_store::style::Looks,
    /// Generation of the running layout job (frames of other generations are ignored).
    layout_generation: u64,
    layout_running: bool,
    /// Fit the camera (animated) when the running layout finishes.
    fit_after_layout: bool,
}

/// What to lay out: everything, or only some nodes (incremental placement keeps the rest still).
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutScope {
    All,
    Only(Vec<NodeIx>),
}

impl EventEmitter<CanvasEvent> for GraphCanvas {}

impl GraphCanvas {
    pub fn new(store: GraphStore, cx: &mut Context<'_, Self>) -> Self {
        Self {
            store: Arc::new(store),
            camera: Camera::default(),
            params: SceneParams::default(),
            style: CanvasStyle::dark(),
            renderer: Box::new(GpuiPrimitives::default()),
            grid: None,
            bounds: Bounds::default(),
            hovered: None,
            drag: Drag::None,
            velocity: [0.0, 0.0],
            show_minimap: true,
            perf_hud: false,
            stats: FrameStats::default(),
            last_scene: SceneStats::default(),
            needs_fit: true,
            bench: None,
            focus: cx.focus_handle(),
            raster: RasterCache::default(),
            raster_enabled: std::env::var_os("GREMLINER_NO_RASTER").is_none(),
            style_version: 0,
            minimap_image: None,
            last_raster_request: None,
            retired_images: Vec::new(),
            context_node: None,
            camera_anim: None,
            camera_touched: false,
            layout: None,
            looks: graph_store::style::Looks::default(),
            layout_generation: 0,
            layout_running: false,
            fit_after_layout: false,
        }
    }

    /// Start `kind` on the current graph (replacing a running layout). Pinned nodes stay; with
    /// [`LayoutScope::Only`], every other node stays too. `fit` animates the camera to the result.
    pub fn run_layout(
        &mut self,
        kind: LayoutKind,
        options: LayoutOptions,
        scope: LayoutScope,
        fit: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let g = &self.store;
        let n = g.node_count();
        if n == 0 {
            return;
        }
        let (srcs, dsts) = g.edge_endpoints();
        let graph = LayoutGraph::new(n, srcs.iter().zip(dsts).map(|(a, b)| [a.0, b.0]));
        let pinned: Vec<bool> = g
            .node_flags()
            .iter()
            .map(|f| f & flags::PINNED != 0)
            .collect();
        let focus: Vec<u32> = g.selected().map(|n| n.0).collect();
        let movable = match scope {
            LayoutScope::All => None,
            LayoutScope::Only(nodes) => {
                let mut m = vec![false; n];
                for x in nodes {
                    if let Some(slot) = m.get_mut(x.0 as usize) {
                        *slot = true;
                    }
                }
                Some(m)
            }
        };
        self.layout_generation += 1;
        let job = LayoutJob {
            generation: self.layout_generation,
            graph: Arc::new(graph),
            positions: g.positions().to_vec(),
            pinned,
            kind,
            options,
            focus,
            movable,
        };
        self.layout.get_or_insert_with(LayoutWorker::spawn).run(job);
        self.layout_running = true;
        self.fit_after_layout = fit;
        cx.notify();
    }

    /// Stop the running layout; nodes stay where they are from the next frame on.
    pub fn stop_layout(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(w) = &self.layout {
            w.stop();
        }
        self.layout_generation += 1;
        self.layout_running = false;
        self.fit_after_layout = false;
        cx.notify();
    }

    pub fn is_layout_running(&self) -> bool {
        self.layout_running
    }

    /// Frame timings (tests and the perf HUD).
    pub fn frame_stats(&self) -> &FrameStats {
        &self.stats
    }

    /// Applies the newest layout frame, if any. Returns true while the layout keeps running.
    fn apply_layout_frame(&mut self) -> bool {
        if !self.layout_running {
            return false;
        }
        let Some(frame) = self.layout.as_ref().and_then(LayoutWorker::take_frame) else {
            return true;
        };
        if frame.generation != self.layout_generation {
            return true;
        }
        let dragged = match self.drag {
            Drag::Node { node, .. } => Some(node),
            _ => None,
        };
        let g = self.graph_mut();
        if frame.positions.len() == g.node_count() {
            let mut positions = frame.positions;
            // Pinned and dragged nodes keep their current positions.
            for (i, f) in g.node_flags().iter().enumerate() {
                if f & flags::PINNED != 0 || dragged == Some(NodeIx(i as u32)) {
                    positions[i] = g.positions()[i];
                }
            }
            g.set_positions(&positions);
        }
        if frame.done {
            self.layout_running = false;
            if std::mem::take(&mut self.fit_after_layout) && !self.camera_touched {
                self.animate_fit();
            }
        }
        self.layout_running
    }

    /// Tween the camera to fit the graph.
    fn animate_fit(&mut self) {
        if let Some(b) = self.store.bounds() {
            let r = self.params.node_radius;
            let to = Camera::fit(
                [b[0] - r, b[1] - r, b[2] + r, b[3] + r],
                self.viewport(),
                32.0,
                1.0,
            );
            self.camera_anim = Some(CameraAnim {
                from: self.camera,
                to,
                started: Instant::now(),
                duration_ms: CAMERA_TWEEN_MS,
                follow: None,
            });
        }
    }

    /// Replaces the colours (e.g. on a theme change).
    pub fn set_style(&mut self, style: CanvasStyle) {
        self.style = style;
        self.style_version += 1;
        self.minimap_image = None;
    }

    pub fn style(&self) -> &CanvasStyle {
        &self.style
    }

    /// Applies resolved stylesheet looks (colours, shapes, icons, label density).
    pub fn set_looks(&mut self, looks: graph_store::style::Looks) {
        self.params.min_label_px = looks.globals.min_label_px.max(4.0);
        self.params.max_labels = looks.globals.max_labels();
        self.params.edge_labels = looks.globals.show_edge_captions;
        self.looks = looks;
        self.style_version += 1;
        self.minimap_image = None;
    }

    pub fn looks(&self) -> &graph_store::style::Looks {
        &self.looks
    }

    /// Fit the graph into the viewport on the next frame.
    pub fn request_fit(&mut self) {
        self.needs_fit = true;
        self.camera_anim = None;
        self.camera_touched = false;
    }

    /// True once the user panned or zoomed after the last fit.
    pub fn camera_touched(&self) -> bool {
        self.camera_touched
    }

    /// The node under the pointer when the context menu was opened.
    pub fn context_node(&self) -> Option<NodeIx> {
        self.context_node
    }

    pub fn hovered(&self) -> Option<NodeIx> {
        self.hovered
    }

    pub fn show_minimap(&self) -> bool {
        self.show_minimap
    }

    /// Zoom about the viewport centre.
    pub fn zoom_by(&mut self, factor: f32) {
        let vp = self.viewport();
        self.camera_anim = None;
        self.camera_touched = true;
        self.camera
            .zoom_about([vp[0] * 0.5, vp[1] * 0.5], factor, vp);
    }

    /// Animate the camera to centre `n`, zooming in far enough for labels to show.
    pub fn focus_node(&mut self, n: NodeIx) {
        let min_zoom = 14.0 / self.params.node_radius;
        let to = Camera {
            center: self.store.position(n),
            zoom: self.camera.zoom.max(min_zoom),
        };
        self.camera_anim = Some(CameraAnim {
            from: self.camera,
            to,
            started: Instant::now(),
            duration_ms: CAMERA_TWEEN_MS,
            follow: Some(n),
        });
    }

    /// Advances the camera tween; returns true while it runs.
    fn step_camera_anim(&mut self) -> bool {
        let Some(mut anim) = self.camera_anim else {
            return false;
        };
        if let Some(n) = anim.follow
            && (n.0 as usize) < self.store.node_count()
        {
            anim.to.center = self.store.position(n);
            self.camera_anim = Some(anim);
        }
        let t = (anim.started.elapsed().as_secs_f32() * 1000.0 / anim.duration_ms).min(1.0);
        // Ease out cubic.
        let k = 1.0 - (1.0 - t).powi(3);
        let lerp = |a: f32, b: f32| a + (b - a) * k;
        // Interpolate zoom geometrically so it feels uniform.
        let zoom = (anim.from.zoom.ln() + (anim.to.zoom.ln() - anim.from.zoom.ln()) * k).exp();
        self.camera = Camera {
            center: [
                lerp(anim.from.center[0], anim.to.center[0]),
                lerp(anim.from.center[1], anim.to.center[1]),
            ],
            zoom,
        };
        if t >= 1.0 {
            self.camera = anim.to;
            self.camera_anim = None;
        }
        true
    }

    /// Mutable access to the store (copy-on-write if a background raster job holds a snapshot).
    fn graph_mut(&mut self) -> &mut GraphStore {
        Arc::make_mut(&mut self.store)
    }

    pub fn store(&self) -> &GraphStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut GraphStore {
        self.graph_mut()
    }

    pub fn camera(&self) -> Camera {
        self.camera
    }

    /// Window-space position of a node's centre (used by tests and automation).
    pub fn node_window_position(&self, n: NodeIx) -> gpui_kit::Point<Pixels> {
        let s = self
            .camera
            .world_to_screen(self.store.position(n), self.viewport());
        point(
            self.bounds.origin.x + px(s[0]),
            self.bounds.origin.y + px(s[1]),
        )
    }

    pub fn set_perf_hud(&mut self, on: bool) {
        self.perf_hud = on;
    }

    pub fn set_show_minimap(&mut self, on: bool) {
        self.show_minimap = on;
    }

    pub fn start_bench(&mut self, run: BenchRun) {
        self.bench = Some(run);
        self.stats.clear();
    }

    fn viewport(&self) -> Vec2 {
        [
            f32::from(self.bounds.size.width),
            f32::from(self.bounds.size.height),
        ]
    }

    fn local(&self, p: gpui_kit::Point<Pixels>) -> Vec2 {
        [
            f32::from(p.x - self.bounds.origin.x),
            f32::from(p.y - self.bounds.origin.y),
        ]
    }

    pub fn fit_to_view(&mut self) {
        if let Some(b) = self.store.bounds() {
            let r = self.params.node_radius;
            self.camera = Camera::fit(
                [b[0] - r, b[1] - r, b[2] + r, b[3] + r],
                self.viewport(),
                32.0,
                1.0,
            );
        }
    }

    fn ensure_grid(&mut self) {
        if self
            .grid
            .as_ref()
            .is_none_or(|g| g.revision() != self.store.revision())
        {
            self.grid = Some(SpatialGrid::build(&self.store, 4.0));
        }
    }

    fn node_at_screen(&mut self, s: Vec2) -> Option<NodeIx> {
        let world = self.camera.screen_to_world(s, self.viewport());
        let radius = self.params.node_radius.max(5.0 / self.camera.zoom);
        self.ensure_grid();
        self.grid
            .as_ref()
            .and_then(|g| g.node_at(&self.store, world, radius))
    }

    /// Re-rasterises the minimap when the graph changed (not while dragging, to keep drags smooth).
    fn refresh_minimap(&mut self, scale: f32) {
        if !self.show_minimap || matches!(self.drag, Drag::Node { .. }) {
            return;
        }
        let rev = self.store.revision();
        if self.minimap_image.as_ref().is_some_and(|(r, _)| *r == rev) {
            return;
        }
        let Some(b) = self.store.bounds() else { return };
        let cam = Camera::fit(b, MINIMAP_SIZE, 8.0, 1.0);
        let region = graph_store::view::raster::RasterRegion {
            offset: [0.0, 0.0],
            size: MINIMAP_SIZE,
            scale,
        };
        let mut params = self.params;
        params.min_node_px = 0.75;
        let mut colors = raster_colors(
            &self.style,
            &self.looks,
            self.store.vertex_label_count(),
            true,
            true,
        );
        colors.palette = vec![
            colors
                .palette
                .first()
                .copied()
                .unwrap_or([255, 255, 255, 255]);
            1
        ];
        let img = graph_store::view::raster::rasterize(
            &self.store,
            &cam,
            MINIMAP_SIZE,
            region,
            &params,
            &colors,
        );
        let (w, h) = (img.width as u32, img.height as u32);
        if let Some(buffer) = image::RgbaImage::from_raw(w.max(1), h.max(1), img.bgra) {
            let image = Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(buffer)]));
            if let Some((_, old)) = self.minimap_image.replace((rev, image)) {
                self.retired_images.push(old);
            }
        }
    }

    fn minimap_rect(&self) -> [f32; 4] {
        let vp = self.viewport();
        let x1 = vp[0] - MINIMAP_MARGIN;
        let y1 = vp[1] - MINIMAP_MARGIN;
        [x1 - MINIMAP_SIZE[0], y1 - MINIMAP_SIZE[1], x1, y1]
    }

    fn minimap_transform(&self) -> Option<MinimapTransform> {
        let b = self.store.bounds()?;
        Some(MinimapTransform::new(b, self.minimap_rect(), 8.0))
    }

    fn in_minimap(&self, s: Vec2) -> bool {
        self.show_minimap && graph_store::view::rect_contains(self.minimap_rect(), s)
    }

    fn center_from_minimap(&mut self, s: Vec2) {
        if let Some(t) = self.minimap_transform() {
            self.camera.center = t.map_to_world(s);
        }
    }

    // ---- input -------------------------------------------------------------------------------

    fn on_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.focus, cx);
        let s = self.local(ev.position);
        self.velocity = [0.0, 0.0];
        self.camera_anim = None;
        if self.in_minimap(s) {
            self.center_from_minimap(s);
            self.drag = Drag::Minimap;
        } else if let Some(n) = self.node_at_screen(s) {
            if ev.click_count == 2 && !ev.modifiers.shift {
                cx.emit(CanvasEvent::NodeDoubleClicked(n));
            }
            if ev.modifiers.shift {
                let on = !self.store.has_flag(n, flags::SELECTED);
                self.graph_mut().set_flag(n, flags::SELECTED, on);
            } else if !self.store.has_flag(n, flags::SELECTED) {
                self.graph_mut().clear_flag(flags::SELECTED);
                self.graph_mut().set_flag(n, flags::SELECTED, true);
            }
            let world = self.camera.screen_to_world(s, self.viewport());
            let p = self.store.position(n);
            self.drag = Drag::Node {
                node: n,
                grab_offset: [p[0] - world[0], p[1] - world[1]],
            };
        } else if ev.modifiers.shift {
            self.drag = Drag::Rect {
                start: s,
                current: s,
            };
        } else {
            if ev.click_count == 1 {
                self.graph_mut().clear_flag(flags::SELECTED);
            }
            self.drag = Drag::Pan {
                last: s,
                last_time: Instant::now(),
            };
        }
        cx.notify();
    }

    /// Right button: remember the node under the pointer for the context menu and select it.
    fn on_right_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.focus, cx);
        let s = self.local(ev.position);
        self.context_node = if self.in_minimap(s) {
            None
        } else {
            self.node_at_screen(s)
        };
        if let Some(n) = self.context_node
            && !self.store.has_flag(n, flags::SELECTED)
        {
            self.graph_mut().clear_flag(flags::SELECTED);
            self.graph_mut().set_flag(n, flags::SELECTED, true);
        }
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        ev: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let s = self.local(ev.position);
        let viewport = self.viewport();
        match &mut self.drag {
            Drag::Pan { last, last_time } => {
                let d = [s[0] - last[0], s[1] - last[1]];
                let dt = last_time.elapsed().as_secs_f32().max(1e-3);
                self.velocity = [d[0] / dt, d[1] / dt];
                *last = s;
                *last_time = Instant::now();
                self.camera.pan_screen(d);
                self.camera_touched = true;
            }
            Drag::Node { node, grab_offset } => {
                let (node, off) = (*node, *grab_offset);
                let w = self.camera.screen_to_world(s, viewport);
                let p = [w[0] + off[0], w[1] + off[1]];
                self.graph_mut().set_position(node, p);
                // A running layout treats the dragged node as pinned where it is.
                if self.layout_running
                    && let Some(worker) = &self.layout
                {
                    worker.pin(node.0, Some(p));
                }
            }
            Drag::Rect { current, .. } => *current = s,
            Drag::Minimap => self.center_from_minimap(s),
            Drag::None => {
                let hovered = if self.in_minimap(s) {
                    None
                } else {
                    self.node_at_screen(s)
                };
                if hovered == self.hovered {
                    return;
                }
                self.hovered = hovered;
            }
        }
        cx.notify();
    }

    fn on_mouse_up(
        &mut self,
        _ev: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Node { node, .. } => self.graph_mut().set_flag(node, flags::PINNED, true),
            Drag::Rect { start, current } => {
                let vp = self.viewport();
                let r = rect_from_points(
                    self.camera.screen_to_world(start, vp),
                    self.camera.screen_to_world(current, vp),
                );
                self.ensure_grid();
                let picked = self
                    .grid
                    .as_ref()
                    .map(|g| g.nodes_in_rect(&self.store, r))
                    .unwrap_or_default();
                for n in picked {
                    self.graph_mut().set_flag(n, flags::SELECTED, true);
                }
            }
            Drag::Pan { last_time, .. } => {
                // Inertia only if the pointer was still moving at release.
                if last_time.elapsed().as_millis() > 60 {
                    self.velocity = [0.0, 0.0];
                }
            }
            Drag::Minimap | Drag::None => {}
        }
        cx.notify();
    }

    fn on_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let dy = match ev.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * 40.0,
        };
        let factor = (dy * 0.0025).exp();
        let s = self.local(ev.position);
        let vp = self.viewport();
        self.camera_anim = None;
        self.camera_touched = true;
        self.camera.zoom_about(s, factor, vp);
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        match ev.keystroke.key.as_str() {
            "f" => self.fit_to_view(),
            "m" => self.show_minimap = !self.show_minimap,
            "p" => self.perf_hud = !self.perf_hud,
            "escape" => self.graph_mut().clear_flag(flags::SELECTED),
            _ => return,
        }
        cx.notify();
    }

    /// Advances inertia; returns true while still moving.
    fn step_inertia(&mut self, dt: f32) -> bool {
        if !matches!(self.drag, Drag::None) {
            return false;
        }
        let speed = (self.velocity[0].powi(2) + self.velocity[1].powi(2)).sqrt();
        if speed < 20.0 {
            self.velocity = [0.0, 0.0];
            return false;
        }
        self.camera
            .pan_screen([self.velocity[0] * dt, self.velocity[1] * dt]);
        let decay = (-dt * 5.0).exp();
        self.velocity = [self.velocity[0] * decay, self.velocity[1] * decay];
        true
    }
}

impl Focusable for GraphCanvas {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GraphCanvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let entity = cx.entity();
        let background = self.style.background;
        div()
            .size_full()
            .bg(background)
            .track_focus(&self.focus)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_key_down(cx.listener(Self::on_key))
            .child(
                canvas(
                    {
                        let entity = entity.clone();
                        move |bounds, _window, cx| {
                            entity.update(cx, |this, _| {
                                this.bounds = bounds;
                                if this.needs_fit
                                    && f32::from(bounds.size.width) > 0.0
                                    && this.store.bounds().is_some()
                                {
                                    this.fit_to_view();
                                    this.needs_fit = false;
                                }
                            });
                        }
                    },
                    move |bounds, (), window, cx| paint_frame(&entity, bounds, window, cx),
                )
                .size_full(),
            )
    }
}

fn paint_frame(
    entity: &gpui_kit::Entity<GraphCanvas>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let started = Instant::now();
    // Advance animations (benchmark camera path, inertia) before drawing.
    let (animating, finished_bench) = entity.update(cx, |this, _| {
        let dt = this.stats.intervals_ms.last().copied().unwrap_or(16.0) / 1000.0;
        // Layout first, so a camera tween that follows a node aims at its new position.
        let mut animating =
            this.apply_layout_frame() | this.step_inertia(dt.min(0.05)) | this.step_camera_anim();
        let mut finished = None;
        let vp = this.viewport();
        if let Some(run) = &mut this.bench {
            match run.camera_at(started, &this.store, vp, &this.params) {
                Some(cam) => {
                    this.camera = cam;
                    animating = true;
                }
                None => finished = this.bench.take(),
            }
        }
        (
            animating || this.perf_hud || this.raster.is_pending(),
            finished,
        )
    });

    let scale = window.scale_factor();
    let viewport = [f32::from(bounds.size.width), f32::from(bounds.size.height)];

    // Decide between the raster level of detail and full vector drawing; request a fresh raster in the
    // background when the cached one has drifted (ADR-0032).
    // Zoomed out: nodes and edges are rasterised. Zoomed in (full detail): only edges are rasterised;
    // nodes, labels and highlighted edges (with arrowheads) are drawn as vectors on top.
    let (use_raster, raster_nodes, request) = entity.update(cx, |this, _| {
        let lod = Lod::for_radius(this.params.node_radius * this.camera.zoom, &this.params);
        let dense = this.store.node_count() + this.store.edge_count() > RASTER_MIN_ELEMENTS;
        let use_raster = this.raster_enabled && dense;
        let raster_nodes = lod != Lod::Full;
        let mut request = None;
        let rev = this.store.revision();
        let throttled = this
            .last_raster_request
            .is_some_and(|t| t.elapsed().as_millis() < RASTER_REFRESH_MS);
        if use_raster
            && !this.raster.is_pending()
            && !(throttled && this.raster.matches_mode(raster_nodes))
            && this.raster.needs_refresh(
                rev,
                &this.camera,
                viewport,
                this.style_version,
                raster_nodes,
            )
        {
            let key = RasterCache::key_for(
                rev,
                this.camera,
                viewport,
                scale,
                this.style_version,
                raster_nodes,
            );
            this.raster.mark_pending(key);
            this.last_raster_request = Some(Instant::now());
            let colors = raster_colors(
                &this.style,
                &this.looks,
                this.store.vertex_label_count(),
                lod == Lod::Far,
                raster_nodes,
            );
            request = Some(spawn_raster(this.store.clone(), key, this.params, colors));
        }
        (use_raster, raster_nodes, request)
    });
    if let Some(rx) = request {
        let entity = entity.clone();
        cx.spawn(async move |cx| {
            if let Ok(result) = rx.await {
                entity.update(cx, |this, cx| {
                    this.raster.accept(result);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    window.with_content_mask(Some(gpui_kit::ContentMask { bounds }), |window| {
        let painted_raster = use_raster
            && entity.update(cx, |this, _| {
                let camera = this.camera;
                this.raster.matches_mode(raster_nodes) && this.raster.paint(&camera, bounds, window)
            });

        let mut renderer = entity.update(cx, |this, _| {
            std::mem::replace(&mut this.renderer, Box::new(GpuiPrimitives::default()))
        });
        let (labels, label_color, scene) = {
            let this = entity.read(cx);
            let mut params = this.params;
            if painted_raster {
                params.subset = if raster_nodes {
                    Subset::Highlighted {
                        extra: this.hovered,
                    }
                } else {
                    Subset::NodesAndHighlightedEdges
                };
            }
            let input = FrameInput {
                store: &this.store,
                camera: &this.camera,
                params: &params,
                style: &this.style,
                looks: &this.looks,
                hovered: this.hovered,
                bounds,
            };
            let (scene, labels) = renderer.paint(&input, window, cx);
            (labels, this.style.label, scene)
        };
        entity.update(cx, |this, _| {
            this.renderer = renderer;
            if !painted_raster {
                this.last_scene = scene;
            }
        });
        window.paint_layer(bounds, |window| {
            paint_labels(&labels, label_color, window, cx)
        });
        paint_overlays(entity, bounds, window, cx);
    });

    let paint_ms = started.elapsed().as_secs_f32() * 1000.0;
    entity.update(cx, |this, _| {
        this.stats.record(started, paint_ms, use_raster)
    });

    if let Some(run) = finished_bench {
        let summary = entity.read(cx).stats.summary();
        let (n, e) = {
            let s = entity.read(cx).store();
            (s.node_count(), s.edge_count())
        };
        run.report(n, e, &summary);
        let st = &entity.read(cx).stats;
        let (r, v) = (st.intervals_for(true), st.intervals_for(false));
        let count = |m: bool| st.raster.iter().filter(|x| **x == m).count();
        println!(
            "  raster frames {:>4}: interval p50={:.2} p95={:.2} max={:.2}",
            count(true),
            r.p50,
            r.p95,
            r.max
        );
        println!(
            "  vector frames {:>4}: interval p50={:.2} p95={:.2} max={:.2}",
            count(false),
            v.p50,
            v.p95,
            v.max
        );
        if run.quit_when_done {
            cx.quit();
        }
    }
    if animating {
        window.request_animation_frame();
    }
}

fn paint_overlays(
    entity: &gpui_kit::Entity<GraphCanvas>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let scale = window.scale_factor();
    let retired = entity.update(cx, |this, _| {
        this.refresh_minimap(scale);
        std::mem::take(&mut this.retired_images)
    });
    for image in retired {
        let _ = window.drop_image(image);
    }
    let this = entity.read(cx);
    let o = [f32::from(bounds.origin.x), f32::from(bounds.origin.y)];
    let to_bounds = |r: [f32; 4]| {
        Bounds::new(
            point(px(r[0] + o[0]), px(r[1] + o[1])),
            size(px(r[2] - r[0]), px(r[3] - r[1])),
        )
    };

    if let Drag::Rect { start, current } = this.drag {
        let r = rect_from_points(start, current);
        window.paint_quad(fill(to_bounds(r), this.style.rubber_band));
        window.paint_quad(outline(
            to_bounds(r),
            this.style.selection,
            gpui_kit::BorderStyle::Solid,
        ));
    }

    if this.show_minimap
        && let Some(t) = this.minimap_transform()
    {
        let map = this.minimap_rect();
        window.paint_quad(
            fill(to_bounds(map), this.style.minimap_bg).corner_radii(this.style.minimap_radius),
        );
        if let Some((_, image)) = &this.minimap_image {
            let _ = window.paint_image(
                to_bounds(map),
                to_bounds(map),
                gpui_kit::Corners::all(this.style.minimap_radius),
                image.clone(),
                0,
                false,
            );
        }
        let vis = this.camera.visible_world_rect(this.viewport());
        let a = t.world_to_map([vis[0], vis[1]]);
        let b = t.world_to_map([vis[2], vis[3]]);
        let r = [
            a[0].max(map[0]),
            a[1].max(map[1]),
            b[0].min(map[2]),
            b[1].min(map[3]),
        ];
        if r[2] > r[0] && r[3] > r[1] {
            window.paint_quad(outline(
                to_bounds(r),
                this.style.minimap_viewport,
                gpui_kit::BorderStyle::Solid,
            ));
        }
    }

    let hud = this.perf_hud.then(|| {
        let st = this.stats.recent(120);
        let text: SharedString = format!(
            "{:>4.0} fps | frame p95 {:>5.2} ms | paint p50 {:>5.2} p95 {:>5.2} ms | {} nodes {} edges | {} visible",
            st.fps,
            st.interval.p95,
            st.paint.p50,
            st.paint.p95,
            this.store.node_count(),
            this.store.edge_count(),
            this.last_scene.visible_nodes,
        )
        .into();
        (text, this.style.hud_bg, this.style.hud_text)
    });
    let hud_radius = this.style.hud_radius;
    if let Some((text, hud_bg, hud_text)) = hud {
        window
            .paint_quad(fill(to_bounds([8.0, 8.0, 700.0, 30.0]), hud_bg).corner_radii(hud_radius));
        let label = renderer::PendingLabel {
            text,
            anchor: [354.0 + o[0], 12.0 + o[1]],
            font_px: 12.0,
        };
        paint_labels(std::slice::from_ref(&label), hud_text, window, cx);
    }
}
