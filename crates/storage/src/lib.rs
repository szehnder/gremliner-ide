//! SQLite application storage and OS keychain secrets (ADR-0022).
//!
//! One database file holds every table. Each [`Storage`] handle sends work to a single dedicated thread
//! that owns the connection, so callers never touch SQLite on the UI thread. The async methods return
//! executor-agnostic futures (a oneshot channel), which GPUI tasks and tokio tasks can both await. The
//! `*_blocking` variants exist only for startup, before the first frame, and for tests.

mod connections;
mod llm_profiles;
mod migrations;
mod schema_cache;
pub mod secrets;
mod stylesheets;
mod tabs;

pub use llm_profiles::LlmProfileRow;
pub use schema_cache::CachedSchema;
pub use stylesheets::StylesheetRow;

pub use connections::ConnectionRow;
pub use tabs::TabRow;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use futures::channel::oneshot;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use rusqlite;

/// File name of the application database inside the data directory.
pub const DB_FILE: &str = "gremliner.db";

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not create data directory {path}: {source}")]
    DataDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("value for {key:?} is not valid JSON: {source}")]
    Json {
        key: String,
        source: serde_json::Error,
    },
    #[error("the storage thread has stopped")]
    Closed,
    #[error("a connection named {0:?} already exists")]
    NameTaken(String),
}

pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Key/value tables. Values are JSON text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// User preferences, one row per settings page (`general`, `editor`, ...).
    Settings,
    /// Window geometry, dock layout, palette history and other UI state.
    UiState,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Settings => "settings",
            Table::UiState => "ui_state",
        }
    }
}

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// A cheap, cloneable handle to the storage thread.
#[derive(Clone)]
pub struct Storage {
    tx: mpsc::Sender<Job>,
    path: Option<PathBuf>,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage").field("path", &self.path).finish()
    }
}

impl Storage {
    /// The platform data directory (`~/.local/share/gremliner` on Linux), or `None` when the OS reports no
    /// home directory.
    pub fn default_data_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "gremliner", "gremliner")
            .map(|dirs| dirs.data_dir().to_path_buf())
    }

    /// Open (creating if needed) `<dir>/gremliner.db` and apply migrations.
    pub fn open_in_dir(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|source| StorageError::DataDir {
            path: dir.to_path_buf(),
            source,
        })?;
        Self::open(&dir.join(DB_FILE))
    }

    /// Open (creating if needed) the database at `path` and apply migrations. Blocks until the database is
    /// ready, which takes a few milliseconds; call it during startup.
    pub fn open(path: &Path) -> Result<Self> {
        let owned = path.to_path_buf();
        Self::spawn(Some(path.to_path_buf()), move || {
            let conn = Connection::open(&owned)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            Ok(conn)
        })
    }

    /// A private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::spawn(None, || Ok(Connection::open_in_memory()?))
    }

    fn spawn(
        path: Option<PathBuf>,
        connect: impl FnOnce() -> Result<Connection> + Send + 'static,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        std::thread::Builder::new()
            .name("gremliner-storage".into())
            .spawn(move || {
                let mut conn = match connect().and_then(|mut conn| {
                    migrations::apply(&mut conn)?;
                    Ok(conn)
                }) {
                    Ok(conn) => {
                        let _ = ready_tx.send(Ok(()));
                        conn
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                // Runs until every handle is dropped.
                while let Ok(job) = rx.recv() {
                    job(&mut conn);
                }
            })
            .map_err(|source| StorageError::DataDir {
                path: path.clone().unwrap_or_default(),
                source,
            })?;
        ready_rx.recv().map_err(|_| StorageError::Closed)??;
        Ok(Self { tx, path })
    }

    /// Path of the database file, or `None` for an in-memory database.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Run `f` on the storage thread and resolve with its result.
    pub fn call<T, F>(&self, f: F) -> impl Future<Output = Result<T>> + Send + 'static + use<T, F>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let sent = self
            .tx
            .send(Box::new(move |conn| {
                let _ = tx.send(f(conn));
            }))
            .is_ok();
        async move {
            if !sent {
                return Err(StorageError::Closed);
            }
            rx.await.map_err(|_| StorageError::Closed)?
        }
    }

    /// Run `f` on the storage thread and wait for it. Startup and tests only.
    pub fn call_blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Box::new(move |conn| {
                let _ = tx.send(f(conn));
            }))
            .map_err(|_| StorageError::Closed)?;
        rx.recv().map_err(|_| StorageError::Closed)?
    }

    /// Wait until every previously queued write has been applied. Call before the process exits.
    pub fn flush_blocking(&self) -> Result<()> {
        self.call_blocking(|_| Ok(()))
    }

    /// Read a raw JSON value.
    pub fn get(
        &self,
        table: Table,
        key: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send + 'static + use<> {
        let key = key.to_owned();
        self.call(move |conn| get_raw(conn, table, &key))
    }

    /// Read a raw JSON value, blocking. Startup only.
    pub fn get_blocking(&self, table: Table, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        self.call_blocking(move |conn| get_raw(conn, table, &key))
    }

    /// Write a raw JSON value.
    pub fn set(
        &self,
        table: Table,
        key: &str,
        value: String,
    ) -> impl Future<Output = Result<()>> + Send + 'static + use<> {
        let key = key.to_owned();
        self.call(move |conn| set_raw(conn, table, &key, &value))
    }

    /// Delete a key. Missing keys are not an error.
    pub fn delete(
        &self,
        table: Table,
        key: &str,
    ) -> impl Future<Output = Result<()>> + Send + 'static + use<> {
        let key = key.to_owned();
        self.call(move |conn| {
            conn.execute(
                &format!("DELETE FROM {} WHERE key = ?1", table.name()),
                params![key],
            )?;
            Ok(())
        })
    }

    /// Read and deserialize a value. A value that does not parse is reported as [`StorageError::Json`]
    /// so callers can fall back to defaults and log it.
    pub fn get_json<T: DeserializeOwned + Send + 'static>(
        &self,
        table: Table,
        key: &str,
    ) -> impl Future<Output = Result<Option<T>>> + Send + 'static + use<T> {
        let key = key.to_owned();
        let raw = self.get(table, &key);
        async move { decode(&key, raw.await?) }
    }

    /// Blocking variant of [`Self::get_json`]. Startup only.
    pub fn get_json_blocking<T: DeserializeOwned>(
        &self,
        table: Table,
        key: &str,
    ) -> Result<Option<T>> {
        decode(key, self.get_blocking(table, key)?)
    }

    /// Blocking variant of [`Self::set_json`].
    pub fn set_json_blocking<T: Serialize>(
        &self,
        table: Table,
        key: &str,
        value: &T,
    ) -> Result<()> {
        let json = serde_json::to_string(value).map_err(|source| StorageError::Json {
            key: key.to_owned(),
            source,
        })?;
        let key = key.to_owned();
        self.call_blocking(move |conn| set_raw(conn, table, &key, &json))
    }

    /// Serialize and write a value.
    pub fn set_json<T: Serialize>(
        &self,
        table: Table,
        key: &str,
        value: &T,
    ) -> impl Future<Output = Result<()>> + Send + 'static + use<T> {
        let encoded = serde_json::to_string(value).map_err(|source| StorageError::Json {
            key: key.to_owned(),
            source,
        });
        let write = encoded.map(|json| self.set(table, key, json));
        async move { write?.await }
    }
}

