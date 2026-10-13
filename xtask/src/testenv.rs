//! Docker test environment (ADR-0026): gremlin-server with air-routes, modern, crew and synthetic graphs.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const HTTP: &str = "http://localhost:8182/gremlin";
const AIR_ROUTES_URL: &str = "https://raw.githubusercontent.com/krlawrence/graph/master/sample-data/air-routes-latest.graphml";
const AIR_ROUTES_SHA256: &str = "497ff2d78cdedb555b4fd3ebe13a0cabe8afc38c86ff9bb32da8b9572ac07a5f";

#[derive(Subcommand)]
pub enum TestenvCmd {
    /// Start the containers and wait until healthy.
    Up,
    /// Stop and remove the containers.
    Down,
    /// Recreate the containers (discards all graph changes), then seed everything.
    Reset,
    /// Show server version and graph sizes.
    Status,
    /// Load data sets into the running server.
    Seed {
        #[arg(long, value_enum, default_value = "all")]
        dataset: Dataset,
        /// Synthetic graph: number of nodes.
        #[arg(long, default_value_t = 10_000)]
        nodes: usize,
        /// Synthetic graph: average out-degree.
        #[arg(long, default_value_t = 2.0)]
        avg_degree: f64,
        /// Synthetic graph: number of vertex labels.
        #[arg(long, default_value_t = 5)]
        labels: usize,
        /// Synthetic graph: RNG seed (same seed → identical graph).
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Reload even if the graph already has data.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum Dataset {
    All,
    AirRoutes,
    Synthetic,
}

pub fn run(root: &Path, cmd: TestenvCmd) -> Result<()> {
    match cmd {
        TestenvCmd::Up => up(root),
        TestenvCmd::Down => compose(root, &["down", "--remove-orphans"]),
        TestenvCmd::Reset => {
            compose(root, &["down", "--remove-orphans"])?;
            up(root)?;
            seed(root, Dataset::All, &SynthOpts::default(), true)?;
            status()
        }
        TestenvCmd::Status => status(),
        TestenvCmd::Seed {
            dataset,
            nodes,
            avg_degree,
            labels,
            seed: s,
            force,
        } => {
            let opts = SynthOpts {
                nodes,
                avg_degree,
                labels,
                seed: s,
            };
            seed(root, dataset, &opts, force)?;
            status()
        }
    }
}

/// What `cargo xtask demo` needs: the server up and air-routes loaded (skipped when already loaded).
pub fn demo(root: &Path) -> Result<()> {
    up(root)?;
    seed(root, Dataset::AirRoutes, &SynthOpts::default(), false)
}

fn compose(root: &Path, args: &[&str]) -> Result<()> {
    let file = root.join("testenv/docker-compose.yml");
    let mut cmd = Command::new("docker");
    cmd.arg("compose").arg("-f").arg(file).args(args);
    crate::run(&format!("docker compose {}", args.join(" ")), &mut cmd)
}

fn up(root: &Path) -> Result<()> {
    compose(root, &["up", "-d", "--wait"])?;
    wait_ready(Duration::from_secs(60))
}

fn wait_ready(timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if eval("1").is_ok() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("gremlin-server did not become ready at {HTTP} within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Evaluate a Groovy/Gremlin script through the HTTP endpoint and return `result.data`.
pub fn eval(script: &str) -> Result<Value> {
    let mut resp = ureq::post(HTTP)
        .config()
        .timeout_global(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .send_json(json!({ "gremlin": script }))
        .with_context(|| format!("POST {HTTP}"))?;
    let status = resp.status();
    let body: Value = resp
        .body_mut()
        .read_json()
        .context("decoding gremlin-server response")?;
    if !status.is_success() {
        bail!(
            "gremlin-server error ({status}): {}",
            body["message"].as_str().unwrap_or(&body.to_string())
        );
    }
    Ok(body["result"]["data"].clone())
}

/// Unwrap a GraphSON v3 `g:List` of one `g:Int64`/`g:Int32` into an integer.
fn eval_count(script: &str) -> Result<i64> {
    let data = eval(script)?;
    data["@value"][0]["@value"]
        .as_i64()
        .or_else(|| data["@value"][0].as_i64())
        .with_context(|| format!("unexpected count result for `{script}`: {data}"))
}

fn status() -> Result<()> {
    let version = eval("Gremlin.version()")?;
    let version = version["@value"][0].as_str().unwrap_or("?").to_string();
    println!("gremlin-server {version} at {HTTP}");
    for (alias, name) in [
        ("g", "air-routes"),
        ("gModern", "modern"),
        ("gCrew", "crew"),
        ("gSynth", "synthetic"),
    ] {
        let v = eval_count(&format!("{alias}.V().count()"))?;
        let e = eval_count(&format!("{alias}.E().count()"))?;
        println!("  {alias:<8} {name:<11} {v:>7} vertices {e:>8} edges");
    }
    match auth_server_reachable() {
        true => {
            println!("  auth server on :8183 (user `stephen`, password `password`) is reachable")
        }
        false => println!("  auth server on :8183 is NOT reachable"),
    }
    Ok(())
}

fn auth_server_reachable() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:8183".parse().expect("valid addr"),
        Duration::from_secs(1),
    )
    .is_ok()
}

#[derive(Clone)]
struct SynthOpts {
    nodes: usize,
    avg_degree: f64,
    labels: usize,
    seed: u64,
}

impl Default for SynthOpts {
    fn default() -> Self {
        Self {
            nodes: 10_000,
            avg_degree: 2.0,
            labels: 5,
            seed: 42,
        }
    }
}

fn seed(root: &Path, dataset: Dataset, opts: &SynthOpts, force: bool) -> Result<()> {
    wait_ready(Duration::from_secs(30))?;
    let data_dir = root.join("testenv/data");
    if matches!(dataset, Dataset::All | Dataset::AirRoutes) {
        let file = ensure_air_routes(&data_dir)?;
        load_graphml("graph", "g", &file, force)?;
    }
    if matches!(dataset, Dataset::All | Dataset::Synthetic) {
        let file = data_dir.join(format!("synthetic-{}-{}.graphml", opts.nodes, opts.seed));
        if !file.exists() || force {
            std::fs::write(&file, synthetic_graphml(opts))?;
            println!("generated {}", file.display());
        }
        load_graphml("synth", "gSynth", &file, force)?;
    }
    Ok(())
}

fn load_graphml(graph: &str, alias: &str, file: &Path, force: bool) -> Result<()> {
    let existing = eval_count(&format!("{alias}.V().count()"))?;
    if existing > 0 && !force {
        println!("{alias}: already has {existing} vertices (use --force to reload)");
        return Ok(());
    }
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .context("file name")?;
    let start = Instant::now();
    eval(&format!(
        "{alias}.V().drop().iterate(); {graph}.io(IoCore.graphml()).readGraph('/data/{name}'); {alias}.V().count()"
    ))?;
    println!(
        "{alias}: loaded {name} in {:.1}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn ensure_air_routes(data_dir: &Path) -> Result<PathBuf> {
    let file = data_dir.join("air-routes.graphml");
    if file.exists() && sha256_file(&file)? == AIR_ROUTES_SHA256 {
        return Ok(file);
    }
    println!("downloading air-routes from {AIR_ROUTES_URL}");
    let bytes = ureq::get(AIR_ROUTES_URL)
        .call()
        .with_context(|| format!("downloading {AIR_ROUTES_URL}"))?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()?;
    let digest = hex(&Sha256::digest(&bytes));
    if digest != AIR_ROUTES_SHA256 {
        bail!(
            "air-routes checksum mismatch: expected {AIR_ROUTES_SHA256}, got {digest}. \
             The upstream file changed; review it and update AIR_ROUTES_SHA256 in xtask/src/testenv.rs \
             (expected counts in tests may change too)."
        );
    }
    std::fs::write(&file, bytes)?;
    Ok(file)
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(hex(&Sha256::digest(std::fs::read(path)?)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// SplitMix64: tiny, deterministic, good enough for test data.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Deterministic synthetic graph with a preferential-attachment flavour (some hubs, many leaves).
fn synthetic_graphml(o: &SynthOpts) -> String {
    let mut rng = Rng(o.seed);
    let mut s = String::with_capacity(o.nodes * 200);
    s.push_str(concat!(
        "<?xml version='1.0' ?>\n",
        "<graphml xmlns='http://graphml.graphdrawing.org/xmlns'>\n",
        "<key id='labelV' for='node' attr.name='labelV' attr.type='string'/>\n",
        "<key id='name' for='node' attr.name='name' attr.type='string'/>\n",
        "<key id='score' for='node' attr.name='score' attr.type='int'/>\n",
        "<key id='labelE' for='edge' attr.name='labelE' attr.type='string'/>\n",
        "<key id='weight' for='edge' attr.name='weight' attr.type='double'/>\n",
        "<graph id='synthetic' edgedefault='directed'>\n"
    ));
    let labels = o.labels.max(1);
    for i in 0..o.nodes {
        let _ = writeln!(
            s,
            "<node id='{i}'><data key='labelV'>L{}</data><data key='name'>n{i}</data><data key='score'>{}</data></node>",
            i % labels,
            rng.below(1000)
        );
    }
    let edges = (o.nodes as f64 * o.avg_degree).round() as usize;
    for e in 0..edges {
        let from = rng.below(o.nodes.max(1));
        // Square the uniform sample to bias targets toward low ids → hubs.
        let to = ((rng.unit().powi(2)) * o.nodes as f64) as usize % o.nodes.max(1);
        let _ = writeln!(
            s,
            "<edge id='{e}' source='{from}' target='{to}'><data key='labelE'>link</data><data key='weight'>{:.3}</data></edge>",
            rng.unit()
        );
    }
    s.push_str("</graph>\n</graphml>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_is_deterministic() {
        let o = SynthOpts {
            nodes: 500,
            avg_degree: 2.0,
            labels: 3,
            seed: 7,
        };
        assert_eq!(synthetic_graphml(&o), synthetic_graphml(&o));
        let other = SynthOpts {
            seed: 8,
            ..o.clone()
        };
        assert_ne!(synthetic_graphml(&o), synthetic_graphml(&other));
    }

    #[test]
    fn synthetic_counts() {
        let o = SynthOpts {
            nodes: 100,
            avg_degree: 3.0,
            labels: 2,
            seed: 1,
        };
        let g = synthetic_graphml(&o);
        assert_eq!(g.matches("<node ").count(), 100);
        assert_eq!(g.matches("<edge ").count(), 300);
    }
}
