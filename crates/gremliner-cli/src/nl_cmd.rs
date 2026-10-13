//! `gremliner-cli nl` (one prompt through the verified pipeline) and `gremliner-cli eval` (the golden set).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use assistant::{Context, Dialect, DryRun, Options, Outcome, PipelineEvent, Shape};
use gremlin_model::plain::to_json;
use llm::{LlmClient, ProfileStore};
use providers::schema::{discover_schema, load_cached, save_cached};
use providers::{ConnectionConfig, Session, provider_for};
use serde::Deserialize;
use storage::Storage;

use crate::connections::open_store;

pub struct NlArgs {
    pub conn: String,
    pub profile: Option<String>,
    pub json: bool,
    pub no_dry_run: bool,
    pub prompt: String,
}

struct Setup {
    session: Session,
    ctx: Context,
    client: LlmClient,
}

fn profiles(data_dir: Option<PathBuf>) -> Result<ProfileStore> {
    let dir = data_dir
        .or_else(Storage::default_data_dir)
        .context("no data directory; pass --data-dir")?;
    Ok(ProfileStore::new(
        Storage::open_in_dir(&dir)?,
        storage::secrets::detect(),
    ))
}

async fn setup(
    config: &ConnectionConfig,
    password: Option<String>,
    storage: &Storage,
    profiles: &ProfileStore,
    profile_name: Option<&str>,
) -> Result<Setup> {
    let profile = match profile_name {
        Some(name) => profiles
            .find(name)?
            .with_context(|| format!("no LLM profile named {name:?}"))?,
        None => profiles
            .resolve(Some(config.id))?
            .context("no LLM profile; add one with `gremliner-cli llm add`")?,
    };
    let key = profiles.key(&profile)?;
    let share_samples = profiles.share_samples(config.id, &profile)?;
    let client = LlmClient::new(profile, key)?;
    let session = provider_for(config, password)?
        .connect()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e.reason))?;
    let schema = match load_cached(storage, config.id).await {
        Some(s) => s,
        None => {
            let s = discover_schema(&session, &schema::Options::default(), &|_| {}).await;
            let _ = save_cached(storage, config.id, &s).await;
            s
        }
    };
    let ctx = Context {
        schema: Some(Arc::new(schema)),
        dialect: Dialect {
            tinkerpop_version: session.info.tinkerpop_version.clone(),
            neptune: config.provider.kind().is_neptune(),
        },
        current_query: None,
        share_samples,
    };
    Ok(Setup {
        session,
        ctx,
        client,
    })
}

fn badge_line(o: &Outcome) -> String {
    let mark = |ok: bool| if ok { "\u{2713}" } else { "\u{2717}" };
    let schema = match o.badges.schema {
        Some(ok) => format!("Schema {}", mark(ok)),
        None => "Schema ?".into(),
    };
    let dry = match &o.badges.dry_run {
        DryRun::Passed { rows, elapsed } => {
            format!(
                "Dry run \u{2713} {rows} rows \u{b7} {} ms",
                elapsed.as_millis()
            )
        }
        DryRun::Failed { message } => format!("Dry run \u{2717} {message}"),
        DryRun::Skipped { reason } => format!("Dry run skipped ({reason})"),
    };
    format!(
        "Syntax {} \u{b7} {schema} \u{b7} {dry}",
        mark(o.badges.syntax)
    )
}

pub fn nl(a: NlArgs, data_dir: Option<PathBuf>, runtime: &tokio::runtime::Runtime) -> Result<()> {
    let store = open_store(data_dir.clone())?;
    let config = store
        .find(&a.conn)?
        .with_context(|| format!("no connection named {:?}", a.conn))?;
    let password = store.password(&config)?;
    let profiles = profiles(data_dir)?;
    let json = a.json;
    let outcome = runtime.block_on(async {
        let s = setup(
            &config,
            password,
            store.storage(),
            &profiles,
            a.profile.as_deref(),
        )
        .await?;
        let opts = Options {
            dry_run: !a.no_dry_run,
            ..Options::default()
        };
        let outcome = assistant::run(
            &a.prompt,
            &[],
            &s.ctx,
            &s.client,
            Some(&s.session.client),
            &opts,
            &|e| {
                if json {
                    return;
                }
                match e {
                    PipelineEvent::Stage(stage) => eprintln!("{}", stage.label()),
                    PipelineEvent::Failed(msg) => eprintln!("error: {msg}"),
                    _ => {}
                }
            },
        )
        .await;
        anyhow::Ok(outcome)
    })?;
    let Some(o) = outcome else {
        bail!("the model produced no usable answer");
    };
    if json {
        let preview: Vec<_> = o.preview.iter().map(to_json).collect();
        let mut v = serde_json::to_value(&o)?;
        v["preview"] = serde_json::Value::Array(preview);
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("{}", o.answer.query);
        if !o.answer.explanation.is_empty() {
            println!("\n{}", o.answer.explanation);
        }
        println!("\n{}", badge_line(&o));
        if o.writes {
            println!("Writes data: not run. Review it before running it yourself.");
        }
        for issue in &o.issues {
            println!("problem: {}", issue.message);
        }
        if !o.preview.is_empty() {
            println!();
            for item in o.preview.iter().take(10) {
                println!("==>{}", to_json(item));
            }
        }
    }
    if !o.verified {
        std::process::exit(2);
    }
    Ok(())
}

