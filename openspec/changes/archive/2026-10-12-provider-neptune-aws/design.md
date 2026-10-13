# Design

## Context

ADR-0016 (aws-config + aws-sigv4 on handshake), `gremlin-client` exposes `HandshakeSigner`. No real Neptune in CI
(ADR-0026): tests use a local signature verifier and recorded responses in `testenv/fixtures/neptune/`.

## Goals / Non-Goals

**Goals:**
- Correct SigV4 for WebSocket and HTTP with temporary credentials.

**Non-Goals:**
- Managing tunnels (later `ssh-ssm-tunnels`).

## Decisions

- **Credentials:** `aws_config::defaults(BehaviorVersion::latest()).profile_name(p).region(r).load()`; the
  `SharedCredentialsProvider` is cached per connection; SSO token expiry errors mapped to a typed `AuthExpired{profile}`.
- **Signing:** build `http::Request` for `GET https://{signing_host}:{port}/gremlin` (scheme https even for wss),
  sign with `aws_sigv4::http_request::sign` (service `neptune-db`, region), copy resulting headers onto the WebSocket upgrade request.
- **Host override:** connect to `tunnel_addr`, set `Host` = signing host, TLS SNI = signing host, with optional "accept invalid certs" for tunnel setups.
- **HTTP endpoints:** `reqwest` with the same signer for `/status`, `/gremlin/profile`, `/gremlin/explain`, `/propertygraph/statistics/summary`.
- **Capabilities table:** engine version → TinkerPop version → unsupported steps (from Neptune Gremlin compliance docs, stored in `providers/neptune/compliance.toml`).
- **Signature test:** implement a minimal SigV4 verifier in test code (recompute the signature with the known test key) to check signed headers without AWS.

## Risks / Trade-offs

- [No live Neptune in CI] → manual checklist `docs/manual-tests/neptune-aws.md` (connect with IAM via profile, run a query, profile, explain, schema summary).

## Verification

- `cargo test -p providers neptune` (region parsing, profile listing from fixture AWS config, signing verifier, status parsing, compliance checks).
- Manual checklist against a real cluster when available.
