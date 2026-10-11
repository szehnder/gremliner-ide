//! `gremliner-cli render-bench`: headless CPU cost of building a frame (scene culling/LOD and the
//! zoomed-out rasteriser) along the same camera path as the in-app benchmark.

use std::time::Instant;

use graph_store::view::raster::{RasterColors, RasterRegion, rasterize};
use graph_store::view::scene::CountingSink;
use graph_store::view::{Camera, Lod, SceneParams, build_scene};

pub struct Args {
    pub nodes: usize,
    pub edges: usize,
    pub frames: usize,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

fn pct(v: &mut [f32], q: f32) -> f32 {
    v.sort_by(f32::total_cmp);
    v.get(((v.len().saturating_sub(1)) as f32 * q).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

pub fn run(a: &Args) {
    let t = Instant::now();
    let store = graph_store::synth::generate(a.nodes, a.edges, 8, 42);
    println!(
        "generated {} nodes / {} edges in {:.0} ms",
        store.node_count(),
        store.edge_count(),
        t.elapsed().as_secs_f32() * 1000.0
    );
    let vp = [a.width, a.height];
    let Some(bounds) = store.bounds() else { return };
    let fit = Camera::fit(bounds, vp, 32.0, 1.0);
    let params = SceneParams::default();
    let colors = RasterColors {
        palette: (0..12).map(|i| [(i * 20) as u8, 120, 200, 255]).collect(),
        edge: [180, 180, 200, 90],
        nodes: true,
    };
    let region = RasterRegion {
        offset: [0.0, 0.0],
        size: vp,
        scale: a.scale,
    };

    let (mut scene_ms, mut raster_ms) = (Vec::new(), Vec::new());
    let mut checksum = 0.0f32;
    let mut blends = Vec::new();
    for f in 0..a.frames {
        let u = f as f32 / a.frames as f32;
        let cam = graph_canvas_path(u, fit, bounds);
        let t = Instant::now();
        let mut sink = CountingSink::default();
        build_scene(&store, &cam, vp, &params, &mut sink);
        scene_ms.push(t.elapsed().as_secs_f32() * 1000.0);
        checksum += sink.checksum;
        if Lod::for_radius(params.node_radius * cam.zoom, &params) != Lod::Full {
            let t = Instant::now();
            let img = rasterize(&store, &cam, vp, region, &params, &colors);
            raster_ms.push(t.elapsed().as_secs_f32() * 1000.0);
            checksum += img.bgra.len() as f32;
            blends.push((img.blends, img.edges, img.nodes, cam.zoom / fit.zoom));
        }
    }
    println!(
        "scene build ms: p50={:.2} p95={:.2} ({} frames)",
        pct(&mut scene_ms, 0.5),
        pct(&mut scene_ms.clone(), 0.95),
        scene_ms.len()
    );
    println!(
        "rasterize ms:   p50={:.2} p95={:.2} ({} zoomed-out frames, {}x{} logical @{}x)",
        pct(&mut raster_ms, 0.5),
        pct(&mut raster_ms.clone(), 0.95),
        raster_ms.len(),
        a.width,
        a.height,
        a.scale
    );
    if std::env::var_os("RENDER_BENCH_VERBOSE").is_some() {
        for (b, e, n, z) in blends.iter().step_by(8) {
            println!(
                "  zoom x{z:.2}: {e} edges, {n} nodes, {:.1} M blends",
                *b as f64 / 1e6
            );
        }
    }
    if checksum.is_nan() {
        println!("(checksum {checksum})");
    }
}

/// Same path as `graph_canvas::bench::camera_path` (duplicated to keep the CLI free of GPUI).
fn graph_canvas_path(t: f32, fit: Camera, bounds: [f32; 4]) -> Camera {
    let w = bounds[2] - bounds[0];
    let h = bounds[3] - bounds[1];
    let mut cam = fit;
    if t < 0.30 {
        cam.center[0] = fit.center[0] + (t / 0.30 * std::f32::consts::TAU).sin() * w * 0.25;
    } else if t < 0.65 {
        let u = (t - 0.30) / 0.35;
        cam.zoom = fit.zoom * 2f32.powf((u * std::f32::consts::PI).sin() * 3.0);
    } else {
        let a = (t - 0.65) / 0.35 * std::f32::consts::TAU;
        cam.zoom = fit.zoom * 3.0;
        cam.center = [
            fit.center[0] + a.cos() * w * 0.2,
            fit.center[1] + a.sin() * h * 0.2,
        ];
    }
    cam
}
