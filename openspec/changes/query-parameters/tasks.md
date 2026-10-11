# Tasks

## 1. Core

- [ ] 1.1 Value parsing and schema-informed typing; verify `scenario_query_parameters_number_and_string_values`
- [ ] 1.2 Bindings in execution; verify `scenario_query_parameters_query_text_unchanged`

## 2. UI

- [ ] 2.1 Parameters panel with live detection; verify `scenario_query_parameters_parameters_listed`
- [ ] 2.2 Missing-value prompt; verify `scenario_query_parameters_prompt_for_missing`
- [ ] 2.3 Defaults and labels saved with queries; verify `scenario_query_parameters_default_restored`
- [ ] 2.4 Report tabs; verify `scenario_query_parameters_run_report`

## 3. Verification

- [ ] 3.1 `cargo xtask ci` is green with testenv running
- [ ] 3.2 `openspec validate query-parameters --strict` passes
