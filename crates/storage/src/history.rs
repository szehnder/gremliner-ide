//! The `history` table: every executed query, searchable by substring through a trigram FTS5 index.
//! Writes are fire-and-forget on the storage thread, so recording never delays a run.

use rusqlite::{Connection, params_from_iter};

use crate::{Result, Storage};

/// Entries kept by default.
pub const DEFAULT_HISTORY_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Error,
    Cancelled,
}

impl Outcome {
    pub fn id(self) -> &'static str {
        match self {
            Outcome::Success => "success",
            Outcome::Error => "error",
            Outcome::Cancelled => "cancelled",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "success" => Outcome::Success,
            "cancelled" => Outcome::Cancelled,
            _ => Outcome::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// 0 for an entry not yet written.
    pub id: i64,
    pub connection_id: Option<String>,
    pub connection_name: String,
    pub text: String,
    /// Milliseconds since the Unix epoch.
    pub started_at: i64,
    pub duration_ms: i64,
    pub outcome: Outcome,
    pub count: i64,
    pub error: Option<String>,
}

/// A history search: substring of the text, connection and outcome filters, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryQuery {
    pub text: String,
    pub connection_id: Option<String>,
    pub outcome: Option<Outcome>,
    pub limit: usize,
}

impl Storage {
    /// Record an execution, then trim the oldest entries beyond `keep`.
    pub fn record_history(
        &self,
        entry: HistoryEntry,
        keep: usize,
    ) -> impl std::future::Future<Output = Result<i64>> + Send + 'static + use<> {
        self.call(move |conn| record(conn, &entry, keep))
    }

    pub fn record_history_blocking(&self, entry: HistoryEntry, keep: usize) -> Result<i64> {
        self.call_blocking(move |conn| record(conn, &entry, keep))
    }

    pub fn search_history(
        &self,
        query: HistoryQuery,
    ) -> impl std::future::Future<Output = Result<Vec<HistoryEntry>>> + Send + 'static + use<> {
        self.call(move |conn| search(conn, &query))
    }

    pub fn search_history_blocking(&self, query: HistoryQuery) -> Result<Vec<HistoryEntry>> {
        self.call_blocking(move |conn| search(conn, &query))
    }

    pub fn history_len_blocking(&self) -> Result<i64> {
        self.call_blocking(|conn| {
            Ok(conn.query_row("SELECT count(*) FROM history", [], |r| r.get(0))?)
        })
    }

    pub fn clear_history(
        &self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static + use<> {
        self.call(|conn| {
            conn.execute_batch(
                "DELETE FROM history; INSERT INTO history_fts(history_fts) VALUES ('rebuild');",
            )?;
            Ok(())
        })
    }
}

fn record(conn: &mut Connection, e: &HistoryEntry, keep: usize) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO history (connection_id, connection_name, text, started_at, duration_ms, outcome, count, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            e.connection_id,
            e.connection_name,
            e.text,
            e.started_at,
            e.duration_ms,
            e.outcome.id(),
            e.count,
            e.error
        ],
    )?;
    let id = tx.last_insert_rowid();
    // Retention: drop the oldest beyond the limit (ids grow with time).
    tx.execute(
        "DELETE FROM history WHERE id IN (
             SELECT id FROM history ORDER BY id DESC LIMIT -1 OFFSET ?1)",
        [i64::try_from(keep.max(1)).unwrap_or(i64::MAX)],
    )?;
    tx.commit()?;
    Ok(id)
}

fn search(conn: &mut Connection, q: &HistoryQuery) -> Result<Vec<HistoryEntry>> {
    let mut sql = String::from(
        "SELECT h.id, h.connection_id, h.connection_name, h.text, h.started_at, h.duration_ms, h.outcome, h.count, h.error FROM history h",
    );
    let mut wheres: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    let needle = q.text.trim();
    if needle.chars().count() >= 3 {
        // Trigram FTS: a quoted phrase matches any substring.
        sql.push_str(" JOIN history_fts f ON f.rowid = h.id");
        wheres.push("history_fts MATCH ?".into());
        args.push(format!("\"{}\"", needle.replace('"', "\"\"")));
    } else if !needle.is_empty() {
        wheres.push("h.text LIKE ? ESCAPE '!'".into());
        let escaped = needle
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_");
        args.push(format!("%{escaped}%"));
    }
    if let Some(c) = &q.connection_id {
        wheres.push("h.connection_id = ?".into());
        args.push(c.clone());
    }
    if let Some(o) = q.outcome {
        wheres.push("h.outcome = ?".into());
        args.push(o.id().to_owned());
    }
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    let limit = if q.limit == 0 { 500 } else { q.limit };
    sql.push_str(&format!(" ORDER BY h.id DESC LIMIT {limit}"));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            let outcome: String = r.get(6)?;
            Ok(HistoryEntry {
                id: r.get(0)?,
                connection_id: r.get(1)?,
                connection_name: r.get(2)?,
                text: r.get(3)?,
                started_at: r.get(4)?,
                duration_ms: r.get(5)?,
                outcome: Outcome::from_id(&outcome),
                count: r.get(7)?,
                error: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
