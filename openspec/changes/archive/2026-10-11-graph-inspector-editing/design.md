# Design

## Context

Selection comes from the canvas; elements have property side-tables (graph-view). Mutations are user-initiated
(ADR-0020 forbids only LLM-initiated mutations). Bindings per ADR-0030. See proposal.md.

## Goals / Non-Goals

**Goals:**
- Generated statements are readable and portable across Gremlin Server and Neptune.

**Non-Goals:**
- Transactions across several edits (each edit is one statement).

## Decisions

- **Edit commands:** `enum EditCommand { SetProperty{el, key, value, cardinality}, RemoveProperty{..}, AddVertex{label, props},
  AddEdge{from, to, label, props}, DropElements{ids} }` → `fn to_statement(&self) -> (String, Bindings)` e.g.
  `g.V(vid).property(single, k, v)` with bindings `{vid, k, v}`; inverse computed from the pre-edit snapshot.
- **Id typing:** ids are bound with their original GValue type (Neptune string ids vs TinkerGraph longs).
- **Undo stack:** per tab, holding `(command, inverse)`; cleared on connection change.
- **Edge gesture:** Alt+drag from a node draws a rubber-band line in the canvas; drop on a node opens the label picker (labels from the schema when available).
- **Apply:** after server success, patch the store locally; on failure show the error and leave the store unchanged.

## Risks / Trade-offs

- [Meta-property and multi-property semantics differ by provider (Neptune: set cardinality default)] → cardinality explicit in the property editor; default from provider capabilities.

## Verification

- Unit tests for statement generation and inverses (insta snapshots).
- Integration tests on a scratch graph (`gSynth` reset per test) for each scenario.
- Headless UI tests for confirmation flows.

## As built

- **Commands:** `gremlin_model::edits::EditCommand` (set/remove property, add vertex, add edge, drop) →
  `Statement { script, bindings }`, snapshot-tested. Binding names are `elementId`, `propKey`,
  `propValue`, `vertexLabel`, `edgeLabel`, `fromId`, `toId`, `vids`, `eids`: **`id`, `key`, `value` and
  `label` must not be used**, because Gremlin Server statically imports `T.*`, so the script resolved
  them to the tokens and the edits silently did nothing (found by the end-to-end tests). Drops of several
  elements bind a list (`g.V(vids).drop()`), which TinkerPop 3.7 unrolls.
- **Inverses:** set/remove restore the previous first value (single cardinality) or remove a property that
  did not exist; creations are undone by dropping the id the server returned; deletes are not undoable.
  When the previous value is unknown (details not loaded), no undo entry is recorded.
- **Flow:** the graph view shows an in-view preview card (description, counts for deletes, the statement
  with typed bindings, Cancel/Run) and emits `GraphViewEvent::Mutate` on Run. The `Runner` adds an alert
  naming the connection for production and read-only connections, runs the statement with bindings on the
  tab's connection, and calls `apply_mutation`. On success the store and the typed details are patched
  locally (properties, new elements merged and placed incrementally, deleted elements removed with index
  compaction); on failure an error card shows and nothing changes.
- **Delete counts** come from the server (`g.V(ids).bothE().dedup().id()`), since the graph may show only
  some incident edges; Run is disabled until they arrive.
- **Inspector:** `ui_results::inspector::Inspector` in the right dock follows the latest graph: kind, label,
  id, in/out degree (a `project()` query per inspected vertex), typed properties with multi-properties and
  meta-properties, inline edit with a type picker, add property with cardinality, remove, delete, and edge
  endpoints that select and centre the vertex. Edges are selectable by clicking near them.
- **Gestures and keys:** Alt-drag from node to node draws a link line and asks for the edge label;
  "New vertex…" (context menu, inspector, palette) takes a label and `key=value; …` properties with
  inferred types. Undo/redo buttons in the toolbar, Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z; Shift+Delete deletes
  from the database (Delete still only hides).
- **Deferred:** edge labels from the schema (schema-discovery); the label prompt is free text.
