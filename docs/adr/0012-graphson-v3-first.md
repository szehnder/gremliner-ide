# 0012. GraphSON v3 first, GraphBinary later

- Status: accepted
- Date: 2026-10-11

## Context
Gremlin Server and Neptune support GraphSON v3 and GraphBinary. GraphSON is text, debuggable and lets us show the
raw response in the JSON view; GraphBinary is ~2–3× smaller/faster.

## Decision
Ship GraphSON v3 (typed `@type`/`@value`) with a hand-written `serde_json::Value` → `GValue` decoder in
`gremlin-model`. GraphBinary is a later change (`graphbinary`), selectable per connection.

## Alternatives considered
- **GraphSON v2** — needed only for Cosmos DB; added with that provider.
- **GraphBinary first** — harder to debug while the protocol layer is young.

## Consequences
- Decoder benchmarked with criterion on air-routes-sized payloads.

## Revisit when
Decoding exceeds 10% of query wall time on large results.
