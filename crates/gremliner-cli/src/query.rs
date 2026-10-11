//! `gremliner-cli query`: run a Gremlin script against a server and print the results.

use std::time::Duration;

use gremlin_client::{Client, ClientConfig, Request};
use gremlin_model::plain::to_json;

pub struct Args {
    pub url: String,
    pub alias: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub timeout_ms: u64,
    pub json: bool,
    pub script: String,
}

pub async fn run(a: Args) -> anyhow::Result<()> {
    let mut cfg = ClientConfig::new(&a.url);
    cfg.pool_size = 1;
    if let Some(alias) = &a.alias {
        cfg = cfg.alias(alias);
    }
    if let (Some(u), Some(p)) = (&a.user, &a.password) {
        cfg = cfg.credentials(u, p);
    }
    let client = Client::connect(cfg).await?;
    let result = client
        .execute_request(Request::new(&a.script).timeout(Duration::from_millis(a.timeout_ms)))
        .await?;
    let elapsed_ms = result.elapsed.as_secs_f64() * 1000.0;
    if a.json {
        let out = serde_json::json!({
            "items": result.items.iter().map(to_json).collect::<Vec<_>>(),
            "count": result.items.len(),
            "elapsed_ms": elapsed_ms,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        for item in &result.items {
            println!("==>{}", to_json(item));
        }
        eprintln!("{} item(s) in {elapsed_ms:.1} ms", result.items.len());
    }
    Ok(())
}
