//! Headless Gremliner CLI. Every non-UI capability is exposed here so agents can verify behaviour
//! without the GUI (ADR-0024). Subcommands are added by the OpenSpec change that implements them.

mod query;
mod render_bench;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "gremliner-cli",
    version,
    about = "Headless Gremliner IDE tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the version.
    Version,
    /// Run a Gremlin script against a server and print the results.
    Query {
        /// Server WebSocket URL.
        #[arg(
            long,
            env = "GREMLINER_URL",
            default_value = "ws://localhost:8182/gremlin"
        )]
        url: String,
        /// Server-side traversal source to bind to `g` (e.g. gModern).
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long, env = "GREMLINER_PASSWORD", hide_env_values = true)]
        password: Option<String>,
        /// Request timeout in milliseconds.
        #[arg(long, default_value_t = 30_000)]
        timeout_ms: u64,
        /// Print machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// The Gremlin script.
        script: String,
    },
    /// Headless CPU cost of graph frames (scene build + zoomed-out rasteriser) on a synthetic graph.
    RenderBench {
        #[arg(long, default_value_t = 10_000)]
        nodes: usize,
        #[arg(long, default_value_t = 20_000)]
        edges: usize,
        #[arg(long, default_value_t = 120)]
        frames: usize,
        #[arg(long, default_value_t = 1600.0)]
        width: f32,
        #[arg(long, default_value_t = 1000.0)]
        height: f32,
        /// Device pixels per logical pixel.
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
    },
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    match Cli::parse().command {
        Command::Query {
            url,
            alias,
            user,
            password,
            timeout_ms,
            json,
            script,
        } => {
            runtime.block_on(query::run(query::Args {
                url,
                alias,
                user,
                password,
                timeout_ms,
                json,
                script,
            }))?;
        }
        Command::Version => println!("gremliner-cli {}", env!("CARGO_PKG_VERSION")),
        Command::RenderBench {
            nodes,
            edges,
            frames,
            width,
            height,
            scale,
        } => {
            render_bench::run(&render_bench::Args {
                nodes,
                edges,
                frames,
                width,
                height,
                scale,
            });
        }
    }
    Ok(())
}
