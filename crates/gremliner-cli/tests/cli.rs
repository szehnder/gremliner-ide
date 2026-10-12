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

#[test]
fn scenario_gremlin_language_cli_lint_exit_code() {
    let corpus = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../gremlin-syntax/tests/corpus"
    );
    let bad = Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
        .args(["lint", &format!("{corpus}/invalid/unclosed.groovy")])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&bad.stdout);
    assert!(
        stdout.contains("unclosed.groovy:1:26: error: Missing closing parenthesis"),
        "{stdout}"
    );
    let good = Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
        .args(["lint", &format!("{corpus}/valid/modern.groovy")])
        .output()
        .unwrap();
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stdout)
    );
}

#[test]
fn fmt_and_parse_work_on_stdin() {
    use std::io::Write as _;
    let run = |args: &[&str], input: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_gremliner-cli"))
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let out = run(&["fmt"], "g . V( ).out( 'x' )");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "g.V().out('x')\n");
    let out = run(&["parse", "--json"], "g.V().has('code', code).drop()");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["statements"][0]["mutating"], true);
    assert_eq!(
        v["statements"][0]["parameters"],
        serde_json::json!(["code"])
    );
}

#[test]
fn scenario_schema_discovery_cli_schema_json() {
    if !testenv_up() {
        eprintln!("SKIPPED: testenv not reachable");
        return;
    }
    let data = std::env::temp_dir().join(format!("gremliner-cli-schema-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let added = cli(
        &data,
        &[
            "connections",
            "add",
            "--name",
            "Modern",
            "--url",
            "ws://localhost:8182/gremlin",
            "--alias",
            "gModern",
        ],
    );
    assert!(added.status.success());
    let out = cli(&data, &["schema", "--conn", "Modern", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let names = |key: &str| -> Vec<String> {
        v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(names("vertex_labels"), ["person", "software"]);
    assert_eq!(names("edge_labels"), ["created", "knows"]);
    let person = &v["vertex_labels"][0];
    assert!(
        person["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["key"] == "age" && p["types"] == serde_json::json!(["Int32"]))
    );
    assert!(
        v["triples"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!({"out": "person", "label": "knows", "in": "person"}))
    );
    // The text form works too.
    let text = cli(&data, &["schema", "--conn", "Modern"]);
    assert!(String::from_utf8_lossy(&text.stdout).contains("(person)-[knows]->(person)"));
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn scenario_llm_providers_cli_llm_test() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let fixture = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../llm/tests/fixtures/openai-text.sse"
    ))
    .unwrap();
    let server = runtime.block_on(llm::testing::MockServer::start(vec![
        llm::testing::Route::sse("/v1/chat/completions", &fixture),
    ]));
    let data = std::env::temp_dir().join(format!("gremliner-cli-llm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let base = format!("{}/v1", server.base);
    let added = cli(
        &data,
        &[
            "llm",
            "add",
            "--name",
            "Local",
            "--preset",
            "ollama",
            "--model",
            "qwen2.5-coder",
            "--base-url",
            &base,
            "--default",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let list = cli(&data, &["llm", "list"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("(default)"));
    let out = cli(&data, &["llm", "test", "--profile", "Local"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.starts_with("Hello! Graph databases model networks."),
        "{stdout}"
    );
    assert!(stdout.contains("First token "), "{stdout}");
    assert!(stdout.contains("total "), "{stdout}");
    // A failing profile exits non-zero.
    let bad = cli(
        &data,
        &[
            "llm",
            "add",
            "--name",
            "Dead",
            "--preset",
            "custom",
            "--model",
            "m",
            "--base-url",
            "http://127.0.0.1:1/v1",
        ],
    );
    assert!(bad.status.success());
    let failed = cli(&data, &["llm", "test", "--profile", "Dead"]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).starts_with("FAILED: "));
    let _ = std::fs::remove_dir_all(&data);
}
