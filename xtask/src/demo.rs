//! `cargo xtask demo`: from a fresh clone to the app showing a real graph in one command. Starts the
//! Docker gremlin-server with the air-routes data set, saves an "Air routes (demo)" connection in the
//! normal data directory (once), and launches the release app connected to it with a first query.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::testenv;

const CONNECTION: &str = "Air routes (demo)";
const URL: &str = "ws://localhost:8182/gremlin";
const QUERY: &str = "g.V().has('airport','code','AUS').outE('route').inV().path().limit(40)";

pub fn run(root: &Path) -> Result<()> {
    testenv::demo(root)?;
    crate::run(
        "build release app",
        Command::new("cargo").args([
            "build",
            "--release",
            "-p",
            "gremliner",
            "-p",
            "gremliner-cli",
        ]),
    )?;
    let cli = root.join("target/release/gremliner-cli");
    let saved = Command::new(&cli)
        .args(["connections", "list", "--json"])
        .output()
        .context("listing saved connections")?;
    let saved: serde_json::Value = serde_json::from_slice(&saved.stdout).unwrap_or_default();
    let exists = saved
        .as_array()
        .is_some_and(|all| all.iter().any(|c| c["name"] == CONNECTION));
    if !exists {
        crate::run(
            "save the demo connection",
            Command::new(&cli).args(["connections", "add", "--name", CONNECTION, "--url", URL]),
        )?;
    }
    println!(
        "\nOpening Gremliner on {CONNECTION}. Stop the server later with `cargo xtask testenv down`."
    );
    crate::run(
        "gremliner",
        Command::new(root.join("target/release/gremliner")).args([
            "--open-connection",
            CONNECTION,
            "--run",
            QUERY,
        ]),
    )
}
