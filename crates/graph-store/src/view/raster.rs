//! Parallel CPU rasteriser for zoomed-out views (ADR-0006 mitigation).
//!
//! When tens of thousands of elements are on screen they are a few pixels each, and GPUI's per-primitive
//! overhead (~200 ns) dominates the frame. Rasterising them here into one image — anti-aliased hairlines
//! and discs, row bands processed in parallel — and painting that image as a single primitive keeps
//! frames inside budget. Zoomed in, culling keeps counts low and the vector path is used instead.

use rayon::prelude::*;

use super::scene::edge_width_px;
use super::{Camera, Lod, SceneParams, Vec2};
use crate::{GraphStore, flags};

/// Rows per parallel work item.
const BAND: usize = 16;

/// Straight-alpha colours (RGBA, 0–255) used by the rasteriser.
#[derive(Clone, Debug)]
pub struct RasterColors {
    pub palette: Vec<[u8; 4]>,
    pub edge: [u8; 4],
    /// Whether nodes are rasterised (false = edges only; nodes are drawn as vectors on top).
    pub nodes: bool,
}

/// What to rasterise: the region of the canvas starting at `offset` (logical px, may be negative) of
/// `size` logical px, at `scale` device px per logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterRegion {
    pub offset: Vec2,
    pub size: Vec2,
    pub scale: f32,
}

impl RasterRegion {
    pub fn device_size(&self) -> (usize, usize) {
        (
            (self.size[0] * self.scale).ceil() as usize,
            (self.size[1] * self.scale).ceil() as usize,
        )
    }
}

/// A rasterised frame in straight-alpha BGRA, as GPUI's `RenderImage` expects.
pub struct RasterImage {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
    pub nodes: usize,
    pub edges: usize,
    /// Pixel blend operations performed (cost metric).
    pub blends: u64,
}

struct Disc {
    c: Vec2,
    r: f32,
    rgba: [f32; 4],
}

struct Seg {
    a: Vec2,
    b: Vec2,
    min_y: f32,
    max_y: f32,
}

