# 0036. Neptune transport: ring-only AWS stack, tunnels, HTTP features and the restriction table

- Status: accepted
- Date: 2026-10-12
- Amends: [0016](0016-neptune-iam-sigv4.md) (how the SDK is configured, and how tunnels work)

## Context
ADR-0016 chose aws-config and aws-sigv4. Their default features build `aws-lc-rs`, a C/CMake library, next
to the `ring` stack the rest of the app already uses for rustls. Neptune is usually reached through an
SSH/SSM tunnel, so the socket goes to `localhost` while SigV4, the Host header and TLS (SNI and certificate
name) must all use the cluster's name. Neptune's HTTP endpoints (status, profile, explain, statistics)
also need a home in the provider abstraction, and its Gremlin restrictions are needed by both the editor
and the assistant.

## Decision
- **One crypto stack.** aws-config is built with `default-features = false` (`rt-tokio`, `sso`,
  `credentials-process`), and its HTTP client is `aws-smithy-http-client` with `rustls-ring`, passed in
  explicitly. Nothing in the tree builds aws-lc.
- **Credentials.** With a profile, `ProfileFileCredentialsProvider` for that profile only, so environment
  keys never override a choice the user made; without one, the default chain. Each connection keeps a
  `CredentialCache` that refreshes 5 minutes before expiry. Each handshake re-signs from the cache, so a
  reconnect after expiry uses fresh credentials. Credentials are resolved before the first handshake, so an
  expired SSO session is reported as such (`FailureKind::AuthExpired`), not as a refused upgrade.
- **Tunnels.** `ClientConfig::connect_addr` opens the TCP socket to the tunnel, then runs the TLS and
  WebSocket handshakes over it with the endpoint's URL, so Host, SNI and certificate checks name the
  cluster. HTTP calls use reqwest `resolve` with the tunnel's port in the URL and an explicit Host header.
  Both are signed for the endpoint's URL.
- **HTTP features** sit behind a `ServerApi` trait on `Session` (`statistics_summary`, `profile`,
  `explain`). It is present only when `/status` answered as Neptune.
- **Restrictions** (no lambdas, unsupported steps, and steps newer than the server's TinkerPop version)
  are a table in `gremlin-syntax` (`compliance.toml`), a core crate both the editor and the assistant
  already depend on. Providers pick the table for their connection.

## Alternatives considered
- **Default aws-config features**: adds a CMake build and a second TLS crypto library.
- **Port forwarding by rewriting the URL to `localhost`**: SigV4 and SNI would then name `localhost`, and
  Neptune rejects the signature.
- **Restrictions in `providers`**: the assistant would have to depend on the AWS SDK to know which steps
  Neptune rejects.

## Consequences
- AWS features that need aws-lc (FIPS) are not available.
- Tests use a fake Neptune that verifies SigV4 independently, serves `/status` and speaks Gremlin over a
  WebSocket, with or without TLS. Real clusters are covered by `docs/manual-tests/neptune-aws.md`.

## Revisit when
The app needs FIPS crypto, or AWS ships an official Rust Neptune data client.
