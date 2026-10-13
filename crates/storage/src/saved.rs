//! Saved queries in nested folders, globally or under a connection.

use rusqlite::{Connection, params};

use crate::{Result, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub connection_id: Option<String>,
    pub name: String,
    pub sort: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedQueryRow {
    pub id: String,
    pub folder_id: Option<String>,
    pub connection_id: Option<String>,
    pub name: String,
    pub text: String,
    /// Tab settings as JSON.
    pub settings: String,
    pub updated_at: i64,
}

impl Storage {
    pub fn list_folders_blocking(&self) -> Result<Vec<FolderRow>> {
        self.call_blocking(list_folders)
    }

    pub fn list_folders(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<FolderRow>>> + Send + 'static + use<> {
        self.call(list_folders)
    }

    pub fn save_folder(
        &self,
        row: FolderRow,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO folders (id, parent_id, connection_id, name, sort) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET parent_id = excluded.parent_id,
                     connection_id = excluded.connection_id, name = excluded.name, sort = excluded.sort",
                params![row.id, row.parent_id, row.connection_id, row.name, row.sort],
            )?;
            Ok(())
        })
    }

    /// Delete a folder with its subfolders and the queries in them.
    pub fn delete_folder(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        let id = id.to_owned();
        self.call(move |conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "WITH RECURSIVE sub(id) AS (
                     SELECT ?1 UNION ALL SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id)
                 DELETE FROM saved_queries WHERE folder_id IN (SELECT id FROM sub)",
                params![id],
            )?;
            tx.execute(
                "WITH RECURSIVE sub(id) AS (
                     SELECT ?1 UNION ALL SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id)
                 DELETE FROM folders WHERE id IN (SELECT id FROM sub)",
                params![id],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn list_saved_blocking(&self) -> Result<Vec<SavedQueryRow>> {
        self.call_blocking(list_saved)
    }

    pub fn list_saved(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<SavedQueryRow>>> + Send + 'static + use<>
    {
        self.call(list_saved)
    }

    pub fn save_query(
        &self,
        row: SavedQueryRow,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| save_query(conn, &row))
    }

    pub fn save_query_blocking(&self, row: SavedQueryRow) -> Result<()> {
        self.call_blocking(move |conn| save_query(conn, &row))
    }

    pub fn delete_saved(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        let id = id.to_owned();
        self.call(move |conn| {
            conn.execute("DELETE FROM saved_queries WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

fn list_folders(conn: &mut Connection) -> Result<Vec<FolderRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, parent_id, connection_id, name, sort FROM folders ORDER BY sort, name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FolderRow {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                connection_id: r.get(2)?,
                name: r.get(3)?,
                sort: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn list_saved(conn: &mut Connection) -> Result<Vec<SavedQueryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, folder_id, connection_id, name, text, settings, updated_at FROM saved_queries
         ORDER BY name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SavedQueryRow {
                id: r.get(0)?,
                folder_id: r.get(1)?,
                connection_id: r.get(2)?,
                name: r.get(3)?,
                text: r.get(4)?,
                settings: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn save_query(conn: &mut Connection, row: &SavedQueryRow) -> Result<()> {
    conn.execute(
        "INSERT INTO saved_queries (id, folder_id, connection_id, name, text, settings, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET folder_id = excluded.folder_id, connection_id = excluded.connection_id,
             name = excluded.name, text = excluded.text, settings = excluded.settings,
             updated_at = excluded.updated_at",
        params![
            row.id,
            row.folder_id,
            row.connection_id,
            row.name,
            row.text,
            row.settings,
            row.updated_at
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn folder(id: &str, parent: Option<&str>) -> FolderRow {
        FolderRow {
            id: id.into(),
            parent_id: parent.map(str::to_owned),
            connection_id: None,
            name: id.to_uppercase(),
            sort: 0,
        }
    }

    fn query(id: &str, folder: Option<&str>) -> SavedQueryRow {
        SavedQueryRow {
            id: id.into(),
            folder_id: folder.map(str::to_owned),
            connection_id: None,
            name: format!("Query {id}"),
            text: "g.V()".into(),
            settings: "{}".into(),
            updated_at: 1,
        }
    }

    #[test]
    fn folders_nest_and_delete_recursively() {
        let s = Storage::open_in_memory().unwrap();
        futures::executor::block_on(s.save_folder(folder("a", None))).unwrap();
        futures::executor::block_on(s.save_folder(folder("b", Some("a")))).unwrap();
        futures::executor::block_on(s.save_folder(folder("c", None))).unwrap();
        s.save_query_blocking(query("q1", Some("b"))).unwrap();
        s.save_query_blocking(query("q2", Some("c"))).unwrap();
        s.save_query_blocking(query("q3", None)).unwrap();
        assert_eq!(s.list_folders_blocking().unwrap().len(), 3);
        // Move q3 into a, rename it.
        s.save_query_blocking(SavedQueryRow {
            folder_id: Some("a".into()),
            name: "Renamed".into(),
            ..query("q3", None)
        })
        .unwrap();
        let q3 = s
            .list_saved_blocking()
            .unwrap()
            .into_iter()
            .find(|q| q.id == "q3")
            .unwrap();
        assert_eq!(
            (q3.folder_id.as_deref(), q3.name.as_str()),
            (Some("a"), "Renamed")
        );
        futures::executor::block_on(s.delete_folder("a")).unwrap();
        let folders: Vec<String> = s
            .list_folders_blocking()
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(folders, ["c"]);
        let left: Vec<String> = s
            .list_saved_blocking()
            .unwrap()
            .into_iter()
            .map(|q| q.id)
            .collect();
        assert_eq!(left, ["q2"], "queries in a and its subfolder b are gone");
        futures::executor::block_on(s.delete_saved("q2")).unwrap();
        assert!(s.list_saved_blocking().unwrap().is_empty());
    }
}
