//! The `connections` table. Rows hold an id, a unique (case-insensitive) name and the provider config as
//! JSON; the typed model lives in `providers`. Secrets are never stored here (see [`crate::secrets`]).

use rusqlite::{Connection, ErrorCode, params};

use crate::{Result, Storage, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRow {
    pub id: String,
    pub name: String,
    /// Provider config JSON.
    pub config: String,
}

impl Storage {
    /// All saved connections in display order.
    pub fn list_connections(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ConnectionRow>>> + Send + 'static + use<>
    {
        self.call(list)
    }

    pub fn list_connections_blocking(&self) -> Result<Vec<ConnectionRow>> {
        self.call_blocking(list)
    }

    /// Insert or update by id. A name used by another connection fails with [`StorageError::NameTaken`].
    pub fn save_connection(
        &self,
        row: ConnectionRow,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| save(conn, &row))
    }

    pub fn save_connection_blocking(&self, row: ConnectionRow) -> Result<()> {
        self.call_blocking(move |conn| save(conn, &row))
    }

    pub fn delete_connection(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        let id = id.to_owned();
        self.call(move |conn| delete(conn, &id))
    }

    pub fn delete_connection_blocking(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.call_blocking(move |conn| delete(conn, &id))
    }
}

fn list(conn: &mut Connection) -> Result<Vec<ConnectionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, config FROM connections ORDER BY position, name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ConnectionRow {
                id: row.get(0)?,
                name: row.get(1)?,
                config: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn save(conn: &mut Connection, row: &ConnectionRow) -> Result<()> {
    let result = conn.execute(
        "INSERT INTO connections (id, name, config, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM connections), unixepoch(), unixepoch())
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, config = excluded.config,
                                       updated_at = excluded.updated_at",
        params![row.id, row.name, row.config],
    );
    match result {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ErrorCode::ConstraintViolation => {
            Err(StorageError::NameTaken(row.name.clone()))
        }
        Err(e) => Err(e.into()),
    }
}

fn delete(conn: &mut Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM connections WHERE id = ?1", params![id])?;
    conn.execute(
        "DELETE FROM schema_cache WHERE connection_id = ?1",
        params![id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn row(id: &str, name: &str) -> ConnectionRow {
        ConnectionRow {
            id: id.into(),
            name: name.into(),
            config: "{}".into(),
        }
    }

    #[test]
    fn crud_and_unique_names() {
        let s = Storage::open_in_memory().unwrap();
        s.save_connection_blocking(row("1", "Local")).unwrap();
        s.save_connection_blocking(row("2", "Prod")).unwrap();
        let err = s.save_connection_blocking(row("3", "local")).unwrap_err();
        assert!(matches!(err, StorageError::NameTaken(_)), "{err}");
        // Renaming a row to its own name is fine; updating keeps the position.
        s.save_connection_blocking(ConnectionRow {
            config: "{\"x\":1}".into(),
            ..row("1", "Local")
        })
        .unwrap();
        let names: Vec<_> = s
            .list_connections_blocking()
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["Local", "Prod"]);
        s.delete_connection_blocking("1").unwrap();
        assert_eq!(s.list_connections_blocking().unwrap().len(), 1);
    }
}
