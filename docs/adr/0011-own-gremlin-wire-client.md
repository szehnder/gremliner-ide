# 0011. Implement our own Gremlin wire client

- Status: accepted
- Date: 2026-10-11

## Context
The only Rust driver, `gremlin-client` 0.8.10, was last released May 2024, is tested against TinkerPop 3.4, has no
SigV4 hook and limited streaming/cancellation control. We need: streamed partial responses (status 206) for
progressive rendering, per-request timeouts and cancellation, request timing, handshake signing (Neptune IAM),
auth challenge (SASL PLAIN, status 407) and connection pooling.

## Decision
`gremlin-client` implements the TinkerPop WebSocket Server Protocol directly on tokio-tungstenite with rustls:
request messages `{requestId, op:"eval", processor:"", args:{gremlin, bindings, language:"gremlin-groovy",
evaluationTimeout}}` sent as `application/vnd.gremlin-v3.0+json` with the MIME-length prefix, response
multiplexing by requestId, a `HandshakeSigner` trait for custom headers, and a small pool.

## Alternatives considered
- **Fork gremlin-client** — similar effort to rewrite, inherits design constraints.
- **HTTP `/gremlin` POST only** — no streaming; Neptune supports it but Gremlin Server's HTTP channel is often disabled.

## Consequences
- We own protocol conformance; covered by integration tests against gremlin-server 3.7 in Docker.

## Revisit when
TinkerPop 4's HTTP-based protocol becomes the norm (add an HTTP transport behind the same trait).
