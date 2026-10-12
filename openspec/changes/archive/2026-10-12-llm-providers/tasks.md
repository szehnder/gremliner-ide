# Tasks

## 1. Client layer

- [x] 1.1 Normalised request/event types and SSE parser; verify unit tests with fixture streams
- [x] 1.2 Anthropic, Gemini and OpenAI-compatible implementations with streaming, tools and JSON output; verify `scenario_llm_providers_incremental_tokens` per format with fixtures
- [x] 1.3 Model listing per format; verify `scenario_llm_providers_list_local_models` (mock server) and `scenario_llm_providers_manual_model_entry`
- [x] 1.4 Capability defaults and overrides; verify `scenario_llm_providers_override_tool_calling`

## 2. Profiles and settings

- [x] 2.1 `llm_profiles` storage with keychain keys and presets; verify `scenario_llm_providers_ollama_preset` and `scenario_llm_providers_key_stored_securely`
- [x] 2.2 AI settings page with profile editor, test action, default and per-connection override; verify `scenario_llm_providers_successful_test`, `scenario_llm_providers_invalid_key`, `scenario_llm_providers_per_connection_override`

## 3. CLI

- [x] 3.1 `gremliner-cli llm test|models`; verify `scenario_llm_providers_cli_llm_test` (mock server)

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate llm-providers --strict` passes