/// Rasterises `store` as seen by `camera` in a canvas of `viewport` logical px, limited to `region`.
/// Selected nodes and edges touching them are skipped; callers draw those on top as vectors.
pub fn rasterize(
    store: &GraphStore,
    camera: &Camera,
    viewport: Vec2,
    region: RasterRegion,
    params: &SceneParams,
    colors: &RasterColors,
) -> RasterImage {
    let (w, h) = region.device_size();
    let s = region.scale;
    let radius_logical = (params.node_radius * camera.zoom).max(params.min_node_px);
    let lod = Lod::for_radius(params.node_radius * camera.zoom, params);
    let r = radius_logical * s;
    let line_w = edge_width_px(lod, radius_logical) * s;
    let to_dev = |p: Vec2| {
        let q = camera.world_to_screen(p, viewport);
        [(q[0] - region.offset[0]) * s, (q[1] - region.offset[1]) * s]
    };
    let positions = store.positions();
    let node_flags = store.node_flags();
    let dev: Vec<Vec2> = positions.par_iter().map(|p| to_dev(*p)).collect();
    let pad = r + 2.0;
    let inside =
        |p: Vec2| p[0] >= -pad && p[1] >= -pad && p[0] <= w as f32 + pad && p[1] <= h as f32 + pad;

    let skip = |f: u8| f & (flags::HIDDEN | flags::SELECTED) != 0;
    let (srcs, dsts) = store.edge_endpoints();
    let segs: Vec<Seg> = srcs
        .par_iter()
        .zip(dsts.par_iter())
        .filter_map(|(si, di)| {
            let (si, di) = (si.0 as usize, di.0 as usize);
            if skip(node_flags[si])
                || skip(node_flags[di])
                || node_flags[si] & node_flags[di] & flags::HIDDEN != 0
            {
                return None;
            }
            let (a, b) = (dev[si], dev[di]);
            let (min_x, max_x) = (a[0].min(b[0]), a[0].max(b[0]));
            let (min_y, max_y) = (a[1].min(b[1]), a[1].max(b[1]));
            if max_x < -1.0 || max_y < -1.0 || min_x > w as f32 + 1.0 || min_y > h as f32 + 1.0 {
                return None;
            }
            if (b[0] - a[0]).abs() + (b[1] - a[1]).abs() < 0.5 {
                return None;
            }
            Some(Seg { a, b, min_y, max_y })
        })
        .collect();

    let to_f = |c: [u8; 4]| {
        [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            c[3] as f32 / 255.0,
        ]
    };
    let palette: Vec<[f32; 4]> = colors.palette.iter().map(|c| to_f(*c)).collect();
    let labels = store.node_labels();
    let discs: Vec<Disc> = dev
        .iter()
        .enumerate()
        .filter(|(i, p)| colors.nodes && !skip(node_flags[*i]) && inside(**p))
        .map(|(i, p)| Disc {
            c: *p,
            r,
            rgba: palette[labels[i].0 as usize % palette.len().max(1)],
        })
        .collect();

    // Each band owns its rows of the output. Edges all share one colour, so the edge layer is a single
    // transmittance channel (t *= 1 - alpha * coverage); nodes are blended into a small RGBA scratch.
    let edge = to_f(colors.edge);
    let mut bgra = vec![0u8; w * h * 4];
    let blends: u64 = bgra
        .par_chunks_mut(w * 4 * BAND)
        .enumerate()
        .map(|(band, out)| {
            let rows_here = out.len() / (w * 4).max(1);
            let y0 = band * BAND;
            let y1 = y0 + rows_here;
            let mut trans = vec![1f32; w * rows_here];
            let mut nodes = vec![[0f32; 4]; w * rows_here];
            let mut n = 0u64;
            for seg in &segs {
                if seg.max_y + line_w < y0 as f32 - 1.0 || seg.min_y - line_w > y1 as f32 + 1.0 {
                    continue;
                }
                n += if line_w <= 1.25 {
                    draw_hairline(
                        &mut trans,
                        w,
                        y0,
                        y1,
                        seg.a,
                        seg.b,
                        edge[3] * line_w.min(1.0),
                    )
                } else {
                    draw_line(&mut trans, w, y0, y1, seg.a, seg.b, line_w, edge[3])
                };
            }
            for d in &discs {
                if d.c[1] + d.r + 1.0 < y0 as f32 || d.c[1] - d.r - 1.0 > y1 as f32 {
                    continue;
                }
                n += draw_disc(&mut nodes, w, y0, y1, d.c, d.r, d.rgba);
            }
            // Compose: nodes over edges, written as straight-alpha BGRA.
            for (i, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let ea = 1.0 - trans[i];
                let nd = nodes[i];
                let na = nd[3];
                let a = na + ea * (1.0 - na);
                if a <= 0.0 {
                    continue;
                }
                let k = ea * (1.0 - na);
                let r = (nd[0] + edge[0] * k) / a;
                let g = (nd[1] + edge[1] * k) / a;
                let b = (nd[2] + edge[2] * k) / a;
                px[0] = (b.min(1.0) * 255.0) as u8;
                px[1] = (g.min(1.0) * 255.0) as u8;
                px[2] = (r.min(1.0) * 255.0) as u8;
                px[3] = (a.min(1.0) * 255.0) as u8;
            }
            n
        })
        .sum();
    RasterImage {
        width: w,
        height: h,
        bgra,
        nodes: discs.len(),
        edges: segs.len(),
        blends,
    }
}

#[inline]
fn blend(px: &mut [f32; 4], rgba: [f32; 4], coverage: f32) {
    let a = rgba[3] * coverage;
    if a <= 0.0 {
        return;
    }
    let inv = 1.0 - a;
    px[0] = rgba[0] * a + px[0] * inv;
    px[1] = rgba[1] * a + px[1] * inv;
    px[2] = rgba[2] * a + px[2] * inv;
    px[3] = a + px[3] * inv;
}

