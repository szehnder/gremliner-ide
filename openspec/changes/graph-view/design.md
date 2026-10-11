# Design

## Context

`graph-canvas` (spike) renders a `graph-store`. `query-execution` produces an `ElementSet`. See proposal.md; ADR-0006, 0008, 0009.

## Goals / Non-Goals

**Goals:**
- Incremental merges that keep the picture stable (existing nodes keep positions; new nodes placed near neighbours).

**Non-Goals:**
- Multiple graph views per tab (one per tab in v1).

## Decisions

- **Merge:** `GraphStore::merge(ElementSet) -> MergeReport { added_nodes, added_edges, missing_endpoints }`.
  Missing endpoints → `g.V(ids...)` follow-up query (batched 500 ids). Edges-between → `g.V(ids).bothE().where(otherV().hasId(within(ids)))`
  batched; for > 2k vertices this is skipped with a notice.
- **Expand:** `g.V(id).bothE().limit(cap+1).as('e').otherV().as('v').select('e','v')` (or direction/label variants);
  `cap+1` detects "more". Paging uses `range(offset, offset+cap)`. Counts via `g.V(id).bothE().count()` lazily for the "of N" text.
- **Placement:** new nodes start at parent position + small jitter; the layout worker runs an incremental pass (graph-layouts) with existing nodes temporarily heavier.
- **Visibility:** `flags.hidden` per element + label visibility set + predicate filter compiled to a closure over property side-table; the canvas skips hidden elements; the spatial grid excludes them.
- **Property side-table:** `HashMap<NodeIx, SmallVec<(PropKey, GValue)>>` with interned keys; filters evaluate on the background executor.
- **Context menu:** gpui-kit `ContextMenu`; actions are GPUI actions so they appear in the command palette when the graph is focused.

## Risks / Trade-offs

- [Edges-between query costly on big sets] → threshold + explicit "Fetch edges between" button.

## Verification

- Unit tests for merge/visibility/filter logic in `graph-store`.
- Integration tests against testenv for expand, edges-between, missing endpoints.
- Headless UI tests for context menu actions, legend toggles and search focus.
- Screenshot of an air-routes neighbourhood.
