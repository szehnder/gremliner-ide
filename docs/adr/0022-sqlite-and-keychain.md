# 0022. SQLite for application data; OS keychain for secrets

- Status: accepted
- Date: 2026-10-11

## Context
We persist connections, query history (potentially 100k entries, searchable), saved queries/folders, stylesheets,
open tabs, layouts and settings. Secrets (DB passwords, LLM API keys) must not be stored in plaintext.

## Decision
One SQLite database (`rusqlite`, bundled, WAL mode) in the platform data dir (`directories`), with embedded,
numbered migrations. Secrets are stored in the OS keychain via `keyring` (Secret Service on Linux, Keychain on
macOS, Credential Manager on Windows); the DB stores only a secret reference. If no keychain is available,
fall back to an encrypted file with a warning.

## Alternatives considered
- **JSON/TOML files** — fine for settings, poor for history search and concurrent writes.
- **sled/redb** — no SQL/FTS for history search.

## Consequences
- History full-text search via SQLite FTS5.
- `storage` runs DB work on a dedicated thread; never on the UI thread.

## Revisit when
Team/shared workspaces need sync (then a separate sync ADR).
