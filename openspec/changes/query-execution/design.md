# Design

## Context

The editor emits `RunRequested` (query-editor). `gremlin-client` streams `Vec<GValue>` batches with optional raw
JSON. gpui-kit offers a virtual `Table` and `List`. See proposal.md; ADR-0005 and 0020.

## Goals / Non-Goals

**Goals:**
- One normalisation pass shared by every results view (and later the graph view).

**Non-Goals:**
- Editing values in the table (graph-inspector-editing).

## Decisions

- **Run pipeline (`ui-workspace::run`):** analyse → guard (read-only) → `ensure_terminal` (via `gremlin-syntax`) →
  `spawn_io(client.execute_stream)` → batches appended to an `Entity<ResultSet>` on the foreground → views observe it.
  A `RunId` prevents stale results from a previous run landing in the new view.
- **ResultSet:** `items: Vec<GValue>`, `raw: Vec<Bytes>` (bounded), `shape: ResultShape`, `status`, timings,
  `truncated`. Normalised artefacts computed lazily and cached on the background executor: `TableModel`
  (per-label groups, column union, typed cells), `TreeModel` (flattened nodes with depth for lazy rendering), `ElementSet`.
- **Server time:** Gremlin Server includes timing in some status attributes; when absent, show client time only.
- **Table:** gpui-kit `Table` with a `TableDelegate` backed by `TableModel`; sorting runs on the background executor for > 10k rows.
- **Console formatting:** `gremlin-model::console::format(&GValue)` mirroring Gremlin Console `toString` conventions.
- **Export:** CSV via `csv` crate (new small dep); JSON via serde with GraphSON-free "plain" JSON mapping.

## Risks / Trade-offs

- [Massive results exhaust memory] → result cap (default 10k, max 1M) and raw-response retention limit (5 MB).

## Verification

- Unit tests for `ensure_terminal`, normalisation, console formatting (insta).
- Integration tests against testenv for streaming, cancel, cap, errors.
- Headless UI tests for table sort/filter, JSON expand, console search.
- Benchmarks: `table_scroll_100k`, `json_tree_open_1m`.
