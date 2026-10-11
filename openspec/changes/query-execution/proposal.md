# Proposal

## Why

This closes the core loop: write a query, run it, and see results quickly in the shape that fits the question
(table, JSON tree, raw console), with clear timing, counts and errors.

## What Changes

- Execution pipeline: take the editor's `RunRequested`, check read-only/mutation guard, append `.toList()` when no
  terminal step, run on the tab's connection with the tab's timeout, stream results, support Cancel (Esc / toolbar).
- Result normalisation (`gremlin-model::normalize`): classify results (elements, paths, maps, scalars, mixed) and
  produce table rows, a tree model, and (for the graph-view change) element sets.
- Results dock with tabs: **Table** (vertices/edges grouped by label, generic table for maps/scalars), **JSON**
  (lazy tree that renders only visible rows), **Console** (raw server output, searchable). Default view picked from result shape.
- Result summary bar: status, item count, elapsed time (client and server when available), "truncated" indicator
  when the result cap (default 10k items, configurable) is hit.
- Error presentation: server error message, code, a link to the offending range when parseable, and a "Fix with assistant" placeholder (wired in assistant-panel).
- Export results to CSV/JSON from table and JSON views (more formats in `export`).

Out of scope: graph view (graph-view), profile/explain (query-profile-explain), history (query-history-saved).

## Capabilities

### New Capabilities
- `query-execution`: running queries from the editor with safety checks, streaming, cancellation, limits and status reporting.
- `results-table`: tabular views of query results with grouping, sorting, filtering and export.
- `results-json`: a navigable tree view of structured query results.
- `results-console`: the raw server response with search, mirroring the Gremlin Console.

### Modified Capabilities
- None.

## Impact

- Crates: `gremlin-model` (normalize), `ui-results`, `ui-workspace`, `ui-editor`.
- ADRs: 0005, 0013, 0020.
