//! ADR-0006 benchmark gate. These open a real window and need a display and the reference machine, so
//! they are ignored by default. Run them explicitly (release build, window visible and focused):
//!
//! ```sh
//! cargo test --release -p gremliner --test render_gate -- --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

fn p95_interval(nodes: usize, edges: usize) -> f32 {
    let out = Command::new(env!("CARGO_BIN_EXE_gremliner"))
        .args([
            "--render-bench",
            &format!("{nodes},{edges}"),
            "--bench-seconds",
            "8",
        ])
        .output()
        .expect("run gremliner --render-bench");
    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("{stdout}");
    let line = stdout
        .lines()
        .find(|l| l.contains("frame interval ms"))
        .expect("benchmark output");
    let p95 = line
        .split("p95=")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .expect("p95 field");
    p95.parse().expect("p95 number")
}

#[test]
#[ignore = "needs a display; run explicitly on the reference machine"]
fn scenario_graph_rendering_medium_graph_benchmark() {
    let p95 = p95_interval(10_000, 20_000);
    assert!(
        p95 <= 8.3,
        "p95 frame interval {p95} ms exceeds 8.3 ms (120 fps)"
    );
}

#[test]
#[ignore = "needs a display; run explicitly on the reference machine"]
fn scenario_graph_rendering_large_graph_benchmark() {
    let p95 = p95_interval(50_000, 100_000);
    assert!(
        p95 <= 16.7,
        "p95 frame interval {p95} ms exceeds 16.7 ms (60 fps)"
    );
}
