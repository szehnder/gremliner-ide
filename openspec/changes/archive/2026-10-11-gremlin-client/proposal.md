# Proposal

## Why

Every feature depends on talking to a Gremlin server. No maintained Rust driver meets our needs (streaming,
cancellation, handshake signing, timing) so we write our own (ADR-0011) and prove it end to end early.

## What Changes

- `gremlin-model`: `GValue` enum covering GraphSON v3 types (primitives incl. Int32/Int64/Float/Double/BigDecimal/
  BigInteger/Date/Timestamp/UUID/Char/Byte/Bytes, List, Set, Map with non-string keys, Vertex, Edge, VertexProperty,
  Property, Path, Tree, T, Direction, Traverser with bulk, Metrics/TraversalMetrics) and a GraphSON v3 decoder/encoder.
- `gremlin-client`: WebSocket session (ws/wss, rustls), `eval` with bindings and per-request timeout, streamed
  partial responses, cancellation, SASL PLAIN authentication, a `HandshakeSigner` hook, a small pool, automatic
  reconnect, and request timing.
- `gremliner-cli query --url <ws-url> [--user --password] [--json] "<gremlin>"`.

Out of scope: GraphBinary (later), connection management UI, providers (Neptune).

## Capabilities

### New Capabilities
- `gremlin-client`: sending Gremlin scripts to a TinkerPop-compatible server and receiving typed, streamed results.

### Modified Capabilities
- None.

## Impact

- Crates: `gremlin-model`, `gremlin-client`, `gremliner-cli`.
- Dependencies: tokio-tungstenite, rustls, serde_json, uuid, bigdecimal (new, small).
- ADRs: 0011, 0012, 0013, 0030.
