//! The `schema_cache` table: the last discovered schema per connection, as JSON. The model lives in the
//! `schema` crate; this table stores the text and when it was discovered.

use rusqlite::{OptionalExtension as _, params};

use crate::{Result, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSchema {
    pub connection_id: String,
    /// Milliseconds since the Unix epoch.
    pub discovered_at_ms: i64,
    pub json: String,
}

impl Storage {
    pub fn load_schema(
        &self,
        connection_id: &str,
    ) -> impl std::future::Future<Output = Result<Option<CachedSchema>>> + Send + 'static + use<>
    {
        let id = connection_id.to_owned();
        self.call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT discovered_at, json FROM schema_cache WHERE connection_id = ?1",
                    params![id],
                    |r| {
                        Ok(CachedSchema {
                            connection_id: id.clone(),
                            discovered_at_ms: r.get(0)?,
                            json: r.get(1)?,
                        })
                    },
                )
                .optional()?)
        })
    }

    pub fn save_schema(
        &self,
        row: CachedSchema,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO schema_cache (connection_id, discovered_at, json) VALUES (?1, ?2, ?3)
                 ON CONFLICT(connection_id) DO UPDATE SET discovered_at = excluded.discovered_at,
                     json = excluded.json",
                params![row.connection_id, row.discovered_at_ms, row.json],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn schema_cache_round_trip_and_replace() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(
            futures::executor::block_on(s.load_schema("c1")).unwrap(),
            None
        );
        let row = CachedSchema {
            connection_id: "c1".into(),
            discovered_at_ms: 10,
            json: "{}".into(),
        };
        futures::executor::block_on(s.save_schema(row.clone())).unwrap();
        futures::executor::block_on(s.save_schema(CachedSchema {
            discovered_at_ms: 20,
            json: "{\"a\":1}".into(),
            ..row
        }))
        .unwrap();
        let back = futures::executor::block_on(s.load_schema("c1"))
            .unwrap()
            .unwrap();
        assert_eq!(back.discovered_at_ms, 20);
        assert_eq!(back.json, "{\"a\":1}");
    }
}
