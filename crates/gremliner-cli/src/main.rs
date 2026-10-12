//! Headless Gremliner CLI. Every non-UI capability is exposed here so agents can verify behaviour
//! without the GUI (ADR-0024). Subcommands are added by the OpenSpec change that implements them.

mod connections;
mod lang;
mod layout_bench;
mod llm_cmd;
mod query;
mod render_bench;
mod schema_cmd;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "gremliner-cli",
    version,
    about = "Headless Gremliner IDE tools"
)]
struct Cli {
    /// App data directory (default: the platform data directory, shared with the app).
    #[arg(long, global = true, env = "GREMLINER_DATA_DIR")]
    data_dir: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the version.
    Version,
    /// Report syntax errors and unknown steps; exits 1 on errors.
    Lint {
        /// File to lint (`-` or omitted: stdin).
        file: Option<std::path::PathBuf>,
        /// Treat warnings as failures.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Format Gremlin (Gremlint style).
    Fmt {
        file: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = gremlin_syntax::format::DEFAULT_WIDTH)]
        width: usize,
        /// Rewrite the file in place.
        #[arg(long)]
        write: bool,
        /// Exit 1 if the file is not already formatted.
        #[arg(long)]
        check: bool,
    },
    /// Print the syntax tree, or with --json the statements, steps, mutation flags and parameters.
    Parse {
        file: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Manage saved connections (shared with the app).
    Connections {
        #[command(subcommand)]
        cmd: connections::Cmd,
    },
    /// Manage and test LLM profiles (shared with the app).
    Llm {
        #[command(subcommand)]
        cmd: llm_cmd::Cmd,
    },
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
    /// Discover and print a graph's schema (labels, typed properties, counts, connections).
    Schema {
        /// A saved connection (its cached schema is updated too).
        #[arg(long)]
        conn: Option<String>,
        /// Server WebSocket URL, when no `--conn` is given.
        #[arg(
            long,
            env = "GREMLINER_URL",
            default_value = "ws://localhost:8182/gremlin"
        )]
        url: String,
        /// Server-side traversal source to bind to `g`, when no `--conn` is given.
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        json: bool,
        /// Elements read per sampling query.
        #[arg(long, default_value_t = 10_000)]
        sample: u32,
        /// Timeout per discovery query in milliseconds.
        #[arg(long, default_value_t = 5_000)]
        timeout_ms: u64,
    },
    /// Time a graph layout on a GraphML dataset (default: the testenv's air-routes).
    LayoutBench {
        /// A testenv dataset name (`air-routes`, `synthetic-10000-42`) or a GraphML path.
        #[arg(long, default_value = "air-routes")]
        dataset: String,
        /// Layout id: fa2, hierarchical-td, hierarchical-lr, radial, circular, grid, concentric.
        #[arg(long, default_value = "fa2")]
        layout: String,
        #[arg(long, default_value_t = 42)]
        seed: u64,
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
    let cli = Cli::parse();
    match cli.command {
        Command::Connections { cmd } => connections::run(cmd, cli.data_dir, &runtime)?,
        Command::Llm { cmd } => llm_cmd::run(cmd, cli.data_dir, &runtime)?,
        Command::LayoutBench {
            dataset,
            layout,
            seed,
        } => layout_bench::run(&layout_bench::Args {
            dataset,
            layout,
            seed,
        })?,
        Command::Lint {
            file,
            deny_warnings,
        } => {
            std::process::exit(lang::lint(file.as_ref(), deny_warnings)?);
        }
        Command::Fmt {
            file,
            width,
            write,
            check,
        } => std::process::exit(lang::fmt(file.as_ref(), width, write, check)?),
        Command::Parse { file, json } => std::process::exit(lang::parse(file.as_ref(), json)?),
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
        Command::Schema {
            conn,
            url,
            alias,
            json,
            sample,
            timeout_ms,
        } => schema_cmd::run(
            schema_cmd::Args {
                conn,
                url,
                alias,
                json,
                sample,
                timeout_ms,
            },
            cli.data_dir,
            &runtime,
        )?,
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
