# 0016. Neptune IAM auth via aws-config and aws-sigv4 on the WebSocket handshake

- Status: accepted
- Date: 2026-10-11

## Context
Neptune with IAM auth requires SigV4-signed requests (service `neptune-db`). For WebSockets, the HTTP upgrade
request must carry `Authorization`, `X-Amz-Date`, `Host` and (for temporary creds) `X-Amz-Security-Token`.

## Decision
Resolve credentials with `aws-config` (default chain: env, profiles incl. SSO, IMDS) honouring a per-connection
profile and region; sign the upgrade `GET wss://host:port/gremlin` with `aws-sigv4` via the client's
`HandshakeSigner` hook. Re-sign on every (re)connect; credentials refresh automatically. HTTP endpoints
(`/gremlin/profile`, `/gremlin/explain`, `/status`, `/propertygraph/statistics/summary`) are signed the same way.

## Alternatives considered
- **Shelling out to the AWS CLI** — slow, fragile.
- **aws-sdk-neptunedata** — covers HTTP data API but not WebSocket streaming; may be used for status/summary later.

## Consequences
- Neptune is typically VPC-only; users connect through a tunnel (SSH/SSM is a later change); the TLS host header
  override setting lets signing use the cluster hostname while connecting to `localhost`.

## Revisit when
AWS ships an official Rust Gremlin/Neptune driver.
