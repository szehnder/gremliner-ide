//! End-to-end tests of the headless CLI binary.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

fn testenv_up() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:8182".parse().unwrap(),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}

#[test]
fn scenario_gremlin_client_cli_json_output() {
    if !testenv_up() {
        eprintln!(
            "SKIPPED: testenv not reachable; run `cargo xtask testenv up && cargo xtask testenv seed`"
        );
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
        .args(["query", "--alias", "gModern", "--json", "g.V().count()"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON on stdout");
    assert_eq!(v["items"], serde_json::json!([6]));
    assert!(v["elapsed_ms"].as_f64().unwrap() > 0.0);
}

#[test]
fn version_prints() {
    let out = Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
        .arg("version")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("gremliner-cli "));
}
