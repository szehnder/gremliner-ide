//! Pure view geometry shared by the GPUI canvas and the headless render benchmark (ADR-0006):
//! camera math, a spatial grid for hit-testing, level-of-detail rules, and a renderer-agnostic
//! scene builder that emits primitives into a [`scene::SceneSink`].

pub mod camera;
pub mod grid;
pub mod minimap;
pub mod raster;
pub mod scene;

pub use camera::Camera;
pub use grid::SpatialGrid;
pub use scene::{Lod, SceneParams, SceneSink, SceneStats, Subset, build_scene};

pub type Vec2 = [f32; 2];

#[inline]
pub fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

#[inline]
pub fn add(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] + b[0], a[1] + b[1]]
}

#[inline]
pub fn scale(a: Vec2, s: f32) -> Vec2 {
    [a[0] * s, a[1] * s]
}

#[inline]
pub fn len(a: Vec2) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

/// Distance from point `p` to segment `a`–`b`.
pub fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    if l2 <= f32::EPSILON {
        return len(sub(p, a));
    }
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0);
    len(sub(p, add(a, scale(ab, t))))
}

/// Axis-aligned rectangle `[min_x, min_y, max_x, max_y]`.
pub type Rect = [f32; 4];

#[inline]
pub fn rect_from_points(a: Vec2, b: Vec2) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    ]
}

#[inline]
pub fn rect_contains(r: Rect, p: Vec2) -> bool {
    p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
}

#[inline]
pub fn rects_intersect(a: Rect, b: Rect) -> bool {
    a[0] <= b[2] && a[2] >= b[0] && a[1] <= b[3] && a[3] >= b[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_distance() {
        assert!((point_segment_distance([0.0, 1.0], [-1.0, 0.0], [1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((point_segment_distance([3.0, 0.0], [-1.0, 0.0], [1.0, 0.0]) - 2.0).abs() < 1e-6);
        assert!(
            (point_segment_distance([0.0, 0.0], [1.0, 1.0], [1.0, 1.0]) - 2f32.sqrt()).abs() < 1e-6
        );
    }
}
