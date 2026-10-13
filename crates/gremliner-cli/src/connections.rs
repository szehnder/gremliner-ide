//! `gremliner-cli connections list|add|test|remove|aws-profiles`: the same database and credential store as
//! the app.

use std::io::Read as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use providers::endpoint;
use providers::{
    ConnectionConfig, ConnectionStore, Environment, GremlinServerConfig, NeptuneConfig,
    PasswordChange, ProviderConfig, test_connection, validate_name,
};
use storage::Storage;

#[derive(Subcommand)]
pub enum Cmd {
    /// List saved connections.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Save a new connection.
    Add {
        #[arg(long)]
        name: String,
        /// Endpoint: `ws://host:port/path`, `wss://...`, `host:port` or a Neptune endpoint.
        #[arg(long)]
        url: String,
        /// Server-side traversal source bound to `g`.
        #[arg(long, default_value = "g")]
        alias: String,
        #[arg(long)]
        user: Option<String>,
        /// Read the password from stdin (never pass it as an argument).
        #[arg(long)]
        password_stdin: bool,
        #[arg(long, value_enum, default_value_t = EnvArg::Local)]
        env: EnvArg,
        #[arg(long)]
        read_only: bool,
        /// Skip TLS certificate verification (development servers, tunnels).
        #[arg(long)]
        accept_invalid_certs: bool,
        /// Amazon Neptune even when the host is not a `*.neptune.amazonaws.com` name.
        #[arg(long)]
        neptune: bool,
        /// Neptune: AWS region (default: from the endpoint name).
        #[arg(long)]
        region: Option<String>,
        /// Neptune: AWS profile (default: the default credential chain).
        #[arg(long)]
        profile: Option<String>,
        /// Neptune: connect through this local tunnel (`host:port`), signing for the endpoint.
        #[arg(long)]
        tunnel: Option<String>,
        /// Neptune: no IAM authentication.
        #[arg(long)]
        no_iam: bool,
        /// Neptune: plain ws:// (stand-ins such as a local Gremlin Server).
        #[arg(long)]
        no_tls: bool,
    },
    /// List the AWS profiles Neptune connections can use.
    AwsProfiles {
        #[arg(long)]
        json: bool,
    },
    /// Test a saved connection: prints latency and version, exits non-zero on failure.
    Test { name: String },
    /// Delete a saved connection and its stored password.
    Remove { name: String },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum EnvArg {
    Local,
    Dev,
    Staging,
    Production,
}

impl From<EnvArg> for Environment {
    fn from(e: EnvArg) -> Self {
        match e {
            EnvArg::Local => Environment::Local,
            EnvArg::Dev => Environment::Dev,
            EnvArg::Staging => Environment::Staging,
            EnvArg::Production => Environment::Production,
        }
    }
}

pub fn open_store(data_dir: Option<PathBuf>) -> Result<ConnectionStore> {
    let dir = data_dir
        .or_else(Storage::default_data_dir)
        .context("no data directory; pass --data-dir")?;
    let storage = Storage::open_in_dir(&dir)?;
    Ok(ConnectionStore::new(storage, storage::secrets::detect()))
}

pub fn run(cmd: Cmd, data_dir: Option<PathBuf>, runtime: &tokio::runtime::Runtime) -> Result<()> {
    let store = open_store(data_dir)?;
    match cmd {
        Cmd::List { json } => {
            let all = store.list()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else if all.is_empty() {
                println!("No saved connections.");
            } else {
                for c in all {
                    let flags = if c.read_only { "  read-only" } else { "" };
                    println!(
                        "{:<24} {:<16} {:<11} {}{flags}",
                        c.name,
                        c.provider.kind().label(),
                        c.environment.label(),
                        c.endpoint_label()
                    );
                }
            }
        }
        Cmd::Add {
            name,
            url,
            alias,
            user,
            password_stdin,
            env,
            read_only,
            accept_invalid_certs,
            neptune,
            region,
            profile,
            tunnel,
            no_iam,
            no_tls,
        } => {
            if let Err(err) = validate_name(&store.list()?, &name, None) {
                bail!("{err}");
            }
            let parsed =
                endpoint::parse(&url).with_context(|| format!("not an endpoint: {url}"))?;
            let is_neptune = neptune || parsed.neptune_region.is_some();
            let provider = match is_neptune {
                true => ProviderConfig::NeptuneAws(NeptuneConfig {
                    host: parsed.host.clone(),
                    port: parsed.port.unwrap_or(8182),
                    region: region.or(parsed.neptune_region.clone()).unwrap_or_default(),
                    iam_auth: !no_iam,
                    profile,
                    tunnel,
                    accept_invalid_certs,
                    tls: !no_tls,
                }),
                false => ProviderConfig::GremlinServer(GremlinServerConfig {
                    host: parsed.host.clone(),
                    port: parsed.port.unwrap_or(8182),
                    path: parsed.path.clone().unwrap_or_else(|| "/gremlin".into()),
                    tls: parsed.tls.unwrap_or(false),
                    accept_invalid_certs,
                    username: user,
                    alias,
                    ..Default::default()
                }),
            };
            let mut config = ConnectionConfig::new(name, provider);
            config.environment = env.into();
            config.read_only = read_only;
            let password = if password_stdin {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                PasswordChange::Set(buf.trim_end_matches(['\r', '\n']).to_owned())
            } else {
                PasswordChange::Clear
            };
            let saved = store.save(config, password)?;
            if !store.secrets_persist() && password_stdin {
                eprintln!("warning: no credential store; the password was not saved");
            }
            println!("Saved {} ({})", saved.name, saved.endpoint_label());
        }
        Cmd::Test { name } => {
            let config = store
                .find(&name)?
                .with_context(|| format!("no connection named {name:?}"))?;
            let password = store.password(&config)?;
            let report = runtime.block_on(test_connection(&config, password));
            match report.result {
                Ok(info) => {
                    let engine = info
                        .engine_version
                        .map(|v| format!(", Neptune {v}"))
                        .unwrap_or_default();
                    let version = info
                        .tinkerpop_version
                        .map(|v| format!("{engine}, TinkerPop {v}"))
                        .unwrap_or(engine);
                    println!(
                        "OK: connected to {} in {} ms{version}",
                        config.name,
                        info.latency.as_millis()
                    );
                }
                Err(err) => {
                    eprintln!("FAILED: {}", err.reason);
                    if !err.suggestion.is_empty() {
                        eprintln!("{}", err.suggestion);
                    }
                    if !err.detail.is_empty() {
                        eprintln!("({})", storage::secrets::redact(&err.detail));
                    }
                    std::process::exit(1);
                }
            }
        }
        Cmd::AwsProfiles { json } => {
            let profiles = providers::neptune::AwsFiles::from_env()
                .map(|f| providers::neptune::list_profiles(&f))
                .unwrap_or_default();
            if json {
                let rows: Vec<_> = profiles
                    .iter()
                    .map(|p| serde_json::json!({"name": p.name, "sso": p.sso, "region": p.region}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if profiles.is_empty() {
                println!("No AWS profiles found in ~/.aws/config or ~/.aws/credentials.");
            } else {
                for p in profiles {
                    println!(
                        "{:<24} {:<4} {}",
                        p.name,
                        if p.sso { "SSO" } else { "" },
                        p.region.unwrap_or_default()
                    );
                }
            }
        }
        Cmd::Remove { name } => {
            let config = store
                .find(&name)?
                .with_context(|| format!("no connection named {name:?}"))?;
            store.delete(&config)?;
            println!("Removed {}", config.name);
        }
    }
    Ok(())
}
