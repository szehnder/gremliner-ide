# Proposal

## Why

Every connection, query, schema, assistant and debugger feature needs a real Gremlin endpoint with realistic data,
and agents need to start it with one command. LocalStack Neptune needs a Pro licence we don't have (ADR-0026).

## What Changes

- `testenv/docker-compose.yml`: `tinkerpop/gremlin-server:3.7.x` on 8182 (no auth, GraphSON v3, TinkerGraph) and a second instance on 8183 with SASL PLAIN (WebSocket only) using the image's bundled credentials `stephen`/`password`.
- Seed data: TinkerPop `modern` and `crew` graphs, Kelvin Lawrence's air-routes (downloaded to `testenv/data/`, cached and gitignored), and a deterministic synthetic graph generator (`--nodes`, `--avg-degree`, `--labels`, `--seed`).
- `cargo xtask testenv up|seed [--dataset modern|crew|air-routes|synthetic]|down|status`.
- A test helper crate module (`gremlin-client` dev-dependency, `testenv::endpoint()`) that integration tests use to skip cleanly when the env is down.
- Neptune fixtures folder `testenv/fixtures/neptune/` (illustrative, written from documented formats until real responses can be recorded) (profile, explain, status, statistics summary, DescribeDBClusters) for later changes.

Out of scope: JanusGraph or other containers; LocalStack.

## Capabilities

### New Capabilities
- None. This change is developer tooling only (`skip_specs: true`).

### Modified Capabilities
- None.

## Impact

- New `testenv/` directory and `xtask` subcommands. CI Linux job gains a service step.
- Relies on ADR-0025, ADR-0026.
