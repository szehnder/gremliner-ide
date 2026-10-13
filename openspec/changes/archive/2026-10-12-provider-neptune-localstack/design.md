# Design

## Context

ADR-0017 (DescribeDBClusters discovery) and ADR-0016 (signing). LocalStack Neptune needs Pro; CI uses recorded
responses (ADR-0026).

## Goals / Non-Goals

**Goals:**
- Discovery via a single signed (dummy creds) Query-protocol call; no AWS SDK RDS dependency needed.

**Non-Goals:**
- Live LocalStack tests in CI.

## Decisions

- **Discovery call:** POST `{localstack}/` with `Action=DescribeDBClusters&Version=2014-10-31&Filters...Engine=neptune`,
  SigV4-signed (service `rds`, test creds); parse XML with `quick-xml` (small new dep). Endpoint and Port from each `DBCluster`.
- **Error classification:** connection refused → NotRunning; `InvalidAction`/"not included in your current license" → NotAvailable; empty list → NoClusters.
- **Mapping table:** `engine_version → tinkerpop_version` in `providers/neptune/versions.toml`, shared with the AWS provider.
- **Connection:** reuse the Gremlin transport; TLS off by default (LocalStack serves ws), toggleable.

## Risks / Trade-offs

- [LocalStack response format changes] → fixture update + manual checklist.

## Verification

- `cargo test -p providers localstack` with fixtures in `testenv/fixtures/neptune/`.
- `docs/manual-tests/localstack-neptune.md` checklist (Pro required).
