# Tasks

## 1. Pipeline core

- [ ] 1.1 Prompt builder with schema compaction, dialect and privacy modes; verify insta snapshots and `scenario_nl_to_gremlin_cloud_model_without_sample_sharing`
- [ ] 1.2 Structured output handling with tolerant extraction; verify unit tests across JSON-schema, JSON-object and free-text fixtures
- [ ] 1.3 Schema checker (labels, keys, direction, unsupported steps); verify `scenario_nl_to_gremlin_schema_mismatch_detected`
- [ ] 1.4 `inject_limit` and the read-only dry run; verify `scenario_nl_to_gremlin_dry_run_preview` and `scenario_nl_to_gremlin_mutation_never_auto_executed`
- [ ] 1.5 Repair loop and state machine events; verify `scenario_nl_to_gremlin_repair_after_server_error` and `scenario_nl_to_gremlin_give_up_gracefully` with the fake LLM
- [ ] 1.6 Grounding check with the fake LLM asserting the context contains labels and triples; verify `scenario_nl_to_gremlin_uses_real_labels`

## 2. UI

- [ ] 2.1 Prompt bar (Ctrl/Cmd+K) with profile and privacy disclosure and the no-LLM state; verify `scenario_nl_to_gremlin_open_and_submit` and `scenario_nl_to_gremlin_no_llm_configured`
- [ ] 2.2 Result card with badges, preview table/mini-graph and live status; verify via headless UI test
- [ ] 2.3 Insert, Run, Copy and Refine actions; verify `scenario_nl_to_gremlin_insert_into_editor`, `scenario_nl_to_gremlin_run_from_card`, `scenario_nl_to_gremlin_refine_a_query`

## 3. CLI and evaluation

- [ ] 3.1 `gremliner-cli nl`; verify `scenario_nl_to_gremlin_cli_nl` with the fake LLM
- [ ] 3.2 Golden set and `cargo xtask eval-assistant`; verify `scenario_nl_to_gremlin_golden_set_evaluation` with the reference model (manual gate) and record the Ollama score

## 4. Verification

- [ ] 4.1 `cargo xtask ci` is green with testenv running
- [ ] 4.2 `openspec validate nl-to-gremlin --strict` passes
- [ ] 4.3 Screenshot review of the prompt bar and a verified result card
