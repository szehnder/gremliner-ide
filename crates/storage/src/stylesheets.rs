//! The `stylesheets` table: graph stylesheets as JSON documents keyed by id. The model and its schema
//! live in `graph_store::style`; this table stores the text.

use rusqlite::params;

use crate::{Result, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StylesheetRow {
    pub id: String,
    pub name: String,
    pub json: String,
}

impl Storage {
    pub fn list_stylesheets(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<StylesheetRow>>> + Send + 'static + use<>
    {
        self.call(|conn| {
            let mut stmt = conn.prepare("SELECT id, name, json FROM stylesheets ORDER BY rowid")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(StylesheetRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        json: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    pub fn save_stylesheet(
        &self,
        row: StylesheetRow,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO stylesheets (id, name, json, updated_at) VALUES (?1, ?2, ?3, unixepoch())
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, json = excluded.json,
                     updated_at = excluded.updated_at",
                params![row.id, row.name, row.json],
            )?;
            Ok(())
        })
    }

    pub fn delete_stylesheet(
        &self,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        let id = id.to_owned();
        self.call(move |conn| {
            conn.execute("DELETE FROM stylesheets WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn stylesheets_round_trip() {
        let s = Storage::open_in_memory().unwrap();
        let row = StylesheetRow {
            id: "a".into(),
            name: "Air".into(),
            json: "{}".into(),
        };
        futures::executor::block_on(s.save_stylesheet(row.clone())).unwrap();
        futures::executor::block_on(s.save_stylesheet(StylesheetRow {
            name: "Air 2".into(),
            ..row.clone()
        }))
        .unwrap();
        let rows = futures::executor::block_on(s.list_stylesheets()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Air 2");
        futures::executor::block_on(s.delete_stylesheet("a")).unwrap();
        assert!(
            futures::executor::block_on(s.list_stylesheets())
                .unwrap()
                .is_empty()
        );
    }
}
