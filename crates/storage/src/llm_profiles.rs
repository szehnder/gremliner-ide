//! The `llm_profiles` table: LLM profiles as JSON keyed by id with a unique (case-insensitive) name. The
//! model lives in `llm`; keys are never stored here (see [`crate::secrets`]).

use rusqlite::{ErrorCode, params};

use crate::{Result, Storage, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProfileRow {
    pub id: String,
    pub name: String,
    pub json: String,
}

impl Storage {
    pub fn list_llm_profiles_blocking(&self) -> Result<Vec<LlmProfileRow>> {
        self.call_blocking(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, json FROM llm_profiles ORDER BY position, name COLLATE NOCASE",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(LlmProfileRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        json: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    }

    /// Insert or update by id; a name used by another profile fails with [`StorageError::NameTaken`].
    pub fn save_llm_profile_blocking(&self, row: LlmProfileRow) -> Result<()> {
        self.call_blocking(move |conn| {
            let result = conn.execute(
                "INSERT INTO llm_profiles (id, name, json, position, updated_at)
                 VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM llm_profiles), unixepoch())
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, json = excluded.json,
                     updated_at = excluded.updated_at",
                params![row.id, row.name, row.json],
            );
            match result {
                Ok(_) => Ok(()),
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == ErrorCode::ConstraintViolation =>
                {
                    Err(StorageError::NameTaken(row.name.clone()))
                }
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn delete_llm_profile_blocking(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.call_blocking(move |conn| {
            conn.execute("DELETE FROM llm_profiles WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn profiles_round_trip_with_unique_names() {
        let s = Storage::open_in_memory().unwrap();
        let row = |id: &str, name: &str| LlmProfileRow {
            id: id.into(),
            name: name.into(),
            json: "{}".into(),
        };
        s.save_llm_profile_blocking(row("a", "Claude")).unwrap();
        s.save_llm_profile_blocking(row("b", "Local")).unwrap();
        let err = s.save_llm_profile_blocking(row("c", "claude")).unwrap_err();
        assert!(matches!(err, StorageError::NameTaken(_)));
        s.delete_llm_profile_blocking("a").unwrap();
        let names: Vec<_> = s
            .list_llm_profiles_blocking()
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["Local"]);
    }
}
