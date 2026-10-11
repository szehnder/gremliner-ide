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
