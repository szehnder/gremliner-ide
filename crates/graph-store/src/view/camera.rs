//! World ↔ screen transform. Screen coordinates are local to the canvas (origin top-left, in logical pixels).

use super::{Rect, Vec2};

pub const MIN_ZOOM: f32 = 0.0005;
pub const MAX_ZOOM: f32 = 64.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// World point shown at the centre of the viewport.
    pub center: Vec2,
    /// Screen pixels per world unit.
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            zoom: 1.0,
        }
    }
}

impl Camera {
    #[inline]
    pub fn world_to_screen(&self, p: Vec2, viewport: Vec2) -> Vec2 {
        [
            (p[0] - self.center[0]) * self.zoom + viewport[0] * 0.5,
            (p[1] - self.center[1]) * self.zoom + viewport[1] * 0.5,
        ]
    }

    #[inline]
    pub fn screen_to_world(&self, s: Vec2, viewport: Vec2) -> Vec2 {
        [
            (s[0] - viewport[0] * 0.5) / self.zoom + self.center[0],
            (s[1] - viewport[1] * 0.5) / self.zoom + self.center[1],
        ]
    }

    /// The world rectangle currently visible.
    pub fn visible_world_rect(&self, viewport: Vec2) -> Rect {
        let a = self.screen_to_world([0.0, 0.0], viewport);
        let b = self.screen_to_world(viewport, viewport);
        [a[0], a[1], b[0], b[1]]
    }

    /// Pan by a screen-space delta (dragging the background moves the world with the cursor).
    pub fn pan_screen(&mut self, delta: Vec2) {
        self.center[0] -= delta[0] / self.zoom;
        self.center[1] -= delta[1] / self.zoom;
    }

    /// Multiply zoom by `factor`, keeping the world point under `screen_pt` fixed on screen.
    pub fn zoom_about(&mut self, screen_pt: Vec2, factor: f32, viewport: Vec2) {
        let before = self.screen_to_world(screen_pt, viewport);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.screen_to_world(screen_pt, viewport);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    /// Camera that frames `bounds` with `margin` screen pixels on every side, using the largest zoom
    /// that fits. Degenerate bounds (a single point) use `fallback_zoom`.
    pub fn fit(bounds: Rect, viewport: Vec2, margin: f32, fallback_zoom: f32) -> Camera {
        let w = bounds[2] - bounds[0];
        let h = bounds[3] - bounds[1];
        let avail = [
            (viewport[0] - 2.0 * margin).max(1.0),
            (viewport[1] - 2.0 * margin).max(1.0),
        ];
        let zoom = if w <= f32::EPSILON && h <= f32::EPSILON {
            fallback_zoom
        } else {
            let zx = if w > f32::EPSILON {
                avail[0] / w
            } else {
                f32::INFINITY
            };
            let zy = if h > f32::EPSILON {
                avail[1] / h
            } else {
                f32::INFINITY
            };
            zx.min(zy)
        };
        Camera {
            center: [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5],
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VP: Vec2 = [800.0, 600.0];

    fn close(a: Vec2, b: Vec2) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn round_trip() {
        let c = Camera {
            center: [10.0, -5.0],
            zoom: 2.5,
        };
        let w = [3.0, 7.0];
        assert!(close(c.screen_to_world(c.world_to_screen(w, VP), VP), w));
    }

    #[test]
    fn scenario_graph_rendering_zoom_keeps_point_under_cursor() {
        let mut c = Camera {
            center: [0.0, 0.0],
            zoom: 1.0,
        };
        let node_world = [120.0, -40.0];
        let cursor = c.world_to_screen(node_world, VP);
        for factor in [1.25, 1.25, 2.0, 0.5, 8.0] {
            c.zoom_about(cursor, factor, VP);
            assert!(
                close(c.world_to_screen(node_world, VP), cursor),
                "node drifted at zoom {}",
                c.zoom
            );
        }
        assert!(c.zoom > 1.0);
    }

    #[test]
    fn scenario_graph_rendering_fit_to_view() {
        let bounds = [-100.0, -50.0, 300.0, 150.0];
        let margin = 20.0;
        let c = Camera::fit(bounds, VP, margin, 1.0);
        let tl = c.world_to_screen([bounds[0], bounds[1]], VP);
        let br = c.world_to_screen([bounds[2], bounds[3]], VP);
        // Every corner inside the viewport with the margin...
        assert!(tl[0] >= margin - 1e-3 && tl[1] >= margin - 1e-3);
        assert!(br[0] <= VP[0] - margin + 1e-3 && br[1] <= VP[1] - margin + 1e-3);
        // ...and the zoom is the largest that achieves it: one axis touches the margin.
        let touches_x = (tl[0] - margin).abs() < 1e-3 && (br[0] - (VP[0] - margin)).abs() < 1e-3;
        let touches_y = (tl[1] - margin).abs() < 1e-3 && (br[1] - (VP[1] - margin)).abs() < 1e-3;
        assert!(touches_x || touches_y);
    }

    #[test]
    fn fit_single_point_uses_fallback() {
        let c = Camera::fit([5.0, 5.0, 5.0, 5.0], VP, 20.0, 3.0);
        assert_eq!(c.zoom, 3.0);
        assert_eq!(c.center, [5.0, 5.0]);
    }

    #[test]
    fn pan_moves_world_with_cursor() {
        let mut c = Camera {
            center: [0.0, 0.0],
            zoom: 2.0,
        };
        let p = c.world_to_screen([10.0, 10.0], VP);
        c.pan_screen([30.0, -12.0]);
        assert!(close(
            c.world_to_screen([10.0, 10.0], VP),
            [p[0] + 30.0, p[1] - 12.0]
        ));
    }
}