fn decode<T: DeserializeOwned>(key: &str, raw: Option<String>) -> Result<Option<T>> {
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|source| StorageError::Json {
            key: key.to_owned(),
            source,
        })
    })
    .transpose()
}

fn get_raw(conn: &Connection, table: Table, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            &format!("SELECT value FROM {} WHERE key = ?1", table.name()),
            params![key],
            |row| row.get(0),
        )
        .optional()?)
}

fn set_raw(conn: &Connection, table: Table, key: &str, value: &str) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO {} (key, value, updated_at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            table.name()
        ),
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct General {
        theme: String,
        editor_font_size: f32,
    }

    #[test]
    fn setting_round_trips() {
        let storage = Storage::open_in_memory().unwrap();
        let value = General {
            theme: "dark".into(),
            editor_font_size: 15.0,
        };
        futures::executor::block_on(storage.set_json(Table::Settings, "general", &value)).unwrap();
        let back: Option<General> =
            futures::executor::block_on(storage.get_json(Table::Settings, "general")).unwrap();
        assert_eq!(back, Some(value));
        // Tables are separate namespaces.
        assert_eq!(
            storage.get_blocking(Table::UiState, "general").unwrap(),
            None
        );
    }

    #[test]
    fn values_persist_across_reopen() {
        let dir = std::env::temp_dir().join(format!("gremliner-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let storage = Storage::open_in_dir(&dir).unwrap();
            futures::executor::block_on(storage.set(Table::UiState, "dock", "{\"a\":1}".into()))
                .unwrap();
            storage.flush_blocking().unwrap();
        }
        let storage = Storage::open_in_dir(&dir).unwrap();
        assert_eq!(
            storage
                .get_blocking(Table::UiState, "dock")
                .unwrap()
                .as_deref(),
            Some("{\"a\":1}")
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_json_is_reported_not_panicked() {
        let storage = Storage::open_in_memory().unwrap();
        futures::executor::block_on(storage.set(Table::UiState, "dock", "{not json".into()))
            .unwrap();
        let err = storage
            .get_json_blocking::<General>(Table::UiState, "dock")
            .unwrap_err();
        assert!(matches!(err, StorageError::Json { .. }), "{err}");
    }

    #[test]
    fn overwrite_and_delete() {
        let storage = Storage::open_in_memory().unwrap();
        futures::executor::block_on(storage.set(Table::Settings, "k", "1".into())).unwrap();
        futures::executor::block_on(storage.set(Table::Settings, "k", "2".into())).unwrap();
        assert_eq!(
            storage
                .get_blocking(Table::Settings, "k")
                .unwrap()
                .as_deref(),
            Some("2")
        );
        futures::executor::block_on(storage.delete(Table::Settings, "k")).unwrap();
        assert_eq!(storage.get_blocking(Table::Settings, "k").unwrap(), None);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::apply(&mut conn).unwrap();
        migrations::apply(&mut conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, migrations::LATEST);
    }
}
