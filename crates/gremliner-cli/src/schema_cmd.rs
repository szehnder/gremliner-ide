//! `gremliner-cli schema`: discover a graph's schema and print it as text or JSON. With `--conn` the
//! result also replaces that connection's cached schema, which the app shows on its next connect.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use providers::schema::{discover_schema, save_cached};
use providers::{ConnectionConfig, GremlinServerConfig, ProviderConfig, endpoint, provider_for};

use crate::connections::open_store;

pub struct Args {
    pub conn: Option<String>,
    pub url: String,
    pub alias: Option<String>,
    pub json: bool,
    pub sample: u32,
    pub timeout_ms: u64,
}

pub fn run(a: Args, data_dir: Option<PathBuf>, runtime: &tokio::runtime::Runtime) -> Result<()> {
    let (config, password, store) = match &a.conn {
        Some(name) => {
            let store = open_store(data_dir)?;
            let config = store
                .find(name)?
                .with_context(|| format!("no connection named {name:?}"))?;
            let password = store.password(&config)?;
            (config, password, Some(store))
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
            (config, None, None)
        }
    };
    let opts = schema::Options {
        sample: a.sample,
        timeout: Duration::from_millis(a.timeout_ms),
        ..Default::default()
    };
    let schema = runtime.block_on(async {
        let session = provider_for(&config, password)?
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e.reason))?;
        let schema = discover_schema(&session, &opts, &|_| {}).await;
        if let Some(store) = &store
            && let Err(err) = save_cached(store.storage(), config.id, &schema).await
        {
            eprintln!("warning: could not cache the schema: {err}");
        }
        anyhow::Ok(schema)
    })?;
    if a.json {
        println!("{}", schema.to_json_pretty());
    } else {
        print!("{}", schema.render_text());
    }
    Ok(())
}