/// Anti-aliased line of `width` device px (Wu-style coverage across the minor axis) multiplied into the
/// transmittance buffer `trans`, clipped to rows `[y0, y1)` of the band whose first row is `trans[0]`.
#[allow(clippy::too_many_arguments)]
fn draw_line(
    trans: &mut [f32],
    w: usize,
    y0: usize,
    y1: usize,
    a: Vec2,
    b: Vec2,
    width: f32,
    alpha: f32,
) -> u64 {
    let mut n = 0u64;
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let half = (width * 0.5).max(0.5);
    // Thin lines get proportionally less opacity instead of sub-pixel width.
    let k = alpha * width.min(1.0);
    let mut plot = |x: i64, y: i64, c: f32| {
        if x >= 0 && (x as usize) < w && y >= y0 as i64 && (y as usize) < y1 {
            trans[(y as usize - y0) * w + x as usize] *= 1.0 - k * c;
            n += 1;
        }
    };
    let span = half.ceil() as i64;
    if dx.abs() >= dy.abs() {
        // x-major: walk columns, covering pixels within `half` of the line's y.
        let (a, b) = if a[0] <= b[0] { (a, b) } else { (b, a) };
        let slope = if (b[0] - a[0]).abs() > f32::EPSILON {
            (b[1] - a[1]) / (b[0] - a[0])
        } else {
            0.0
        };
        // Restrict columns to those whose line y can reach this band.
        let (mut xs, mut xe) = (a[0].max(0.0), b[0].min(w as f32 - 1.0));
        if slope.abs() > 1e-6 {
            let xa = a[0] + ((y0 as f32 - half - 1.0) - a[1]) / slope;
            let xb = a[0] + ((y1 as f32 + half + 1.0) - a[1]) / slope;
            xs = xs.max(xa.min(xb));
            xe = xe.min(xa.max(xb));
        } else if a[1] + half + 1.0 < y0 as f32 || a[1] - half - 1.0 > y1 as f32 {
            return 0;
        }
        let mut x = xs.floor() as i64;
        while (x as f32) <= xe {
            let cy = a[1] + slope * (x as f32 + 0.5 - a[0]);
            let yi = cy.floor() as i64;
            for yy in (yi - span)..=(yi + span) {
                let d = ((yy as f32 + 0.5) - cy).abs();
                let c = (half + 0.5 - d).clamp(0.0, 1.0);
                if c > 0.0 {
                    plot(x, yy, c);
                }
            }
            x += 1;
        }
    } else {
        // y-major: walk rows of this band only.
        let (a, b) = if a[1] <= b[1] { (a, b) } else { (b, a) };
        let slope = (b[0] - a[0]) / (b[1] - a[1]);
        let ys = a[1].max(y0 as f32).floor() as i64;
        let ye = b[1].min(y1 as f32 - 1.0);
        let mut y = ys;
        while (y as f32) <= ye {
            let cx = a[0] + slope * (y as f32 + 0.5 - a[1]);
            let xi = cx.floor() as i64;
            for xx in (xi - span)..=(xi + span) {
                let d = ((xx as f32 + 0.5) - cx).abs();
                let c = (half + 0.5 - d).clamp(0.0, 1.0);
                if c > 0.0 {
                    plot(xx, y, c);
                }
            }
            y += 1;
        }
    }
    drop(plot);
    n
}

