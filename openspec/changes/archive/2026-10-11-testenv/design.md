# Design

## Context

Gremlin Server images ship with sample configs. Script evaluation is enabled by default in `gremlin-server.yaml`.
Air-routes is published as GraphML (`air-routes-latest.graphml`, ~3.7k vertices, ~57k edges) in Kelvin Lawrence's
`graph` repository on GitHub.

## Goals / Non-Goals

**Goals:**
- One-command, idempotent environment; seeding takes < 30 s.
- Data sets cover tiny (modern), multi-property/meta-property (crew), realistic (air-routes), and scale (synthetic up to 100k).

**Non-Goals:**
- Neptune emulation fidelity (fixtures cover Neptune specifics).

## Decisions

- **Seeding via the Gremlin Server itself**: mount `testenv/data` read-only into the container and run
  `graph.io(graphml()).readGraph('/data/air-routes.graphml')` through a script request; `modern`/`crew` via
  `TinkerFactory.generateModern(graph)`-style scripts. This exercises our own client early (`xtask` posts the script
  via HTTP `/gremlin` to avoid depending on unfinished crates).
- **Multiple graphs**: configure gremlin-server with three graph bindings (`g` → air-routes by default, `gModern`,
  `gCrew`, `gSynth`) via a custom `gremlin-server.yaml` and `empty-sample.groovy`-style init script. Tests choose the
  traversal source alias.
- **Synthetic generator** in `xtask` (Rust, seeded `rand_chacha`) writes GraphML, then loads it into `gSynth`.
- **Health check**: `xtask testenv status` sends `g.inject(1)` and reports the server version.

## Risks / Trade-offs

- [GitHub download of air-routes unavailable] → checksum-verified cache under `testenv/data`, with a clear error naming the URL.
- [Image tag drift] → pin an exact 3.7.x tag in compose.

## Verification

- `cargo xtask testenv up && cargo xtask testenv seed` then `cargo xtask testenv status` reports counts: air-routes ~3.7k V / ~57k E, modern 6 V / 6 E.
