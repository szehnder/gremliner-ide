//! Mapping between the graph's world bounds and a minimap rectangle on screen.

use super::{Rect, Vec2};

/// Uniform scale + offset that fits `world` inside `map` (screen rect), centred, preserving aspect ratio.
#[derive(Clone, Copy, Debug)]
pub struct MinimapTransform {
    scale: f32,
    offset: Vec2,
}

impl MinimapTransform {
    pub fn new(world: Rect, map: Rect, padding: f32) -> Self {
        let ww = (world[2] - world[0]).max(1e-3);
        let wh = (world[3] - world[1]).max(1e-3);
        let mw = (map[2] - map[0] - 2.0 * padding).max(1.0);
        let mh = (map[3] - map[1] - 2.0 * padding).max(1.0);
        let scale = (mw / ww).min(mh / wh);
        let offset = [
            map[0] + padding + (mw - ww * scale) * 0.5 - world[0] * scale,
            map[1] + padding + (mh - wh * scale) * 0.5 - world[1] * scale,
        ];
        Self { scale, offset }
    }

    pub fn world_to_map(&self, p: Vec2) -> Vec2 {
        [
            p[0] * self.scale + self.offset[0],
            p[1] * self.scale + self.offset[1],
        ]
    }

    pub fn map_to_world(&self, s: Vec2) -> Vec2 {
        [
            (s[0] - self.offset[0]) / self.scale,
            (s[1] - self.offset[1]) / self.scale,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Camera;

    #[test]
    fn scenario_graph_rendering_navigate_via_minimap() {
        let world = [-1000.0, -500.0, 1000.0, 500.0];
        let map = [600.0, 400.0, 800.0, 500.0]; // 200x100 minimap in the bottom-right
        let t = MinimapTransform::new(world, map, 0.0);
        // Clicking the minimap's top-left corner centres the camera on the graph's top-left region.
        let target = t.map_to_world([map[0], map[1]]);
        assert!((target[0] - world[0]).abs() < 1e-2 && (target[1] - world[1]).abs() < 1e-2);
        let mut cam = Camera {
            center: [0.0, 0.0],
            zoom: 1.0,
        };
        cam.center = target;
        assert_eq!(cam.center, target);
    }

    #[test]
    fn round_trip() {
        let t = MinimapTransform::new([0.0, 0.0, 50.0, 200.0], [10.0, 10.0, 110.0, 60.0], 4.0);
        let p = [20.0, 120.0];
        let q = t.map_to_world(t.world_to_map(p));
        assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3);
    }
}