// ---- golden set -----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Golden {
    pub case: Vec<Case>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub prompt: String,
    /// Server-side traversal source (`g` = air-routes, `gModern`).
    #[serde(default = "default_alias")]
    pub alias: String,
    /// Labels the query should use.
    #[serde(default)]
    pub labels: Vec<String>,
}

fn default_alias() -> String {
    "g".into()
}

pub const GOLDEN: &str = include_str!("../../assistant/eval/golden.toml");

pub fn golden() -> Result<Golden> {
    Ok(toml::from_str(GOLDEN)?)
}

pub struct EvalArgs {
    pub profile: String,
    pub url: String,
    pub min: f64,
    pub json: bool,
}

/// Run every golden case; pass = verified, non-empty dry run (or a scalar), and the expected labels used.
pub fn eval(
    a: EvalArgs,
    data_dir: Option<PathBuf>,
    runtime: &tokio::runtime::Runtime,
) -> Result<()> {
    let set = golden()?;
    let profiles = profiles(data_dir)?;
    let storage = Storage::open_in_memory()?;
    let mut results = Vec::new();
    for (i, case) in set.case.iter().enumerate() {
        let parsed = providers::endpoint::parse(&a.url).context("bad --url")?;
        let config = ConnectionConfig::new(
            format!("eval-{}", case.alias),
            providers::ProviderConfig::GremlinServer(providers::GremlinServerConfig {
                host: parsed.host,
                port: parsed.port.unwrap_or(8182),
                alias: case.alias.clone(),
                ..Default::default()
            }),
        );
        let started = std::time::Instant::now();
        let outcome = runtime.block_on(async {
            let s = setup(&config, None, &storage, &profiles, Some(&a.profile)).await?;
            anyhow::Ok(
                assistant::run(
                    &case.prompt,
                    &[],
                    &s.ctx,
                    &s.client,
                    Some(&s.session.client),
                    &Options::default(),
                    &|_| {},
                )
                .await,
            )
        })?;
        let (pass, query, why) = match &outcome {
            None => (false, String::new(), "no answer".to_owned()),
            Some(o) => {
                let rows = matches!(o.badges.dry_run, DryRun::Passed { rows, .. } if rows > 0)
                    || (o.answer.expected_shape == Shape::Scalar
                        && matches!(o.badges.dry_run, DryRun::Passed { .. }));
                let labels_ok = case
                    .labels
                    .iter()
                    .all(|l| o.answer.query.contains(&format!("'{l}'")));
                let why = if !o.verified {
                    o.issues
                        .first()
                        .map(|i| i.message.clone())
                        .unwrap_or_else(|| "not verified".into())
                } else if !rows {
                    "no rows".into()
                } else if !labels_ok {
                    format!("expected labels {:?}", case.labels)
                } else {
                    String::new()
                };
                (o.verified && rows && labels_ok, o.answer.query.clone(), why)
            }
        };
        if !a.json {
            println!(
                "{:>2} {} {:<60} {:>5} ms  {}",
                i + 1,
                if pass { "PASS" } else { "FAIL" },
                case.prompt,
                started.elapsed().as_millis(),
                if pass { query.clone() } else { why.clone() }
            );
        }
        results.push(serde_json::json!({"prompt": case.prompt, "pass": pass, "query": query, "problem": why}));
    }
    let passed = results.iter().filter(|r| r["pass"] == true).count();
    #[allow(clippy::cast_precision_loss)]
    let rate = passed as f64 / results.len().max(1) as f64;
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"pass_rate": rate, "results": results})
            )?
        );
    } else {
        println!("\n{passed}/{} passed ({:.0}%)", results.len(), rate * 100.0);
    }
    if rate < a.min {
        std::process::exit(1);
    }
    Ok(())
}
