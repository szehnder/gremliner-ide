# Tasks

## 1. Data model

- [x] 1.1 Implement `GValue`/`GKey` and the GraphSON v3 decoder/encoder with insta snapshots for every type; verify `cargo test -p gremlin-model`
- [x] 1.2 Add `graphson_decode` criterion bench with a recorded 10 MB payload; verify it runs and record the baseline

## 2. Protocol

- [x] 2.1 Implement `Connection` (handshake, framing, multiplexing, status handling) and `ResultStream`; verify `scenario_gremlin_client_count_vertices` and `scenario_gremlin_client_bindings`
- [x] 2.2 Typed results end to end; verify `scenario_gremlin_client_vertex_with_properties` and `scenario_gremlin_client_path_result`
- [x] 2.3 Streaming partial batches; verify `scenario_gremlin_client_large_result_streams`
- [x] 2.4 Timeouts and cancellation; verify `scenario_gremlin_client_timeout` and `scenario_gremlin_client_cancel`
- [x] 2.5 Server error mapping; verify `scenario_gremlin_client_syntax_error`
- [x] 2.6 SASL PLAIN and `HandshakeSigner`; verify `scenario_gremlin_client_sasl_credentials` and `scenario_gremlin_client_wrong_credentials`
- [x] 2.7 Pool and reconnect; verify `scenario_gremlin_client_concurrent_requests` and `scenario_gremlin_client_reconnect_after_server_restart`

## 3. CLI

- [x] 3.1 Implement `gremliner-cli query` (text and `--json`); verify `scenario_gremlin_client_cli_json_output`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green with testenv running
- [x] 4.2 `openspec validate gremlin-client --strict` passes
