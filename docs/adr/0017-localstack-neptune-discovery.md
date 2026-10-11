# 0017. LocalStack Neptune endpoint discovery via DescribeDBClusters

- Status: accepted
- Date: 2026-10-11

## Context
LocalStack (Pro) emulates Neptune by starting a TinkerPop server per cluster on a dynamic port (e.g. 4510).
TinkerPop version depends on the engine version (1.1.0.0 → 3.4.11 … 1.2.1.0 → 3.6.2). With IAM enabled, unsigned
requests are rejected. The maintainer currently has no Pro licence.

## Decision
The LocalStack provider takes the LocalStack edge URL (default `http://localhost:4566`) and region, lists clusters
via `DescribeDBClusters` (unsigned-friendly dummy creds `test/test`), lets the user pick one, and connects to its
`Endpoint:Port` over `ws`/`wss`. IAM toggle reuses ADR-0016 signing with dummy creds. Engine version → TinkerPop
version is mapped to set capabilities.

## Alternatives considered
- **Manual host/port entry only** — supported as an override, but discovery is the nice path.

## Consequences
- CI cannot exercise real LocalStack Neptune (ADR-0026); discovery parsing is unit-tested from recorded XML/JSON
  responses, and `docs/manual-tests/localstack-neptune.md` is the checklist.

## Revisit when
A Pro licence is available — add an opt-in CI job.
