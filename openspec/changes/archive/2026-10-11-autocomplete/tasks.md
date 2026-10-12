# Tasks

## 1. Analysis

- [x] 1.1 Completion context extraction and argument roles in the step catalogue; verify `scenario_autocomplete_suggest_steps` and `scenario_autocomplete_source_steps_after_g`
- [x] 1.2 Label-flow inference; verify `scenario_autocomplete_reachable_edge_labels_ranked_first` and `scenario_autocomplete_keys_for_inferred_label`

## 2. Provider

- [x] 2.1 Editor completion provider with ranking; verify `scenario_autocomplete_vertex_label` and `scenario_autocomplete_sample_value_suggestion`
- [x] 2.2 Signature help; verify `scenario_autocomplete_signature_for_has`
- [x] 2.3 Enum, predicate and snippet completions; verify `scenario_autocomplete_predicate_completion`
- [x] 2.4 Schema warnings with quick fixes; verify `scenario_autocomplete_misspelled_label`

## 3. Performance

- [x] 3.1 Completion latency benchmark; verify `scenario_autocomplete_completion_speed`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate autocomplete --strict` passes
