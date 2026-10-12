# Design

## Context

Providers expose optional `schema_summary()` (ADR-0015). Privacy rules for what reaches the LLM are in ADR-0021
(this change only collects; the assistant decides what to send).

## Goals / Non-Goals

**Goals:**
- Labels in < 1 s on air-routes; full schema in < 5 s; never more than 3 concurrent discovery queries.

**Non-Goals:**
- Exact counts on huge graphs (approximate is fine).

## Decisions

- **Sampling queries (Gremlin Server and fallback):**
  1. `g.V().limit(S).label().dedup()` and `g.E().limit(S).label().dedup()`, then for completeness `g.V().label().dedup()` with timeout (partial if it times out).
  2. Per vertex label: `g.V().hasLabel(L).limit(S).properties().group().by(key).by(value().limit(5).fold())` → keys, types from samples.
  3. Triples: `g.E().limit(S).project('o','e','i').by(outV().label()).by(label()).by(inV().label()).dedup()`.
  4. Counts: `g.V().hasLabel(L).limit(C).count()` (capped, shown as "≥ C" when capped).
  S default 10,000; C default 100,000; timeouts 5 s each.
- **Type inference:** majority GValue type per key; mixed types reported as a union.
- **Neptune:** `GET /propertygraph/statistics/summary?mode=detailed` gives labels, edge labels, node/edge property keys per label, counts; triples still sampled (cheap).
- **Model & cache:** `Schema { discovered_at, partial: bool, vertex_labels, edge_labels, triples, props }` JSON in `schema_cache` table.
- **Consumers:** an `Entity<SchemaState>` per connection that autocomplete, the assistant and the explorer observe.

## Risks / Trade-offs

- [Sampling misses rare labels] → the "full label scan" step with timeout plus auto-refresh on unknown labels in results.

## Verification

- Integration tests on modern, crew and air-routes in testenv; fixture test for the Neptune summary.
- `cargo run -p gremliner-cli -- schema --conn Local --json`.
- Headless UI test for the schema tree and sample query action.

## As built

- **Crates.** `schema` (core) holds the model (`model.rs`), the sampling driver (`discover.rs`) over a
  `QueryRunner` trait (bounded with `futures::buffer_unordered`, no tokio), the Neptune summary parser
  (`summary.rs`) and the schema-as-graph elements (`graph.rs`). `providers::schema` implements the runner
  on the Gremlin client, adds `Session::schema_summary()` (always `None` until the Neptune provider lands)
  and cache helpers. `storage` gained migration 5, `schema_cache(connection_id, discovered_at, json)`,
  cleared when a connection is deleted.
- **Queries.** As designed, except that triples are sampled per edge label
  (`g.E().hasLabel(L).limit(S).project('o','i')…dedup()`). A single global edge sample missed `contains`
  on air-routes, whose edges are ordered by id. Vertex properties and the "multi" cardinality hint come from a single
  `fold().project('samples','multi')` query per label. The label binding is `schemaLabel` (not `label`, which
  collides with `T.label`). Server error 598 and client timeouts both count as timeouts.
- **Summary path.** Labels, property keys and vertex counts come from `nodeStructures`. Neptune's
  `edgeStructures` carry no edge label, so edge properties, edge counts, types, samples and triples are
  still sampled.
- **Measured** (testenv, air-routes): labels after about 19 ms, full schema in about 190 ms. The budgets are
  1 s and 5 s.
- **App.** `ui_workspace::schema::SchemaRegistry` is a global entity that observes the connection manager.
  On connect it shows the cached schema (`from_cache`), then refreshes on the I/O runtime. Snapshots are
  throttled to 10 per second, and the cache is kept until the labels are found. It also refreshes when
  results contain unknown labels, at most every 30 s. Autocomplete and the assistant will read
  `SchemaRegistry::schema(connection)`.
- **Sidebar.** Under each connected connection there is a Schema header with its age or a
  "Discovering…" status, plus Refresh and Schema graph buttons. Below it come the vertex and edge label
  groups with counts, and labels expand to show `key Type · multi e.g. …`. Hovering a label reveals
  "Sample 25", which dispatches `OpenSampleQuery` and opens and runs a new tab bound to that connection.
- **Schema graph.** `SchemaGraphPanel` is a centre tab that restores from saved layouts with its connection
  id. It hosts a `GraphView` in the new offline mode, which emits no follow-up fetches, has no editing and
  shows a trimmed context menu. The graph is re-ingested only when the elements change.
- **CLI.** `gremliner-cli schema [--conn NAME | --url … --alias …] [--json] [--sample N] [--timeout-ms N]`.
  With `--conn` it also updates that connection's cache.
- **Palette.** "Open Schema Graph" and "Refresh Schema" act on the active connection.
