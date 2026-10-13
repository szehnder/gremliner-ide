//! Embedded, numbered schema migrations. `PRAGMA user_version` records the last one applied.
//!
//! Never edit a released migration; append a new one.

use rusqlite::Connection;

use crate::Result;

const MIGRATIONS: &[&str] = &[
    // 1: key/value tables for settings and UI state.
    "CREATE TABLE settings (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE ui_state (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );",
    // 2: saved connections. `config` is the provider-specific JSON; secrets are never stored here.
    "CREATE TABLE connections (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        config TEXT NOT NULL,
        position INTEGER NOT NULL DEFAULT 0,
        group_id TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );",
    // 3: open editor tabs (content, cursor, bound connection), restored on startup.
    "CREATE TABLE open_tabs (
        id TEXT PRIMARY KEY NOT NULL,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        cursor INTEGER NOT NULL DEFAULT 0,
        connection_id TEXT,
        updated_at INTEGER NOT NULL
    );",
    // 4: graph stylesheets (JSON documents, schema_version inside).
    "CREATE TABLE stylesheets (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        json TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );",
    // 5: the last discovered schema per connection (JSON, schema_version inside).
    "CREATE TABLE schema_cache (
        connection_id TEXT PRIMARY KEY NOT NULL,
        discovered_at INTEGER NOT NULL,
        json TEXT NOT NULL
    );",
    // 6: LLM profiles (JSON documents; API keys live in the credential store).
    "CREATE TABLE llm_profiles (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        json TEXT NOT NULL,
        position INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
    );",
    // 7: query history with a trigram full-text index for substring search.
    "CREATE TABLE history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        connection_id TEXT,
        connection_name TEXT NOT NULL,
        text TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL,
        outcome TEXT NOT NULL,
        count INTEGER NOT NULL,
        error TEXT
    );
    CREATE INDEX history_started ON history(started_at);
    CREATE VIRTUAL TABLE history_fts USING fts5(text, content='history', content_rowid='id', tokenize='trigram');
    CREATE TRIGGER history_ai AFTER INSERT ON history BEGIN
        INSERT INTO history_fts(rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER history_ad AFTER DELETE ON history BEGIN
        INSERT INTO history_fts(history_fts, rowid, text) VALUES ('delete', old.id, old.text);
    END;",
    // 8: saved queries in nested folders, and the saved query a tab was opened from (with tab settings).
    "CREATE TABLE folders (
        id TEXT PRIMARY KEY NOT NULL,
        parent_id TEXT,
        connection_id TEXT,
        name TEXT NOT NULL,
        sort INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE saved_queries (
        id TEXT PRIMARY KEY NOT NULL,
        folder_id TEXT,
        connection_id TEXT,
        name TEXT NOT NULL,
        text TEXT NOT NULL,
        settings TEXT NOT NULL DEFAULT '{}',
        updated_at INTEGER NOT NULL
    );
    ALTER TABLE open_tabs ADD COLUMN saved_query_id TEXT;
    ALTER TABLE open_tabs ADD COLUMN settings TEXT;",
];

/// The schema version after every migration has run.
#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::cast_possible_wrap)]
pub const LATEST: i64 = MIGRATIONS.len() as i64;

pub fn apply(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        #[allow(clippy::cast_possible_wrap)]
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        log::info!("storage: applied migration {version}");
    }
    Ok(())
}
