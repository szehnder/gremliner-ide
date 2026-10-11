# 0026. TinkerPop gremlin-server in Docker as the test environment and Neptune stand-in

- Status: accepted
- Date: 2026-10-11

## Context
CI needs a real Gremlin endpoint. LocalStack Neptune requires a Pro licence we do not have; real Neptune costs money.

## Decision
`testenv/docker-compose.yml` runs `tinkerpop/gremlin-server:3.7.x` (TinkerGraph, GraphSON v3, script eval on)
on port 8182, seeded by `cargo xtask testenv seed` with: TinkerPop `modern` and `crew`, Kelvin Lawrence's
air-routes (~3.7k vertices / ~57k edges, downloaded and cached), and a synthetic generator (configurable up to
100k nodes) for performance tests. A second service runs gremlin-server with SASL auth to test credentials.
Neptune-specific behaviour (profile/explain HTTP, statistics API, SigV4) is tested against fixture responses (illustrative until recorded ones exist) and a
small mock HTTP server.

## Alternatives considered
- **JanusGraph container** — heavier; added with the JanusGraph provider.

## Consequences
- Neptune dialect differences (no lambdas, different `profile` format) are covered by fixtures + manual checklist.

## Revisit when
A LocalStack Pro licence or a dev Neptune cluster becomes available.
