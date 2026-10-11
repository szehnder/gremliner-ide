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

## As built

- **Pipeline:** `ui_workspace::run::Runner` handles `EditorTabsEvent::RunRequested`: read-only guard
  (alert with "Run anyway" for one execution), `gremlin_syntax::ensure_terminal`, connect if needed, then
  stream on the I/O runtime into an `Entity<ResultSet>` (`ui-results`). One run at a time; a new run
  cancels the previous one. Results are kept per tab; the dock shows the latest run and opens itself.
- **Cancel:** Esc (only while running, via the `QueryRunning` key context) or the Cancel button. The result
  is marked Cancelled immediately and further batches are ignored (measured < 200 ms). Gremlin Server keeps
  evaluating until its timeout; see `docs/gpui-notes.md`.
- **Cap:** `GeneralSettings::result_limit` (default 10,000, max 1,000,000; Settings → General). Reaching it
  stops streaming and marks the result truncated.
- **Normalisation:** `gremlin_model::normalize` (shape, preferred view, table model with numeric sort keys,
  lazy tree rows, CSV/TSV) and `gremlin_model::console` (Gremlin Console text). No `csv` crate: RFC 4180
  quoting is a few lines. `regex` is used for console search and error positions.
- **Views:** Table (gpui-kit `DataTable`, our own sort headers, quick filter, CSV/JSON export via a save
  dialog), JSON (`uniform_list`, click to expand, copy value / copy path, expand/collapse all), Console
  (console text or pretty-printed raw GraphSON, text/regex search with highlights and next/previous).
  Views rebuild adaptively while streaming (see notes). Raw frames are kept up to 5 MB.
- **Errors:** server message, code and exception class; Groovy `line N, column M` positions are framed in the
  editor. The "Fix with assistant" action is a placeholder line until `assistant-panel`.
- **Server time:** Gremlin Server does not report it for `eval`; the summary shows client time.
- **Measured (headless harness):** 100k-row table scroll worst frame 3.3 ms (budget 16 ms); 1M-node JSON
  result interactive after 94 ms (budget 500 ms); 57k edges stream end to end in about a second.
- `--run "<gremlin>"` (with `--open-connection`) opens a tab and runs it after the first frame.
