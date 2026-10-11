//! Raster level of detail (ADR-0032): dense, zoomed-out views are rasterised off the main thread into a
//! cached image. Every frame paints that image transformed to the current camera (pan = translate,
//! zoom = scale), so frame cost stays flat regardless of graph size; a fresh raster is requested
//! whenever the cached one has drifted too far and swapped in when ready.

use std::sync::Arc;

use gpui_kit::{Bounds, Corners, Pixels, RenderImage, Window, point, px, size};
use graph_store::GraphStore;
use graph_store::view::raster::{RasterColors, RasterRegion, rasterize};
use graph_store::view::{Camera, SceneParams, Vec2};

/// Extra area rasterised around the viewport (fraction of the viewport on each side) so panning can
/// reuse the cached image for a while.
const MARGIN: f32 = 0.2;
/// Zoom drift (ratio) after which a new raster is requested.
const ZOOM_TOLERANCE: f32 = 0.04;

/// Inputs that determine a raster; a cached image is valid for exactly one key (up to camera drift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterKey {
    pub revision: u64,
    pub camera: Camera,
    pub viewport: Vec2,
    pub region: RasterRegion,
    pub style_version: u64,
    /// Whether nodes are in the raster (zoomed out) or drawn as vectors (zoomed in).
    pub nodes: bool,
}

pub struct RasterResult {
    pub key: RasterKey,
    pub image: Arc<RenderImage>,
}

#[derive(Default)]
pub struct RasterCache {
    current: Option<RasterResult>,
    pending: Option<RasterKey>,
    /// Images replaced since the last paint; freed from the GPU atlas on the next paint.
    retired: Vec<Arc<RenderImage>>,
}

impl RasterCache {
    pub fn has_image(&self) -> bool {
        self.current.is_some()
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Accepts a finished raster (ignored if a newer request superseded it... we still show it if it is
    /// better than nothing, since any raster at the right revision is a valid approximation).
    pub fn accept(&mut self, result: RasterResult) {
        if self.pending == Some(result.key) {
            self.pending = None;
        }
        if let Some(old) = self.current.replace(result) {
            self.retired.push(old.image);
        }
    }

    pub fn invalidate(&mut self) {
        if let Some(old) = self.current.take() {
            self.retired.push(old.image);
        }
    }

    /// Whether the cached raster (or the one in flight) is still close enough to `camera`.
    pub fn needs_refresh(
        &self,
        revision: u64,
        camera: &Camera,
        viewport: Vec2,
        style_version: u64,
        nodes: bool,
    ) -> bool {
        let key = self.pending.or(self.current.as_ref().map(|c| c.key));
        let Some(key) = key else { return true };
        if key.revision != revision
            || key.viewport != viewport
            || key.style_version != style_version
            || key.nodes != nodes
        {
            return true;
        }
        let ratio = camera.zoom / key.camera.zoom;
        if (ratio - 1.0).abs() > ZOOM_TOLERANCE {
            return true;
        }
        // Visible area must stay inside the rasterised region.
        let vis_world = camera.visible_world_rect(viewport);
        let tl = key
            .camera
            .world_to_screen([vis_world[0], vis_world[1]], key.viewport);
        let br = key
            .camera
            .world_to_screen([vis_world[2], vis_world[3]], key.viewport);
        let r = key.region;
        tl[0] < r.offset[0]
            || tl[1] < r.offset[1]
            || br[0] > r.offset[0] + r.size[0]
            || br[1] > r.offset[1] + r.size[1]
    }

    /// Builds the request key for the current camera.
    pub fn key_for(
        revision: u64,
        camera: Camera,
        viewport: Vec2,
        scale: f32,
        style_version: u64,
        nodes: bool,
    ) -> RasterKey {
        let m = [viewport[0] * MARGIN, viewport[1] * MARGIN];
        RasterKey {
            revision,
            camera,
            viewport,
            region: RasterRegion {
                offset: [-m[0], -m[1]],
                size: [viewport[0] + 2.0 * m[0], viewport[1] + 2.0 * m[1]],
                scale,
            },
            style_version,
            nodes,
        }
    }

    /// Whether the cached image matches the current draw mode (nodes included or not).
    pub fn matches_mode(&self, nodes: bool) -> bool {
        self.current.as_ref().is_some_and(|c| c.key.nodes == nodes)
    }

    pub fn mark_pending(&mut self, key: RasterKey) {
        self.pending = Some(key);
    }

    /// Paints the cached raster transformed to `camera`. Returns false if there is nothing to paint.
    pub fn paint(&mut self, camera: &Camera, bounds: Bounds<Pixels>, window: &mut Window) -> bool {
        for img in self.retired.drain(..) {
            let _ = window.drop_image(img);
        }
        let Some(cur) = &self.current else {
            return false;
        };
        let k = cur.key;
        // Region's top-left in the raster camera's canvas space -> world -> current canvas space.
        let world_tl = k.camera.screen_to_world(k.region.offset, k.viewport);
        let tl = camera.world_to_screen(world_tl, k.viewport);
        let s = camera.zoom / k.camera.zoom;
        let image_bounds = Bounds::new(
            point(bounds.origin.x + px(tl[0]), bounds.origin.y + px(tl[1])),
            size(px(k.region.size[0] * s), px(k.region.size[1] * s)),
        );
        let _ = window.paint_image(
            bounds,
            image_bounds,
            Corners::all(Pixels::ZERO),
            cur.image.clone(),
            0,
            false,
        );
        true
    }
}

/// Dedicated pool for raster jobs, sized to leave cores for the UI thread and GPUI.
fn raster_pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let n = std::thread::available_parallelism().map_or(4, |n| n.get());
        rayon::ThreadPoolBuilder::new()
            .num_threads((n / 2).max(2))
            .thread_name(|i| format!("graph-raster-{i}"))
            .build()
            .unwrap_or_else(|_| {
                rayon::ThreadPoolBuilder::new()
                    .build()
                    .expect("default rayon pool")
            })
    })
}

/// Rasterises on the raster pool and returns the finished image through a oneshot channel.
pub fn spawn_raster(
    store: Arc<GraphStore>,
    key: RasterKey,
    params: SceneParams,
    colors: RasterColors,
) -> futures::channel::oneshot::Receiver<RasterResult> {
    let (tx, rx) = futures::channel::oneshot::channel();
    let pool = raster_pool();
    pool.spawn(move || {
        let img = raster_pool().install(|| {
            rasterize(
                &store,
                &key.camera,
                key.viewport,
                key.region,
                &params,
                &colors,
            )
        });
        let w = u32::try_from(img.width).unwrap_or(1).max(1);
        let h = u32::try_from(img.height).unwrap_or(1).max(1);
        let Some(buffer) = image::RgbaImage::from_raw(w, h, img.bgra) else {
            return;
        };
        let frame = image::Frame::new(buffer);
        let image = Arc::new(RenderImage::new(vec![frame]));
        let _ = tx.send(RasterResult { key, image });
    });
    rx
}

/// Converts GPUI colours to the rasteriser's straight-alpha RGBA bytes.
pub fn raster_colors(style: &crate::theme::CanvasStyle, far: bool, nodes: bool) -> RasterColors {
    let to = |c: gpui_kit::Hsla| {
        let r = gpui_kit::Rgba::from(c);
        [
            (r.r * 255.0) as u8,
            (r.g * 255.0) as u8,
            (r.b * 255.0) as u8,
            (r.a * 255.0) as u8,
        ]
    };
    RasterColors {
        palette: style.palette.iter().map(|c| to(*c)).collect(),
        edge: to(if far { style.edge_far } else { style.edge }),
        nodes,
    }
}