/// Xiaolin Wu anti-aliased hairline (two pixels per step) multiplied into `trans`, clipped to the band
/// rows `[y0, y1)` and columns `[0, w)`. `k` is the line's peak opacity.
fn draw_hairline(
    trans: &mut [f32],
    w: usize,
    y0: usize,
    y1: usize,
    a: Vec2,
    b: Vec2,
    k: f32,
) -> u64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let (band_lo, band_hi) = (y0 as f32, y1 as f32);
    let mut n = 0u64;
    if dx.abs() >= dy.abs() {
        let (a, b) = if a[0] <= b[0] { (a, b) } else { (b, a) };
        let g = if (b[0] - a[0]).abs() > f32::EPSILON {
            (b[1] - a[1]) / (b[0] - a[0])
        } else {
            0.0
        };
        // Column range where the line's centre y lies within the band (±1 row for the AA pair).
        let mut xs = a[0].max(0.0);
        let mut xe = b[0].min(w as f32 - 1.0);
        if g.abs() > 1e-6 {
            let xa = a[0] + (band_lo - 1.5 - a[1]) / g;
            let xb = a[0] + (band_hi + 0.5 - a[1]) / g;
            xs = xs.max(xa.min(xb));
            xe = xe.min(xa.max(xb));
        } else if a[1] < band_lo - 1.5 || a[1] > band_hi + 0.5 {
            return 0;
        }
        if xe < xs {
            return 0;
        }
        let x_first = xs.floor() as usize;
        let x_last = xe.floor() as usize;
        // y at pixel centre of column x, shifted so that floor() gives the upper pixel of the AA pair.
        let mut y = a[1] + g * (x_first as f32 + 0.5 - a[0]) - 0.5;
        for x in x_first..=x_last.min(w - 1) {
            let yi = y.floor();
            let f = y - yi;
            let row = yi as i64 - y0 as i64;
            if row >= 0 && (row as usize) < y1 - y0 {
                trans[row as usize * w + x] *= 1.0 - k * (1.0 - f);
                n += 1;
            }
            let row = row + 1;
            if row >= 0 && (row as usize) < y1 - y0 {
                trans[row as usize * w + x] *= 1.0 - k * f;
                n += 1;
            }
            y += g;
        }
    } else {
        let (a, b) = if a[1] <= b[1] { (a, b) } else { (b, a) };
        let g = (b[0] - a[0]) / (b[1] - a[1]);
        let ys = a[1].max(band_lo).floor().max(0.0) as usize;
        let ye = b[1].min(band_hi - 1.0);
        if ye < ys as f32 {
            return 0;
        }
        let y_last = ye.floor() as usize;
        let mut x = a[0] + g * (ys as f32 + 0.5 - a[1]) - 0.5;
        for yy in ys..=y_last.min(y1 - 1) {
            let xi = x.floor();
            let f = x - xi;
            let base = (yy - y0) * w;
            let xi = xi as i64;
            if xi >= 0 && (xi as usize) < w {
                trans[base + xi as usize] *= 1.0 - k * (1.0 - f);
                n += 1;
            }
            if xi + 1 >= 0 && ((xi + 1) as usize) < w {
                trans[base + (xi + 1) as usize] *= 1.0 - k * f;
                n += 1;
            }
            x += g;
        }
    }
    n
}

