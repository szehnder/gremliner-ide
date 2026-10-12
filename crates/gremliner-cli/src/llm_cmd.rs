//! `gremliner-cli llm list|add|models|test|remove`: the app's LLM profiles from the command line.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use llm::{KeyChange, LlmClient, PRESETS, ProfileStore, preset};
use storage::Storage;

#[derive(Subcommand)]
pub enum Cmd {
    /// List saved profiles.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Save a profile from a preset (anthropic, gemini, openai, ollama, lmstudio, openrouter, custom).
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        preset: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        /// Read the API key from stdin (never pass it as an argument).
        #[arg(long)]
        key_stdin: bool,
        /// Make it the default profile.
        #[arg(long)]
        default: bool,
    },
    /// List the provider's models for a profile.
    Models {
        #[arg(long)]
        profile: String,
    },
    /// Stream a short reply and print timings; exits 1 on failure.
    Test {
        #[arg(long)]
        profile: String,
    },
    /// Delete a profile and its stored key.
    Remove { name: String },
}

fn open(data_dir: Option<PathBuf>) -> Result<ProfileStore> {
    let dir = data_dir
        .or_else(Storage::default_data_dir)
        .context("no data directory; pass --data-dir")?;
    Ok(ProfileStore::new(
        Storage::open_in_dir(&dir)?,
        storage::secrets::detect(),
    ))
}

fn client(store: &ProfileStore, name: &str) -> Result<LlmClient> {
    let profile = store
        .find(name)?
        .with_context(|| format!("no LLM profile named {name:?}"))?;
    let key = store.key(&profile)?;
    Ok(LlmClient::new(profile, key)?)
}

pub fn run(cmd: Cmd, data_dir: Option<PathBuf>, runtime: &tokio::runtime::Runtime) -> Result<()> {
    let store = open(data_dir)?;
    match cmd {
        Cmd::List { json } => {
            let all = store.list()?;
            let default = store.default_id()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else if all.is_empty() {
                println!("No LLM profiles.");
            } else {
                for p in all {
                    let mark = if Some(p.id) == default {
                        " (default)"
                    } else {
                        ""
                    };
                    println!(
                        "{:<20} {:<18} {:<28} {}{mark}",
                        p.name,
                        p.kind.label(),
                        p.model,
                        p.base()
                    );
                }
            }
        }
        Cmd::Add {
            name,
            preset: id,
            model,
            base_url,
            key_stdin,
            default,
        } => {
            let Some(preset) = preset(&id) else {
                let ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
                bail!("unknown preset {id:?}; one of {}", ids.join(", "));
            };
            let mut profile = preset.profile(name);
            if let Some(m) = model {
                profile.model = m;
            }
            if let Some(b) = base_url {
                profile.base_url = b;
            }
            let key = if key_stdin {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                KeyChange::Set(buf.trim().to_owned())
            } else {
                KeyChange::Keep
            };
            let saved = store.save(profile, key)?;
            if default {
                store.set_default(Some(saved.id))?;
            }
            if key_stdin && !store.secrets_persist() {
                eprintln!("warning: no credential store; the key was not saved");
            }
            println!(
                "Saved {} ({} {})",
                saved.name,
                saved.kind.label(),
                saved.base()
            );
        }
        Cmd::Models { profile } => {
            let client = client(&store, &profile)?;
            match runtime.block_on(client.list_models()) {
                Ok(models) => models.iter().for_each(|m| println!("{m}")),
                Err(err) => {
                    eprintln!("FAILED: {err}");
                    std::process::exit(1);
                }
            }
        }
        Cmd::Test { profile } => {
            let client = client(&store, &profile)?;
            let result = runtime.block_on(client.test(|t| {
                print!("{t}");
                let _ = std::io::stdout().flush();
            }));
            match result {
                Ok(report) => {
                    println!();
                    println!("{}", report.summary());
                }
                Err(err) => {
                    eprintln!("FAILED: {}", storage::secrets::redact(&err.to_string()));
                    std::process::exit(1);
                }
            }
        }
        Cmd::Remove { name } => {
            let p = store
                .find(&name)?
                .with_context(|| format!("no LLM profile named {name:?}"))?;
            store.delete(&p)?;
            println!("Removed {}", p.name);
        }
    }
    Ok(())
}
