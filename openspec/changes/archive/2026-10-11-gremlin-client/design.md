# Design

## Context

TinkerPop's WebSocket protocol: client sends a binary frame of `<mime-len byte><mime-type><JSON request>`; the
server replies with JSON frames `{requestId, status:{code,message,attributes}, result:{data, meta}}` where code 206 =
partial content, 200 = success, 204 = no content, 407 = authenticate, 4xx/5xx = errors. See ADR-0011/0012/0013.

## Goals / Non-Goals

**Goals:**
- Conformance with Gremlin Server 3.4–3.7 and Neptune (script eval).
- Zero-copy-ish decoding: parse frames with `serde_json::from_slice` into `Value`, then convert to `GValue` once.

**Non-Goals:**
- Bytecode, sessions/transactions (later if needed).

## Decisions

- **Architecture:** a `Connection` task owns the socket (split sink/stream) and a `HashMap<Uuid, mpsc::Sender<Batch>>`;
  callers get a `ResultStream` (`Stream<Item = Result<Vec<GValue>>>`) plus a `RequestHandle` for cancel.
  `Client` holds a pool of N connections (default 2), round-robin with health check.
- **Cancel:** dropping the stream or calling `cancel()` removes the sender; remaining frames for that id are discarded.
  (TinkerPop has no server-side cancel for sessionless requests; `evaluationTimeout` bounds server work.)
- **Timeout:** both `evaluationTimeout` arg (server side) and a client-side `tokio::time::timeout` (+ 500 ms grace).
- **Auth:** on status 407, respond with `op: "authentication"`, `args: {saslMechanism:"PLAIN", sasl: base64("\0user\0pass")}`.
  Errors are scrubbed of secrets.
- **HandshakeSigner:** `async fn sign(&self, req: &mut http::Request<()>) -> Result<()>` applied to the upgrade request (used by Neptune IAM later).
- **Terminal step:** the client does not modify scripts; appending `.toList()` is the editor's job (query-execution change).
- **GraphSON decoder:** a hand-written match over `@type` strings (`g:Int32`, `g:Int64`, `g:Double`, `g:Float`,
  `g:List`, `g:Set`, `g:Map` (flat k/v array), `g:Vertex`, `g:Edge`, `g:VertexProperty`, `g:Property`, `g:Path`,
  `g:T`, `g:Direction`, `g:Traverser`, `g:UUID`, `g:Date`, `g:Timestamp`, `gx:BigDecimal`, `gx:BigInteger`,
  `gx:Byte`, `gx:ByteBuffer`, `gx:Char`, `g:TraversalMetrics`, `g:Metrics`, `g:Tree`, `g:Class`, unknown → `GValue::Unknown{type,value}`).
  `GValue::Map` is an `IndexMap<GKey, GValue>` where `GKey` covers hashable variants.
- **Raw response retention:** each batch keeps its raw JSON bytes (behind an option) for the Console/JSON view.

## Outcome (implementation notes)

- GraphSON fixtures were captured from the real testenv server (`crates/gremlin-model/tests/fixtures/`). They
  showed that `g:Metrics` wraps its fields in a typed `g:Map`, which the first decoder got wrong.
- Response frames are decoded in one streaming pass with simd-json (ADR-0033): 10 MB in ~60 ms versus 268 ms for
  the tree-based reference decoder, which is kept and checked for equivalence on every fixture.
- Server status 598 (the server-side evaluation timeout) maps to `Error::Timeout`, like the client deadline.
- The reconnect test kills an in-process TCP proxy instead of restarting the shared Docker server.
- `ClientConfig` and `Credentials` redact the password in `Debug`.

## Risks / Trade-offs

- [Neptune returns slightly different GraphSON for some types] → fixture tests from recorded Neptune responses.
- [Big results flood memory] → batches are delivered as they arrive; callers decide retention.

## Verification

- `cargo test -p gremlin-model` (insta snapshots for every GraphSON type from TinkerPop's IO docs samples).
- `cargo test -p gremlin-client` integration tests against testenv (8182 open, 8183 SASL).
- `cargo run -p gremliner-cli -- query --url ws://localhost:8182/gremlin --json "g.V().count()"`.
- criterion bench `graphson_decode` on a 10 MB air-routes `g.E().elementMap()` payload (budget in perf-budgets.toml).
