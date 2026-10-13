//! The app's view of saved connections and live sessions. Every store and network call runs on the I/O
//! runtime; this entity only holds state and notifies observers.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::*;
use providers::{
    ConnectionConfig, ConnectionStore, PasswordChange, ProviderError, ServerInfo, Session,
    StoreError, TestReport, provider_for, test_connection, validate_name,
};
use storage::Storage;
use storage::secrets::{LazyStore, SecretStore};
use uuid::Uuid;

use crate::io::{spawn_io, spawn_io_tracked};

/// Connection state shown by the sidebar and status bar.
#[derive(Debug, Clone, Default)]
pub enum ConnStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected(ServerInfo),
    Error(ProviderError),
}

impl ConnStatus {
    pub fn label(&self) -> &'static str {
        match self {
            ConnStatus::Disconnected => "Disconnected",
            ConnStatus::Connecting => "Connecting…",
            ConnStatus::Connected(_) => "Connected",
            ConnStatus::Error(_) => "Connection failed",
        }
    }
}

pub struct ConnectionManager {
    store: ConnectionStore,
    connections: Vec<ConnectionConfig>,
    status: HashMap<Uuid, ConnStatus>,
    sessions: HashMap<Uuid, Session>,
    active: Option<Uuid>,
    aws_files: Option<providers::neptune::AwsFiles>,
}

struct ManagerGlobal(Entity<ConnectionManager>);

impl Global for ManagerGlobal {}

/// A completed save.
#[derive(Debug, Clone)]
pub struct Saved {
    pub config: ConnectionConfig,
    /// False when a password was given but there is no credential store (kept for this session only).
    pub password_persisted: bool,
}

/// Why a save failed, for the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Name(String),
    Other(String),
}

impl ConnectionManager {
    /// Install the global manager. `storage` is the app database (an in-memory one when not persisting);
    /// `secrets` defaults to the lazily probed OS credential store.
    pub fn init(storage: Storage, secrets: Option<Arc<dyn SecretStore>>, cx: &mut App) {
        let secrets = secrets.unwrap_or_else(|| Arc::new(LazyStore::default()));
        let store = ConnectionStore::new(storage, secrets);
        // Listing reads SQLite only (no secrets), before the first frame.
        let connections = store.list().unwrap_or_else(|err| {
            log::warn!("could not load saved connections: {err}");
            Vec::new()
        });
        let entity = cx.new(|_| Self {
            store,
            connections,
            status: HashMap::new(),
            sessions: HashMap::new(),
            active: None,
            aws_files: None,
        });
        cx.set_global(ManagerGlobal(entity));
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<ManagerGlobal>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<ManagerGlobal>().map(|g| g.0.clone())
    }

    /// Read AWS profiles from these files instead of the usual locations (tests, custom setups).
    pub fn set_aws_files(&mut self, files: Option<providers::neptune::AwsFiles>) {
        self.aws_files = files;
    }

    /// The AWS profiles for the Neptune profile picker, read off the UI thread.
    pub fn aws_profiles(&self, cx: &App) -> Task<Vec<providers::neptune::AwsProfile>> {
        let files = self
            .aws_files
            .clone()
            .or_else(providers::neptune::AwsFiles::from_env);
        cx.background_spawn(async move {
            files
                .map(|f| providers::neptune::list_profiles(&f))
                .unwrap_or_default()
        })
    }

    /// The app database (schema cache, connections).
    pub fn storage(&self) -> &Storage {
        self.store.storage()
    }

    pub fn connections(&self) -> &[ConnectionConfig] {
        &self.connections
    }

