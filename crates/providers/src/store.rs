//! Saved connections: configs in the `connections` table, passwords in the credential store.
//!
//! Methods are blocking (SQLite round trip plus a possible keychain call): run them on a blocking thread
//! (`tokio::task::spawn_blocking`) or from the CLI.

use std::sync::Arc;

use storage::secrets::{SecretError, SecretStore};
use storage::{ConnectionRow, Storage, StorageError};
use uuid::Uuid;

use crate::config::{ConnectionConfig, ProviderConfig};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error("saved connection {0} is unreadable: {1}")]
    Corrupt(String, serde_json::Error),
    #[error("background task failed: {0}")]
    Task(String),
}

/// What to do with the stored password when saving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordChange {
    Keep,
    Set(String),
    Clear,
}

/// Name validation shared by the form, the CLI and the store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Enter a name.")]
    Empty,
    #[error("A connection named \"{0}\" already exists.")]
    Taken(String),
}

/// Check `name` against the other saved connections (case-insensitive), ignoring `own_id`.
pub fn validate_name(
    existing: &[ConnectionConfig],
    name: &str,
    own_id: Option<Uuid>,
) -> Result<(), NameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    let taken = existing
        .iter()
        .any(|c| Some(c.id) != own_id && c.name.trim().eq_ignore_ascii_case(name));
    if taken {
        Err(NameError::Taken(name.to_owned()))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionStore {
    storage: Storage,
    secrets: Arc<dyn SecretStore>,
}

impl ConnectionStore {
    pub fn new(storage: Storage, secrets: Arc<dyn SecretStore>) -> Self {
        Self { storage, secrets }
    }

    /// The app database this store reads and writes.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn secrets_persist(&self) -> bool {
        self.secrets.is_persistent()
    }

    /// Every saved connection. Unreadable rows are skipped with a warning.
    pub fn list(&self) -> Result<Vec<ConnectionConfig>, StoreError> {
        Ok(self
            .storage
            .list_connections_blocking()?
            .into_iter()
            .filter_map(
                |row| match serde_json::from_str::<ConnectionConfig>(&row.config) {
                    Ok(config) => Some(config),
                    Err(err) => {
                        log::warn!("skipping unreadable connection {:?}: {err}", row.name);
                        None
                    }
                },
            )
            .collect())
    }

    pub fn find(&self, name: &str) -> Result<Option<ConnectionConfig>, StoreError> {
        Ok(self
            .list()?
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(name.trim())))
    }

    /// Insert or update, applying the password change. Returns the config as stored.
    pub fn save(
        &self,
        mut config: ConnectionConfig,
        password: PasswordChange,
    ) -> Result<ConnectionConfig, StoreError> {
        config.name = config.name.trim().to_owned();
        let account = config.password_account();
        let has_password = match &password {
            PasswordChange::Set(p) if !p.is_empty() => {
                self.secrets.set(&account, p)?;
                true
            }
            PasswordChange::Set(_) | PasswordChange::Clear => {
                self.secrets.delete(&account)?;
                false
            }
            PasswordChange::Keep => match &config.provider {
                ProviderConfig::GremlinServer(c) => c.has_password,
                ProviderConfig::NeptuneAws(_) | ProviderConfig::NeptuneLocalStack(_) => false,
            },
        };
        if let ProviderConfig::GremlinServer(c) = &mut config.provider {
            c.has_password = has_password;
        }
        let row = ConnectionRow {
            id: config.id.to_string(),
            name: config.name.clone(),
            config: serde_json::to_string(&config)
                .map_err(|e| StoreError::Corrupt(config.name.clone(), e))?,
        };
        self.storage.save_connection_blocking(row)?;
        Ok(config)
    }

    /// The stored password, if any.
    pub fn password(&self, config: &ConnectionConfig) -> Result<Option<String>, StoreError> {
        let wants = matches!(&config.provider, ProviderConfig::GremlinServer(c) if c.has_password);
        if !wants {
            return Ok(None);
        }
        Ok(self.secrets.get(&config.password_account())?)
    }

    /// Remove a connection and its secrets.
    pub fn delete(&self, config: &ConnectionConfig) -> Result<(), StoreError> {
        self.secrets.delete(&config.password_account())?;
        self.storage
            .delete_connection_blocking(&config.id.to_string())?;
        Ok(())
    }

    /// A copy named "<name> copy" (or "copy 2", ...), with the same password.
    pub fn duplicate(&self, config: &ConnectionConfig) -> Result<ConnectionConfig, StoreError> {
        let existing = self.list()?;
        let mut copy = config.clone();
        copy.id = Uuid::new_v4();
        copy.name = (1..)
            .map(|n| match n {
                1 => format!("{} copy", config.name),
                n => format!("{} copy {n}", config.name),
            })
            .find(|name| validate_name(&existing, name, None).is_ok())
            .unwrap_or_default();
        let password = match self.password(config)? {
            Some(p) => PasswordChange::Set(p),
            None => PasswordChange::Clear,
        };
        self.save(copy, password)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use storage::secrets::MemoryStore;

    use super::*;
    use crate::config::GremlinServerConfig;

    fn store() -> (ConnectionStore, Storage) {
        let storage = Storage::open_in_memory().unwrap();
        (
            ConnectionStore::new(storage.clone(), Arc::new(MemoryStore::default())),
            storage,
        )
    }

    fn server(name: &str) -> ConnectionConfig {
        ConnectionConfig::new(
            name,
            ProviderConfig::GremlinServer(GremlinServerConfig {
                username: Some("stephen".into()),
                ..Default::default()
            }),
        )
    }

    #[test]
    fn scenario_connections_password_not_on_disk() {
        let dir = std::env::temp_dir().join(format!("gremliner-conn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::open_in_dir(&dir).unwrap();
        let store = ConnectionStore::new(storage.clone(), Arc::new(MemoryStore::default()));
        let secret = "pa55-unique-7f3e9c";
        let saved = store
            .save(server("Auth"), PasswordChange::Set(secret.into()))
            .unwrap();
        assert_eq!(store.password(&saved).unwrap().as_deref(), Some(secret));
        storage.flush_blocking().unwrap();
        // Nothing under the data directory (database, WAL, shared memory) contains the password.
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let Ok(bytes) = std::fs::read(&path) else {
                continue; // SQLite may remove its side files at any moment
            };
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "{} contains the password",
                path.display()
            );
        }
        // And redaction keeps it out of logs.
        assert!(!storage::secrets::redact(&format!("error: {secret}")).contains(secret));
        drop(store);
        drop(storage);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_unique_case_insensitively() {
        let (store, _) = store();
        let a = store.save(server("Local"), PasswordChange::Keep).unwrap();
        let all = store.list().unwrap();
        assert_eq!(
            validate_name(&all, " local ", None),
            Err(NameError::Taken("local".into()))
        );
        assert_eq!(validate_name(&all, "Local", Some(a.id)), Ok(()));
        assert_eq!(validate_name(&all, "  ", None), Err(NameError::Empty));
        let err = store
            .save(server("LOCAL"), PasswordChange::Keep)
            .unwrap_err();
        assert!(matches!(
            err,
            StoreError::Storage(StorageError::NameTaken(_))
        ));
    }

    #[test]
    fn delete_removes_row_and_secret() {
        let (store, _) = store();
        let c = store
            .save(server("Gone"), PasswordChange::Set("secret-123".into()))
            .unwrap();
        store.delete(&c).unwrap();
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.secrets.get(&c.password_account()).unwrap(), None);
    }

    #[test]
    fn duplicate_copies_settings_and_password() {
        let (store, _) = store();
        let c = store
            .save(server("Dev"), PasswordChange::Set("secret-456".into()))
            .unwrap();
        let copy = store.duplicate(&c).unwrap();
        let copy2 = store.duplicate(&c).unwrap();
        assert_eq!(copy.name, "Dev copy");
        assert_eq!(copy2.name, "Dev copy 2");
        assert_ne!(copy.id, c.id);
        assert_eq!(
            store.password(&copy).unwrap().as_deref(),
            Some("secret-456")
        );
    }
}
