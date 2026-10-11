# Proposal

## Why

Gremlin databases are schemaless, but users (and the assistant) need to know which labels, properties, types and
connections exist. Schema knowledge powers autocomplete, the assistant's grounding, the graph explorer and validation.

## What Changes

- Schema model: vertex labels, edge labels, connectivity triples `(outLabel)-[edgeLabel]->(inLabel)`, property keys per label with inferred types, cardinality hints, approximate counts, and a few sample values per property.
- Discovery strategies: Neptune statistics/summary API when available (fast), otherwise sampled Gremlin queries with configurable sample size and timeouts; incremental (labels first, then properties, then triples).
- Background refresh on connect (cached per connection with timestamp), manual refresh, and auto-refresh when results show unknown labels.
- Schema browser in the sidebar under each connection (labels → properties with types and counts), and a schema graph view (labels as nodes, edge labels as links).
- `gremliner-cli schema --conn <name> [--json]`.

Out of scope: schema editing/management APIs (JanusGraph later).

## Capabilities

### New Capabilities
- `schema-discovery`: discovering, caching and presenting the effective schema of a connected graph.

### Modified Capabilities
- None.

## Impact

- Crates: `schema`, `providers` (schema_summary hook), `storage` (schema cache), `ui-workspace` (sidebar tree, schema graph), `gremliner-cli`.
- ADRs: 0015, 0021.
