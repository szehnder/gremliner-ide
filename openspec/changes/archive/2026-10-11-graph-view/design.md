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

## As built

- **Model:** `gremlin_model::elements` extracts the distinct vertices and edges of any result (top level,
  paths, lists, map keys/values, trees, traversers, and `elementMap()` rows). `graph_store::GraphStore::merge`
  takes a neutral `GraphDelta` (the store does not depend on `gremlin-model`; `ui-results` converts) and
  returns a `MergeReport`. Edge endpoints that were not returned become placeholder nodes (drawn faded);
  so do vertices returned without properties (Gremlin Server's `path()`/`select()` vertices), and the view
  fetches them with `g.V(ids)` in batches of 500 so captions and properties fill in.
- **Placement** (until graph-layouts): existing nodes never move. New nodes adjacent to existing ones go on
  compact concentric rings around them, on an arc facing away from their existing neighbours; other
  components are laid out as trees around their best-connected node and packed on a spiral outside the
  existing graph; unconnected nodes form a sunflower disc. Streaming 57k edges among 3.7k vertices merges
  in ~0.35 s in total.
- **Visibility:** `flags::USER_HIDDEN` (Delete), hidden vertex/edge labels and one property `Filter`
  (label + key + op + value, numeric or case-insensitive text) fold into the effective `flags::HIDDEN` on
  nodes and the new per-edge flags, which scene building, rasterising and hit-testing already honour.
  Filters run synchronously (an O(n) pass; fast enough at our sizes), not on the background executor.
- **Follow-ups:** `GraphView` (in `ui-results`) emits `GraphViewEvent::Fetch` with a ready script; the
  `Runner` runs it on the tab's connection (`collect`, up to 20k items) and calls `apply_fetch`. Scripts
  use typed id literals (`1`, `1L`, `'a'`) rather than bindings so they also work on Neptune. Results for a
  cleared graph (older generation) are dropped. Edges between: `g.V(chunk).outE().where(inV().hasId(within(all)))`
  after vertex-only results, skipped above 2,000 vertices with a "Fetch edges" button.
- **Expand:** `g.V(id).bothE(label?).range(o, o+cap+1).as('e').otherV().as('v').select('e','v')` (out/in
  variants); `cap + 1` rows mean more exist, and a lazy `count()` fills the "100 of N neighbours shown"
  banner with Load more.
- **UI:** toolbar (fit, zoom, Clear/Accumulate, Edges between, Legend, Filter, Minimap, find box with
  match counter, Fetching… spinner, counts), legend overlay with per-label toggles, filter panel, context
  menu (expand both/out/in by edge label, select neighbours, hide, pin/unpin, copy id, show in table,
  select all, unhide all, fit), actions with palette entries (`When::GraphFocused`) and keys (Delete/
  Backspace hide, Ctrl/Cmd+A select all, N neighbours, E expand, F fit, Ctrl/Cmd+Shift+H unhide all,
  Ctrl/Cmd+F find). Find animates the camera (320 ms ease-out) and selects the match. The canvas follows
  the light/dark theme.
- **Default view:** element and path results now open in the Graph view (query-execution spec delta).
- **Deferred:** the layout and stylesheet pickers arrive with graph-layouts and graph-stylesheets;
  PNG/SVG export with `export`; "search path from here" with graph-explorer. Expanding by edge label offers
  the labels already in the graph (schema-based labels come with schema-discovery).
