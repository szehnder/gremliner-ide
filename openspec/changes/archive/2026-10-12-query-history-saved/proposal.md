# Proposal

## Why

Users constantly return to earlier queries. A searchable history and organised saved queries (with their settings)
are basic IDE hygiene that G.V() and Compass both provide.

## What Changes

- Automatic history of every executed query: text, connection, time, duration, outcome, result count; searchable (full-text), filterable by connection and outcome; re-open or re-run from history. Retention setting (default 10,000 entries).
- Saved queries: save the tab (Ctrl/Cmd+S) with a name into folders (nested) under a connection or globally, including tab settings (timeout, fetch-edges, default view, parameters); dirty indicator; rename, move by drag, delete.
- Sidebar sections "Saved" and "History", plus a history palette (Ctrl/Cmd+Shift+H).

Out of scope: sharing/syncing saved queries (later), dashboards.

## Capabilities

### New Capabilities
- `query-history`: automatic, searchable record of executed queries.
- `saved-queries`: named, organised queries saved with their settings.

### Modified Capabilities
- None.

## Impact

- Crates: `storage` (history with FTS5, saved_queries, folders), `ui-workspace` (sidebar sections, palette).
- ADRs: 0022.
