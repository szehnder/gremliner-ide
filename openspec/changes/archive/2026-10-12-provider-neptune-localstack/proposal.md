# Proposal

## Why

Developers build Neptune apps locally against LocalStack. Connecting should be one click: point at LocalStack, pick
a cluster, go, without hunting for the dynamic port. (The maintainer lacks Pro today, so this ships with fixture tests and a manual checklist.)

## What Changes

- LocalStack Neptune provider: LocalStack URL (default `http://localhost:4566`), region (default `us-east-1`),
  cluster picker populated via `DescribeDBClusters`, IAM toggle (signs with test credentials), manual host/port override.
- Engine version → TinkerPop version mapping for capabilities.
- Helpful errors: LocalStack not running, Pro feature not available, no clusters (with the `awslocal neptune create-db-cluster` command to create one).

Out of scope: creating clusters from the IDE.

## Capabilities

### New Capabilities
- `provider-neptune-localstack`: connecting to Neptune clusters emulated by LocalStack with automatic endpoint discovery.

### Modified Capabilities
- None.

## Impact

- Crates: `providers` (localstack module), `ui-workspace` (form section).
- ADRs: 0016, 0017, 0026.
