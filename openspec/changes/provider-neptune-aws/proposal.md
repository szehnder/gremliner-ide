# Proposal

## Why

Amazon Neptune is a primary target. Connecting to it means IAM SigV4 authentication from the user's normal AWS
credentials (profiles, SSO), cluster-aware endpoints, and Neptune-specific features (profile/explain APIs, status,
statistics) that G.V() exposes and we should too.

## What Changes

- Neptune (AWS) provider: cluster endpoint or reader endpoint, port (8182), region (auto from hostname), IAM auth
  on/off, AWS profile picker (from `~/.aws/config` and credentials, SSO supported), optional "connect via localhost
  tunnel" mode with a host override for SigV4 and TLS SNI.
- SigV4 signing of the WebSocket handshake and of HTTP calls (ADR-0016), with automatic credential refresh and clear
  messages for expired SSO sessions ("Run `aws sso login --profile X`").
- Capabilities: Neptune engine version from `/status`, Neptune dialect restrictions (no lambdas, unsupported steps list), profile/explain via HTTP endpoints, statistics summary for schema-discovery.
- Neptune extras in the result summary: server-side timing when present, and links to slow-query and audit log guidance.

Out of scope: SSH/SSM tunnel management (later change), Neptune Analytics (openCypher only), bulk loader.

## Capabilities

### New Capabilities
- `provider-neptune-aws`: connecting to Amazon Neptune clusters with IAM authentication and Neptune-specific features.

### Modified Capabilities
- None.

## Impact

- Crates: `providers` (neptune module), `gremlin-client` (signer usage), `ui-workspace` (Neptune form section).
- Dependencies: aws-config, aws-sigv4, aws-credential-types.
- ADRs: 0015, 0016, 0026.
