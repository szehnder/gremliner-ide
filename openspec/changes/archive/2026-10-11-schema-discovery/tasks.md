# Tasks

## 1. Discovery

- [x] 1.1 Schema model and sampling discovery with limits and partial results; verify `scenario_schema_discovery_modern_graph_schema` and `scenario_schema_discovery_timeout_yields_partial_schema`
- [x] 1.2 Counts and sample values; verify `scenario_schema_discovery_sample_values`
- [x] 1.3 Provider fast path using the Neptune summary fixture; verify `scenario_schema_discovery_neptune_summary_used`
- [x] 1.4 Cache, background refresh and manual refresh; verify `scenario_schema_discovery_cached_schema_on_reconnect`

## 2. UI and CLI

- [x] 2.1 Sidebar schema tree with sample-query action; verify `scenario_schema_discovery_open_sample_query_from_schema`
- [x] 2.2 Schema graph view reusing the graph canvas; verify `scenario_schema_discovery_schema_graph_of_air_routes`
- [x] 2.3 `gremliner-cli schema`; verify `scenario_schema_discovery_cli_schema_json`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green with testenv running
- [x] 3.2 `openspec validate schema-discovery --strict` passes
