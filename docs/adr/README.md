# Architecture Decision Records

One decision per file, MADR-style (see [0001](0001-record-architecture-decisions.md) and [template.md](template.md)). Accepted ADRs are immutable; supersede them with a new ADR.

| # | Decision | Status |
|---|---|---|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions as atomic MADR ADRs | accepted |
| [0002](0002-rust-as-the-only-language.md) | Rust as the single implementation language | accepted |
| [0003](0003-gpui-and-gpui-kit-for-ui.md) | GPUI + gpui-kit for the user interface | accepted |
| [0004](0004-layered-cargo-workspace.md) | Layered Cargo workspace with strict dependency direction | accepted |
| [0005](0005-tokio-runtime-bridged-to-gpui.md) | A dedicated tokio runtime bridged to GPUI executors | accepted |
| [0006](0006-graph-canvas-on-gpui-primitives.md) | Graph canvas built on GPUI paint primitives with LOD and culling | accepted |
| [0007](0007-fallback-wgpu-surface.md) | Fallback: custom wgpu surface for the graph canvas | proposed |
| [0008](0008-soa-graph-store.md) | Struct-of-arrays graph store with dense u32 indices | accepted |
| [0009](0009-off-main-thread-layout.md) | Layouts run off the main thread and stream position frames | accepted |
| [0010](0010-forceatlas2-default-layout.md) | Barnes-Hut ForceAtlas2 as the default layout | accepted |
| [0011](0011-own-gremlin-wire-client.md) | Implement our own Gremlin wire client | accepted |
| [0012](0012-graphson-v3-first.md) | GraphSON v3 first, GraphBinary later | accepted |
| [0013](0013-script-eval-submission.md) | Submit queries as scripts (`eval`), not bytecode | accepted |
| [0014](0014-tree-sitter-gremlin.md) | A single tree-sitter Gremlin grammar powers all language features | accepted |
| [0015](0015-graph-provider-trait.md) | `GraphProvider` trait with a capability matrix | accepted |
| [0016](0016-neptune-iam-sigv4.md) | Neptune IAM auth via aws-config and aws-sigv4 on the WebSocket handshake | accepted |
| [0017](0017-localstack-neptune-discovery.md) | LocalStack Neptune endpoint discovery via DescribeDBClusters | accepted |
| [0018](0018-own-llm-layer.md) | Own thin LLM layer with three wire formats | accepted |
| [0019](0019-code-driven-schema-grounded-pipeline.md) | Code-driven, schema-grounded NL→Gremlin pipeline | accepted |
| [0020](0020-llm-never-runs-mutations.md) | The LLM never executes mutations; dry runs are read-only and bounded | accepted |
| [0021](0021-schema-context-privacy.md) | Schema context sent to LLMs: typed summary by default, sample values opt-in | accepted |
| [0022](0022-sqlite-and-keychain.md) | SQLite for application data; OS keychain for secrets | accepted |
| [0023](0023-design-tokens-and-theming.md) | Design tokens and theming for Compass-level polish | accepted |
| [0024](0024-headless-cli-verification-surface.md) | Headless CLI as the agent's primary verification surface | accepted |
| [0025](0025-testing-strategy.md) | Testing strategy | accepted |
| [0026](0026-gremlin-server-docker-testenv.md) | TinkerPop gremlin-server in Docker as the test environment and Neptune stand-in | accepted |
| [0027](0027-performance-budgets.md) | Performance budgets as CI gates | accepted |
| [0028](0028-no-telemetry.md) | No telemetry; local-only diagnostics | accepted |
| [0029](0029-release-packaging.md) | Release packaging with cargo-dist | accepted |
| [0030](0030-parameters-as-bindings.md) | Query parameters are sent as native bindings | accepted |
| [0031](0031-x86-64-v2-baseline.md) | x86-64-v2 as the minimum x86 CPU target | accepted |
| [0032](0032-raster-level-of-detail.md) | Raster level of detail for dense graph views | accepted |
| [0033](0033-streaming-simd-graphson-decoding.md) | Streaming GraphSON decoding with simd-json | accepted |
| [0034](0034-session-only-secret-fallback.md) | Session-only secret fallback and log redaction (amends 0022) | accepted |
| [0035](0035-ui-theme-crate.md) | A `ui-theme` crate for design tokens (amends 0004, 0023) | accepted |
| [0036](0036-neptune-transport-details.md) | Neptune transport: ring-only AWS stack, tunnels, HTTP features, restriction table (amends 0016) | accepted |
| [0037](0037-localstack-discovery-details.md) | LocalStack Neptune discovery details and engine version table (amends 0017) | accepted |
| [0038](0038-resvg-for-image-export.md) | Graph image export through SVG and resvg (amends 0006) | accepted |
