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

fn cli(data: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .env("GREMLINER_SECRET_STORE", "memory")
        .output()
        .unwrap()
}

#[test]
fn scenario_connections_cli_test() {
    let data = std::env::temp_dir().join(format!("gremliner-cli-conn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let added = cli(
        &data,
        &[
            "connections",
            "add",
            "--name",
            "Local",
            "--url",
            "ws://localhost:8182/gremlin",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let dup = cli(
        &data,
        &[
            "connections",
            "add",
            "--name",
            "local",
            "--url",
            "localhost:8182",
        ],
    );
    assert!(!dup.status.success(), "duplicate names are rejected");
    let list = cli(&data, &["connections", "list"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("localhost:8182"));

    // Failure path works without the testenv: nothing listens on port 1.
    let dead = cli(
        &data,
        &[
            "connections",
            "add",
            "--name",
            "Dead",
            "--url",
            "127.0.0.1:1",
        ],
    );
    assert!(dead.status.success());
    let failed = cli(&data, &["connections", "test", "Dead"]);
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("Nothing is listening on 127.0.0.1:1.")
    );

    if testenv_up() {
        let ok = cli(&data, &["connections", "test", "Local"]);
        let stdout = String::from_utf8_lossy(&ok.stdout);
        assert!(
            ok.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&ok.stderr)
        );
        assert!(stdout.starts_with("OK: connected to Local in "), "{stdout}");
        assert!(stdout.contains(" ms"));
    } else {
        eprintln!("SKIPPED success path: testenv not reachable");
    }
    let removed = cli(&data, &["connections", "remove", "Local"]);
    assert!(removed.status.success());
    let _ = std::fs::remove_dir_all(&data);
}
