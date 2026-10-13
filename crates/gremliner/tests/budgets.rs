//! ADR-0027 app-level budgets: cold start and idle memory. They open a real window, so they need a display
//! and the reference machine and are ignored by default. `cargo xtask bench --check` runs the same
//! measurements; to run these directly (release build, window visible):
//!
//! ```sh
//! cargo test --release -p gremliner --test budgets -- --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use providers::{
    ConnectionConfig, ConnectionStore, GremlinServerConfig, PasswordChange, ProviderConfig,
};

fn measure(data_dir: &Path, args: &[&str], name: &str) -> f64 {
    let out = Command::new(env!("CARGO_BIN_EXE_gremliner"))
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--no-tour")
        .args(args)
        .output()
        .expect("run gremliner");
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("measure {name}=")))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("no `measure {name}` line in: {stdout}"))
}

fn data_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gremliner-budget-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore = "needs a display; run explicitly on the reference machine"]
fn scenario_performance_budgets_cold_start_measurement() {
    let dir = data_dir("start");
    let mut runs: Vec<f64> = (0..10)
        .map(|_| measure(&dir, &["--measure-startup"], "first_frame_ms"))
        .collect();
    runs.sort_by(f64::total_cmp);
    let median = runs[runs.len() / 2];
    println!("first frame (ms): {runs:?}, median {median:.1}");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        median <= 300.0,
        "median time to first frame {median:.1} ms exceeds 300 ms"
    );
}

#[test]
#[ignore = "needs a display and the testenv; run explicitly on the reference machine"]
fn scenario_performance_budgets_idle_memory_measurement() {
    let dir = data_dir("idle");
    {
        let storage = storage::Storage::open_in_dir(&dir).unwrap();
        let store =
            ConnectionStore::new(storage, Arc::new(storage::secrets::MemoryStore::default()));
        let config = ConnectionConfig::new(
            "local",
            ProviderConfig::GremlinServer(GremlinServerConfig::default()),
        );
        store.save(config, PasswordChange::Keep).unwrap();
    }
    let mb = measure(
        &dir,
        &[
            "--open-connection",
            "local",
            "--command",
            "workspace::NewQueryTab",
            "--measure-idle-memory",
            "30",
        ],
        "idle_rss",
    );
    println!("idle RSS after 30 s: {mb:.1} MB");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        mb <= 150.0,
        "idle resident memory {mb:.1} MB exceeds 150 MB"
    );
}
