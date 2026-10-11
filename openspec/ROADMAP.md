# Roadmap: change order

Implement changes in this order. Each change lives in `openspec/changes/<name>/` until archived.
A milestone gate must pass before the next milestone starts.

| # | Milestone | Change | Capabilities (delta specs) | Gate |
|---|---|---|---|---|
| 1 | M0 Foundations | `bootstrap-workspace` | — (tooling, skip_specs) | `cargo xtask ci` green on empty crates |
| 2 | M0 | `testenv` | — (tooling, skip_specs) | `cargo xtask testenv up && seed` loads air-routes |
| 3 | M1 De-risk | `app-shell` | app-shell, theming, command-palette, settings | window opens, dock layout persists |
| 4 | M1 | `graph-canvas-spike` | graph-rendering | **render-bench meets ADR-0006 targets** (else ADR-0007) |
| 5 | M1 | `gremlin-client` | gremlin-client | CLI round-trip against testenv |
| 6 | M2 Core loop | `connections` | connections, provider-gremlin-server | |
| 7 | M2 | `gremlin-language` | gremlin-language | grammar corpus green |
| 8 | M2 | `query-editor` | query-editor | |
| 9 | M2 | `query-execution` | query-execution, results-table, results-json, results-console | run query, see results end-to-end |
| 10 | M3 Visualization | `graph-view` | graph-view | |
| 11 | M3 | `graph-layouts` | graph-layouts | air-routes FA2 < 2 s |
| 12 | M3 | `graph-stylesheets` | graph-stylesheets | |
| 13 | M3 | `graph-inspector-editing` | graph-editing | |
| 14 | M4 Schema | `schema-discovery` | schema-discovery | |
| 15 | M4 | `autocomplete` | autocomplete | |
| 16 | M5 AI | `llm-providers` | llm-providers | |
| 17 | M5 | `nl-to-gremlin` | nl-to-gremlin | golden set ≥ 90% (reference model) |
| 18 | M5 | `assistant-panel` | assistant-panel | |
| 19 | M6 Neptune | `provider-neptune-aws` | provider-neptune-aws | |
| 20 | M6 | `provider-neptune-localstack` | provider-neptune-localstack | |
| 21 | M6 | `query-profile-explain` | query-profile-explain | |
| 22 | M7 Power | `query-debugger` | query-debugger | |
| 23 | M7 | `graph-explorer` | graph-explorer | |
| 24 | M7 | `query-history-saved` | query-history, saved-queries | |
| 25 | M7 | `query-parameters` | query-parameters | |
| 26 | M7 | `export` | export | |
| 27 | M8 Polish | `polish-pass` | ux-quality | screenshot review of every surface |
| 28 | M8 | `performance-budgets` | performance-budgets | all budgets enforced |

## Later (not v1; propose when needed)
`assistant-autonomous-agent`, `graphbinary`, `provider-janusgraph`, `provider-cosmosdb`, `provider-aerospike`,
`ssh-ssm-tunnels`, `opencypher`, `dashboards`, `release-packaging`.
