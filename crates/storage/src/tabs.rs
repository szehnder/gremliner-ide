//! The `open_tabs` table: editor tab content and state, keyed by tab id. Tab order and placement live in
//! the dock layout (`ui_state`); this table holds what the layout references.

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Result, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRow {
    pub id: String,
    pub title: String,
    pub content: String,
    /// Cursor byte offset.
    pub cursor: usize,
    pub connection_id: Option<String>,
    /// The saved query this tab was opened from or saved as.
    pub saved_query_id: Option<String>,
    /// Tab settings as JSON (timeout, fetch edges, default view, parameters).
    pub settings: Option<String>,
}

impl Storage {
    pub fn save_tab(
        &self,
        row: TabRow,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| save(conn, &row))
    }

    pub fn save_tab_blocking(&self, row: TabRow) -> Result<()> {
        self.call_blocking(move |conn| save(conn, &row))
    }

    pub fn load_tab_blocking(&self, id: &str) -> Result<Option<TabRow>> {
        let id = id.to_owned();
        self.call_blocking(move |conn| load(conn, &id))
    }

    pub fn list_tabs_blocking(&self) -> Result<Vec<TabRow>> {
        self.call_blocking(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, content, cursor, connection_id, saved_query_id, settings FROM open_tabs ORDER BY rowid",
            )?;
            let rows = stmt
                .query_map([], row_to_tab)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn delete_tab(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        let id = id.to_owned();
        self.call(move |conn| {
            conn.execute("DELETE FROM open_tabs WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

fn row_to_tab(row: &rusqlite::Row<'_>) -> rusqlite::Result<TabRow> {
    let cursor: i64 = row.get(3)?;
    Ok(TabRow {
        id: row.get(0)?,
        title: row.get(1)?,
        content: row.get(2)?,
        cursor: usize::try_from(cursor).unwrap_or(0),
        connection_id: row.get(4)?,
        saved_query_id: row.get(5)?,
        settings: row.get(6)?,
    })
}

fn save(conn: &mut Connection, row: &TabRow) -> Result<()> {
    conn.execute(
        "INSERT INTO open_tabs (id, title, content, cursor, connection_id, saved_query_id, settings, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch())
         ON CONFLICT(id) DO UPDATE SET title = excluded.title, content = excluded.content,
             cursor = excluded.cursor, connection_id = excluded.connection_id,
             saved_query_id = excluded.saved_query_id, settings = excluded.settings,
             updated_at = excluded.updated_at",
        params![
            row.id,
            row.title,
            row.content,
            i64::try_from(row.cursor).unwrap_or(i64::MAX),
            row.connection_id,
            row.saved_query_id,
            row.settings
        ],
    )?;
    Ok(())
}

fn load(conn: &mut Connection, id: &str) -> Result<Option<TabRow>> {
    Ok(conn
        .query_row(
            "SELECT id, title, content, cursor, connection_id, saved_query_id, settings FROM open_tabs WHERE id = ?1",
            params![id],
            row_to_tab,
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn tabs_round_trip() {
        let s = Storage::open_in_memory().unwrap();
        let row = TabRow {
            id: "t1".into(),
            title: "Query 1".into(),
            content: "g.V()".into(),
            cursor: 3,
            connection_id: Some("c".into()),
            saved_query_id: Some("q".into()),
            settings: Some("{}".into()),
        };
        s.save_tab_blocking(row.clone()).unwrap();
        assert_eq!(s.load_tab_blocking("t1").unwrap(), Some(row.clone()));
        s.save_tab_blocking(TabRow {
            content: "g.E()".into(),
            ..row
        })
        .unwrap();
        assert_eq!(s.list_tabs_blocking().unwrap()[0].content, "g.E()");
        futures::executor::block_on(s.delete_tab("t1")).unwrap();
        assert!(s.list_tabs_blocking().unwrap().is_empty());
    }
}