    pub fn get(&self, id: Uuid) -> Option<&ConnectionConfig> {
        self.connections.iter().find(|c| c.id == id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&ConnectionConfig> {
        self.connections
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn status(&self, id: Uuid) -> ConnStatus {
        self.status.get(&id).cloned().unwrap_or_default()
    }

    pub fn session(&self, id: Uuid) -> Option<&Session> {
        self.sessions.get(&id)
    }

    /// The connection new editor tabs and the status bar use.
    pub fn active(&self) -> Option<&ConnectionConfig> {
        self.active.and_then(|id| self.get(id))
    }

    pub fn set_active(&mut self, id: Option<Uuid>, cx: &mut Context<'_, Self>) {
        self.active = id;
        cx.notify();
    }

    pub fn validate_name(&self, name: &str, own_id: Option<Uuid>) -> Result<(), String> {
        validate_name(&self.connections, name, own_id).map_err(|e| e.to_string())
    }

    /// Save a new or edited connection.
    pub fn save(
        &mut self,
        config: ConnectionConfig,
        password: PasswordChange,
        cx: &mut Context<'_, Self>,
    ) -> Task<Result<Saved, SaveError>> {
        if let Err(err) = self.validate_name(&config.name, Some(config.id)) {
            return Task::ready(Err(SaveError::Name(err)));
        }
        let store = self.store.clone();
        let sets_password = matches!(&password, PasswordChange::Set(p) if !p.is_empty());
        let work = spawn_io(
            cx,
            blocking(move || {
                let saved = store.save(config, password)?;
                // Only ask about persistence when a secret was written (that probe already happened).
                let persisted = !sets_password || store.secrets_persist();
                Ok((saved, persisted))
            }),
        );
        cx.spawn(async move |this, cx| {
            let (saved, password_persisted) = match work.await {
                Ok(Ok(saved)) => saved,
                Ok(Err(StoreError::Storage(storage::StorageError::NameTaken(name)))) => {
                    return Err(SaveError::Name(format!(
                        "A connection named \"{name}\" already exists."
                    )));
                }
                Ok(Err(err)) => return Err(SaveError::Other(err.to_string())),
                Err(err) => return Err(SaveError::Other(err.to_string())),
            };
            let _ = this.update(cx, |this, cx| {
                match this.connections.iter_mut().find(|c| c.id == saved.id) {
                    Some(existing) => *existing = saved.clone(),
                    None => this.connections.push(saved.clone()),
                }
                // Settings changed: an open session no longer matches them.
                if this.sessions.remove(&saved.id).is_some() {
                    this.status.insert(saved.id, ConnStatus::Disconnected);
                }
                cx.notify();
            });
            Ok(Saved {
                config: saved,
                password_persisted,
            })
        })
    }

    /// Delete a connection and its stored secrets (the caller has already confirmed).
    pub fn delete(&mut self, id: Uuid, cx: &mut Context<'_, Self>) -> Task<Result<(), String>> {
        let Some(config) = self.get(id).cloned() else {
            return Task::ready(Ok(()));
        };
        let store = self.store.clone();
        let work = spawn_io(cx, blocking(move || store.delete(&config)));
        cx.spawn(async move |this, cx| {
            match work.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => return Err(err.to_string()),
                Err(err) => return Err(err.to_string()),
            }
            let _ = this.update(cx, |this, cx| {
                this.connections.retain(|c| c.id != id);
                this.sessions.remove(&id);
                this.status.remove(&id);
                if this.active == Some(id) {
                    this.active = None;
                }
                cx.notify();
            });
            Ok(())
        })
    }

    pub fn duplicate(&mut self, id: Uuid, cx: &mut Context<'_, Self>) -> Task<Result<(), String>> {
        let Some(config) = self.get(id).cloned() else {
            return Task::ready(Ok(()));
        };
        let store = self.store.clone();
        let work = spawn_io(cx, blocking(move || store.duplicate(&config)));
        cx.spawn(async move |this, cx| {
            let copy = match work.await {
                Ok(Ok(copy)) => copy,
                Ok(Err(err)) => return Err(err.to_string()),
                Err(err) => return Err(err.to_string()),
            };
            let _ = this.update(cx, |this, cx| {
                this.connections.push(copy);
                cx.notify();
            });
            Ok(())
        })
    }

    /// Connect (and make active). Status goes Disconnected → Connecting → Connected or Error.
    pub fn connect(&mut self, id: Uuid, cx: &mut Context<'_, Self>) -> Task<()> {
        let Some(config) = self.get(id).cloned() else {
            return Task::ready(());
        };
        self.active = Some(id);
        self.status.insert(id, ConnStatus::Connecting);
        cx.notify();
        let store = self.store.clone();
        let label = format!("Connecting to {}", config.name);
        let work = spawn_io_tracked(label, cx, async move {
            let lookup = config.clone();
            let password = blocking(move || store.password(&lookup)).await;
            let password = match password {
                Ok(p) => p,
                Err(err) => {
                    return Err(ProviderError::unsupported(format!(
                        "Could not read the saved password: {err}"
                    )));
                }
            };
            provider_for(&config, password)?.connect().await
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                // Ignore a result for a connection deleted or disconnected meanwhile.
                if !matches!(this.status.get(&id), Some(ConnStatus::Connecting)) {
                    return;
                }
                let status = match result {
                    Ok(Ok(session)) => {
                        let info = session.info.clone();
                        this.sessions.insert(id, session);
                        ConnStatus::Connected(info)
                    }
                    Ok(Err(err)) => ConnStatus::Error(err),
                    Err(err) => ConnStatus::Error(ProviderError::unsupported(err.to_string())),
                };
                log::info!("connection {id}: {}", status.label());
                this.status.insert(id, status);
                cx.notify();
            });
        })
    }

    pub fn disconnect(&mut self, id: Uuid, cx: &mut Context<'_, Self>) {
        self.sessions.remove(&id);
        self.status.insert(id, ConnStatus::Disconnected);
        cx.notify();
    }

    /// Test settings without saving. `password` is the typed password, or the stored one for `Keep`.
    pub fn test(
        &self,
        config: ConnectionConfig,
        password: PasswordChange,
        cx: &mut Context<'_, Self>,
    ) -> Task<Result<TestReport, String>> {
        let store = self.store.clone();
        let work = spawn_io(cx, async move {
            let password = match password {
                PasswordChange::Set(p) => Some(p),
                PasswordChange::Clear => None,
                PasswordChange::Keep => {
                    let lookup = config.clone();
                    blocking(move || store.password(&lookup))
                        .await
                        .ok()
                        .flatten()
                }
            };
            test_connection(&config, password).await
        });
        cx.background_spawn(async move { work.await.map_err(|e| e.to_string()) })
    }
}

/// Run blocking store work on tokio's blocking pool (called from inside `spawn_io` futures).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|join| Err(StoreError::Task(join.to_string())))
}