/// Anti-aliased filled disc clipped to the band.
fn draw_disc(
    rows: &mut [[f32; 4]],
    w: usize,
    y0: usize,
    y1: usize,
    c: Vec2,
    r: f32,
    rgba: [f32; 4],
) -> u64 {
    let mut n = 0u64;
    let ys = ((c[1] - r - 1.0).floor().max(y0 as f32)) as usize;
    let ye = ((c[1] + r + 1.0).ceil().min(y1 as f32)) as usize;
    let xs = (c[0] - r - 1.0).floor().max(0.0) as usize;
    let xe = ((c[0] + r + 1.0).ceil().max(0.0) as usize).min(w);
    for y in ys..ye {
        let fy = y as f32 + 0.5 - c[1];
        for x in xs..xe {
            let fx = x as f32 + 0.5 - c[0];
            let cov = (r + 0.5 - (fx * fx + fy * fy).sqrt()).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend(&mut rows[(y - y0) * w + x], rgba, cov);
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeIx;

    fn colors() -> RasterColors {
        RasterColors {
            palette: vec![[255, 0, 0, 255]],
            edge: [0, 0, 255, 255],
            nodes: true,
        }
    }

    fn pixel(img: &RasterImage, x: usize, y: usize) -> [u8; 4] {
        let i = (y * img.width + x) * 4;
        [
            img.bgra[i],
            img.bgra[i + 1],
            img.bgra[i + 2],
            img.bgra[i + 3],
        ]
    }

    #[test]
    fn draws_nodes_and_edges_where_expected() {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "x", "", [-40.0, 0.0]);
        let b = g.add_node(2.into(), "x", "", [40.0, 0.0]);
        g.add_edge(1.into(), a, b, "e");
        let cam = Camera {
            center: [0.0, 0.0],
            zoom: 0.25,
        }; // radius 1.5px -> Far/Medium
        let vp = [100.0, 50.0];
        let region = RasterRegion {
            offset: [0.0, 0.0],
            size: vp,
            scale: 1.0,
        };
        let img = rasterize(&g, &cam, vp, region, &SceneParams::default(), &colors());
        assert_eq!((img.width, img.height), (100, 50));
        assert_eq!((img.nodes, img.edges), (2, 1));
        // Node centres at x = 50 ± 10: red, opaque (BGRA).
        let n = pixel(&img, 40, 25);
        assert!(n[2] > 200 && n[3] > 200, "node pixel {n:?}");
        // Midpoint of the edge: blue.
        let e = pixel(&img, 50, 25);
        // A hairline on a pixel boundary splits its coverage across two rows.
        assert!(e[0] > 200 && e[3] > 60, "edge pixel {e:?}");
        // Far from everything: transparent.
        assert_eq!(pixel(&img, 5, 5)[3], 0);
    }

    #[test]
    fn selected_elements_are_left_for_the_vector_pass() {
        let mut g = GraphStore::new();
        let a = g.add_node(1.into(), "x", "", [0.0, 0.0]);
        let b = g.add_node(2.into(), "x", "", [10.0, 0.0]);
        g.add_edge(1.into(), a, b, "e");
        g.set_flag(NodeIx(0), flags::SELECTED, true);
        let cam = Camera {
            center: [5.0, 0.0],
            zoom: 1.0,
        };
        let vp = [60.0, 20.0];
        let img = rasterize(
            &g,
            &cam,
            vp,
            RasterRegion {
                offset: [0.0, 0.0],
                size: vp,
                scale: 1.0,
            },
            &SceneParams::default(),
            &colors(),
        );
        assert_eq!((img.nodes, img.edges), (1, 0));
    }

    #[test]
    fn hairline_conserves_coverage() {
        // Each column of a shallow hairline gets total coverage ~1 (split across two rows).
        let mut trans = vec![1f32; 64 * 32];
        draw_hairline(&mut trans, 64, 0, 32, [2.3, 10.2], [60.7, 17.9], 1.0);
        for x in 4..58 {
            let cov: f32 = (0..32).map(|y| 1.0 - trans[y * 64 + x]).sum();
            assert!((cov - 1.0).abs() < 0.02, "column {x} coverage {cov}");
        }
        // Banded drawing equals one-shot drawing.
        let mut banded = vec![1f32; 64 * 32];
        for (i, chunk) in banded.chunks_mut(64 * 8).enumerate() {
            draw_hairline(chunk, 64, i * 8, i * 8 + 8, [2.3, 10.2], [60.7, 17.9], 1.0);
        }
        assert!(trans.iter().zip(&banded).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn steep_and_shallow_lines_cover_their_path() {
        let mut trans = vec![1f32; 64 * 64];
        draw_line(&mut trans, 64, 0, 64, [2.0, 3.0], [60.0, 50.0], 1.0, 1.0);
        draw_line(&mut trans, 64, 0, 64, [10.0, 2.0], [14.0, 62.0], 1.0, 1.0);
        // Every row between the steep line's endpoints has some coverage near its x.
        for y in 3..61 {
            let cx = 10.0 + (14.0 - 10.0) * ((y as f32 + 0.5 - 2.0) / 60.0);
            let x = cx.floor() as usize;
            let cov: f32 = (x.saturating_sub(1)..=x + 1)
                .map(|xx| 1.0 - trans[y * 64 + xx])
                .sum();
            assert!(cov > 0.5, "row {y} coverage {cov}");
        }
    }

    #[test]
    fn bands_match_single_band() {
        // Band size must not change the output: rasterise with the normal band height and compare with
        // a reference that composes the whole image as one band.
        let g = crate::synth::generate(2000, 4000, 4, 5);
        let b = g.bounds().unwrap();
        let vp = [300.0, 200.0];
        let cam = Camera::fit(b, vp, 10.0, 1.0);
        let region = RasterRegion {
            offset: [0.0, 0.0],
            size: vp,
            scale: 1.0,
        };
        let p = SceneParams::default();
        let c = RasterColors {
            palette: vec![[200, 100, 50, 255], [50, 100, 200, 255]],
            edge: [180, 180, 180, 90],
            nodes: true,
        };
        let img = rasterize(&g, &cam, vp, region, &p, &c);

        let dev: Vec<Vec2> = g
            .positions()
            .iter()
            .map(|q| cam.world_to_screen(*q, vp))
            .collect();
        let r = (p.node_radius * cam.zoom).max(p.min_node_px);
        let lw = edge_width_px(Lod::for_radius(p.node_radius * cam.zoom, &p), r);
        let mut trans = vec![1f32; 300 * 200];
        let (s, d) = g.edge_endpoints();
        for (si, di) in s.iter().zip(d) {
            let (a, bb) = (dev[si.0 as usize], dev[di.0 as usize]);
            if (bb[0] - a[0]).abs() + (bb[1] - a[1]).abs() >= 0.5 {
                if lw <= 1.25 {
                    draw_hairline(&mut trans, 300, 0, 200, a, bb, 90.0 / 255.0 * lw.min(1.0));
                } else {
                    draw_line(&mut trans, 300, 0, 200, a, bb, lw, 90.0 / 255.0);
                }
            }
        }
        let mut nodes = vec![[0f32; 4]; 300 * 200];
        let labels = g.node_labels();
        let to_f = |c: [u8; 4]| {
            [
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                c[3] as f32 / 255.0,
            ]
        };
        for (i, q) in dev.iter().enumerate() {
            draw_disc(
                &mut nodes,
                300,
                0,
                200,
                *q,
                r,
                to_f(c.palette[labels[i].0 as usize % 2]),
            );
        }
        let diff = (0..300 * 200)
            .filter(|&i| {
                let na = nodes[i][3];
                let a = na + (1.0 - trans[i]) * (1.0 - na);
                ((a.min(1.0) * 255.0) as i32 - img.bgra[i * 4 + 3] as i32).abs() > 1
            })
            .count();
        assert_eq!(
            diff, 0,
            "banded output differs from the single-band reference in {diff} pixels"
        );
    }
}

#[cfg(test)]
mod perf_probe {
    use super::*;

    /// `cargo test --release -p graph-store perf_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn hairline_throughput() {
        let (w, h) = (1600usize, 1000usize);
        let mut rng = crate::synth::Rng::new(1);
        let lines: Vec<(Vec2, Vec2)> = (0..100_000)
            .map(|_| {
                let a = [rng.unit() * w as f32, rng.unit() * h as f32];
                let ang = rng.unit() * std::f32::consts::TAU;
                (a, [a[0] + ang.cos() * 250.0, a[1] + ang.sin() * 250.0])
            })
            .collect();
        let mut trans = vec![1f32; w * h];
        let t = std::time::Instant::now();
        let mut n = 0;
        for (a, b) in &lines {
            n += draw_hairline(&mut trans, w, 0, h, *a, *b, 0.3);
        }
        let dt = t.elapsed();
        println!(
            "single band: {n} blends in {dt:?} = {:.2} ns/blend",
            dt.as_nanos() as f64 / n as f64
        );

        let t = std::time::Instant::now();
        let mut n = 0;
        for (i, chunk) in trans.chunks_mut(w * BAND).enumerate() {
            let (y0, y1) = (i * BAND, i * BAND + chunk.len() / w);
            for (a, b) in &lines {
                if a[1].max(b[1]) + 2.0 < y0 as f32 || a[1].min(b[1]) - 2.0 > y1 as f32 {
                    continue;
                }
                n += draw_hairline(chunk, w, y0, y1, *a, *b, 0.3);
            }
        }
        let dt = t.elapsed();
        println!(
            "banded (serial): {n} blends in {dt:?} = {:.2} ns/blend",
            dt.as_nanos() as f64 / n as f64
        );
    }
}
