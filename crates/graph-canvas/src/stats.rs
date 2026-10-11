//! Frame timing statistics for the perf HUD and the render benchmark.

use std::time::Instant;

#[derive(Default)]
pub struct FrameStats {
    /// Time between successive paints (ms): what the user perceives.
    pub intervals_ms: Vec<f32>,
    /// CPU time spent in the canvas prepaint+paint (ms).
    pub paint_ms: Vec<f32>,
    /// Draw mode of each frame (aligned with `paint_ms`): true = raster level of detail.
    pub raster: Vec<bool>,
    last_paint: Option<Instant>,
}

impl FrameStats {
    pub fn record(&mut self, paint_started: Instant, paint_ms: f32, raster: bool) {
        if let Some(prev) = self.last_paint {
            self.intervals_ms
                .push(paint_started.duration_since(prev).as_secs_f32() * 1000.0);
        }
        self.last_paint = Some(paint_started);
        self.paint_ms.push(paint_ms);
        self.raster.push(raster);
    }

    /// Interval percentiles for frames drawn in one mode. Interval `i` ends at paint `i + 1`.
    pub fn intervals_for(&self, raster: bool) -> Percentiles {
        let v: Vec<f32> = self
            .intervals_ms
            .iter()
            .zip(self.raster.iter().skip(1))
            .filter(|(_, r)| **r == raster)
            .map(|(i, _)| *i)
            .collect();
        Percentiles::of(&v)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Stats over the most recent `window` samples.
    pub fn recent(&self, window: usize) -> Summary {
        let tail = |v: &Vec<f32>| v[v.len().saturating_sub(window)..].to_vec();
        Summary::of(&tail(&self.intervals_ms), &tail(&self.paint_ms))
    }

    pub fn summary(&self) -> Summary {
        Summary::of(&self.intervals_ms, &self.paint_ms)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    pub frames: usize,
    pub fps: f32,
    pub interval: Percentiles,
    pub paint: Percentiles,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Percentiles {
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

impl Percentiles {
    pub fn of(samples: &[f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut v = samples.to_vec();
        v.sort_by(f32::total_cmp);
        let at = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
        Self {
            p50: at(0.5),
            p95: at(0.95),
            p99: at(0.99),
            max: v[v.len() - 1],
        }
    }
}

impl Summary {
    fn of(intervals: &[f32], paint: &[f32]) -> Self {
        let total: f32 = intervals.iter().sum();
        Self {
            frames: paint.len(),
            fps: if total > 0.0 {
                intervals.len() as f32 / (total / 1000.0)
            } else {
                0.0
            },
            interval: Percentiles::of(intervals),
            paint: Percentiles::of(paint),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let p = Percentiles::of(&v);
        assert_eq!(p.max, 100.0);
        assert!((p.p50 - 50.0).abs() <= 1.0);
        assert!((p.p95 - 95.0).abs() <= 1.0);
    }
}
