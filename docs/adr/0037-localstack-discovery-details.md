# 0037. LocalStack Neptune: discovery details and the engine version table

- Status: accepted
- Date: 2026-10-12
- Amends: [0017](0017-localstack-neptune-discovery.md)

## Context
ADR-0017 chose `DescribeDBClusters` discovery. Two things were left open: how to call RDS without the AWS
SDK's RDS client, and where the engine → TinkerPop mapping lives.

## Decision
- **Discovery** is one SigV4-signed (service `rds`, the connection's test keys) Query-protocol POST to the
  LocalStack edge. The XML reply is parsed with `quick-xml` (already in the dependency tree), keeping only
  `Engine=neptune` clusters. A saved connection stores the picked cluster's id and looks its endpoint and
  port up again on every connect, because LocalStack assigns ports at start. "Enter host and port" skips
  discovery entirely.
- **Errors** are classified from the reply:
  - refused or timed out → LocalStack not running;
  - 501, "pro feature", "not yet implemented" or `InvalidAction` → Neptune not in this edition;
  - an empty list → no clusters, with the `awslocal neptune create-db-cluster` command.
- **Engine versions:** `providers/src/neptune/versions.toml` maps an engine release to its TinkerPop
  version. A version maps to the greatest entry at or below it. The table lists only releases whose
  TinkerPop version is stated in Neptune's release notes. It is used whenever `/status` is missing or says
  nothing (LocalStack, older engines).
- The LocalStack provider reuses the Neptune transport (`NeptuneProvider`) with static test credentials.
  Plain `ws://` is the default.
- **Test credentials** (`test`/`test` by default) are stored in the connection settings, not the keychain,
  because LocalStack does not check them and they are not secrets.

## Consequences
- Engine releases newer than the table map to its newest entry. Add a row when Neptune ships a release
  with a newer TinkerPop.
- The fake LocalStack edge and fake Neptune live in `providers::testing` (feature `testing`) so UI tests
  can use them too.

## Revisit when
A Pro licence is available for an opt-in CI job, or LocalStack changes its Neptune response format.
