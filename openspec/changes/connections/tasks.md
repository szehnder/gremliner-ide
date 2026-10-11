# Tasks

## 1. Provider layer

- [ ] 1.1 Define `GraphProvider`, `Capabilities`, `ProviderConfig` and the Gremlin Server provider; verify `scenario_provider_gremlin_server_defaults`
- [ ] 1.2 Implement capability/version detection; verify `scenario_provider_gremlin_server_detect_tinkerpop_version` against testenv
- [ ] 1.3 Implement alias support; verify `scenario_provider_gremlin_server_use_gmodern_alias`

## 2. Storage

- [ ] 2.1 Add the `connections` table, CRUD and keyring-backed secrets with fallback; verify `scenario_connections_password_not_on_disk`
- [ ] 2.2 Add the secret-redacting tracing layer; verify a unit test that logs a registered secret and asserts it is redacted

## 3. UI

- [ ] 3.1 Connection dialog with provider-specific sections, validation and paste-to-fill; verify `scenario_connections_create_a_connection`, `scenario_connections_duplicate_name_rejected`, `scenario_connections_paste_a_websocket_url`, `scenario_connections_paste_a_neptune_endpoint`
- [ ] 3.2 Test-connection flow; verify `scenario_connections_successful_test` and `scenario_connections_unreachable_host`
- [ ] 3.3 Sidebar with status, search and actions, plus delete confirmation; verify `scenario_connections_status_indicator` and `scenario_connections_delete_requires_confirmation`
- [ ] 3.4 Environment badges in tabs and status bar; verify `scenario_connections_production_badge`
- [ ] 3.5 Read-only enforcement with override; verify `scenario_connections_mutation_blocked`

## 4. CLI

- [ ] 4.1 `gremliner-cli connections list|add|test|remove`; verify `scenario_connections_cli_test`

## 5. Verification

- [ ] 5.1 `cargo xtask ci` is green
- [ ] 5.2 `openspec validate connections --strict` passes
- [ ] 5.3 Screenshot review of the connection dialog and sidebar
