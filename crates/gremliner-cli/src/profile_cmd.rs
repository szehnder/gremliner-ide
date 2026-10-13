//! `gremliner-cli profile|explain`: where a traversal spends its time, or how the database plans it,
//! as a table or JSON. Neptune connections use its profile and explain endpoints.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use gremlin_model::profile::{ProfileReport, StepMetric};
use providers::{ConnectionConfig, GremlinServerConfig, ProviderConfig, endpoint, provider_for};

use crate::connections::open_store;

pub struct Args {
    pub conn: Option<String>,
    pub url: String,
    pub alias: Option<String>,
    pub json: bool,
    /// Profile a statement that changes data (profiling executes it).
    pub yes: bool,
    pub timeout_ms: u64,
    pub script: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Profile,
    Explain,
}

fn connection(a: &Args, data_dir: Option<PathBuf>) -> Result<(ConnectionConfig, Option<String>)> {
    match &a.conn {
        Some(name) => {
            let store = open_store(data_dir)?;
            let config = store
                .find(name)?
                .with_context(|| format!("no connection named {name:?}"))?;
            let password = store.password(&config)?;
            Ok((config, password))
        }
        None => {
            let parsed =
                endpoint::parse(&a.url).with_context(|| format!("not an endpoint: {}", a.url))?;
            let config = ConnectionConfig::new(
                "cli",
                ProviderConfig::GremlinServer(GremlinServerConfig {
                    host: parsed.host,
                    port: parsed.port.unwrap_or(8182),
                    path: parsed.path.unwrap_or_else(|| "/gremlin".into()),
                    tls: parsed.tls.unwrap_or(false),
                    alias: a.alias.clone().unwrap_or_else(|| "g".into()),
                    ..Default::default()
                }),
            );
            Ok((config, None))
        }
    }
}

pub fn run(
    kind: Kind,
    a: Args,
    data_dir: Option<PathBuf>,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    if kind == Kind::Profile && gremlin_syntax::is_mutating(&a.script) && !a.yes {
        eprintln!(
            "Profiling executes the traversal, and this statement changes data. Pass --yes to profile it anyway."
        );
        std::process::exit(2);
    }
    let (config, password) = connection(&a, data_dir)?;
    let timeout = Duration::from_millis(a.timeout_ms);
    runtime.block_on(async {
        let session = provider_for(&config, password)?
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e.reason))?;
        match kind {
            Kind::Profile => {
                let report = providers::analysis::profile(&session, &a.script, timeout)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e.reason))?;
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print!("{}", profile_text(&report));
                }
            }
            Kind::Explain => {
                let report = providers::analysis::explain(&session, &a.script, timeout)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e.reason))?;
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    if let Some(o) = &report.original {
                        println!("Original traversal\n  {o}\n");
                    }
                    if !report.strategies.is_empty() {
                        println!("Strategies applied");
                        for s in &report.strategies {
                            println!("  {:<36} {:<13} {}", s.strategy, s.category, s.traversal);
                        }
                        println!();
                    }
                    if let Some(f) = &report.final_traversal {
                        println!("Final traversal\n  {f}\n");
                    }
                    for s in &report.sections {
                        println!("{}\n{}\n", s.title, s.text);
                    }
                }
            }
        }
        anyhow::Ok(())
    })
}

fn rows(steps: &[StepMetric], depth: usize, out: &mut String) {
    let count = |n: Option<i64>| n.map(|n| n.to_string()).unwrap_or_else(|| "-".into());
    for s in steps {
        let name = format!("{}{}", "  ".repeat(depth), s.name);
        out.push_str(&format!(
            "{name:<60} {:>8} {:>11} {:>11.3} {:>7.2}\n",
            count(s.count),
            count(s.traversers),
            s.dur_ms,
            s.pct
        ));
        rows(&s.children, depth + 1, out);
    }
}

/// The profile as a table, slowest step called out.
pub fn profile_text(r: &ProfileReport) -> String {
    let mut out = format!(
        "{:<60} {:>8} {:>11} {:>11} {:>7}\n",
        "Step", "Count", "Traversers", "Time (ms)", "% Dur"
    );
    rows(&r.steps, 0, &mut out);
    out.push_str(&format!("Total {:.3} ms", r.total_ms));
    if let Some(s) = r.bottleneck().and_then(|i| r.steps.get(i)) {
        out.push_str(&format!(" · slowest: {} ({:.0}%)", s.name, s.pct));
    }
    out.push('\n');
    if let Some(o) = &r.optimized {
        out.push_str(&format!("\nOptimized traversal\n{o}\n"));
    }
    for s in &r.sections {
        out.push_str(&format!("\n{}\n{}\n", s.title, s.text));
    }
    out
}
