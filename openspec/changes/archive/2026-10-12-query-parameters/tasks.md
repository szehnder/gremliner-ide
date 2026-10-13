# Tasks

## 1. Core

- [x] 1.1 Value parsing and schema-informed typing; verify `scenario_query_parameters_number_and_string_values`
- [x] 1.2 Bindings in execution; verify `scenario_query_parameters_query_text_unchanged`

## 2. UI

- [x] 2.1 Parameters panel with live detection; verify `scenario_query_parameters_parameters_listed`
- [x] 2.2 Missing-value prompt; verify `scenario_query_parameters_prompt_for_missing`
- [x] 2.3 Defaults and labels saved with queries; verify `scenario_query_parameters_default_restored`
- [x] 2.4 Report tabs; verify `scenario_query_parameters_run_report`

## 3. Verification

- [x] 3.1 `cargo xtask ci` is green with testenv running
- [x] 3.2 `openspec validate query-parameters --strict` passes
