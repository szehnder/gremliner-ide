# Design

## Context

ADR-0022: SQLite with FTS5 on a dedicated storage thread. Tabs already persist (query-editor). See proposal.md.

## Goals / Non-Goals

**Goals:**
- History writes never delay execution (fire-and-forget on the storage thread).

**Non-Goals:**
- Sync or sharing.

## Decisions

- **Tables:** `history(id, conn_id, text, started_at, duration_ms, outcome, count)` + `history_fts` (FTS5, content=history, trigram tokenizer for substring search); `folders(id, parent_id, conn_id NULL, name, sort)`; `saved_queries(id, folder_id, conn_id NULL, name, text, settings_json, updated_at)`.
- **Retention:** trigger or periodic `DELETE … WHERE id IN (oldest beyond limit)` after insert batches.
- **Tab linkage:** `open_tabs.saved_query_id` + content hash for dirty detection.
- **UI:** sidebar tree (gpui-kit `Tree` if available, else virtual list with indentation) with drag-and-drop; history palette reuses command palette component with a history source.

## Risks / Trade-offs

- [FTS trigram index size] → acceptable at 10k entries (< 10 MB).

## Verification

- `cargo test -p storage` (CRUD, FTS search, retention, speed test with 10k rows).
- Headless UI tests for save/reopen, drag into folder, dirty indicator, re-run.
