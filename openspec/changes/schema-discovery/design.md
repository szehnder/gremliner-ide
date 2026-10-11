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
