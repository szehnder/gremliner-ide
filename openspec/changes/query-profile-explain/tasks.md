# Tasks

## 1. Models and parsing

- [ ] 1.1 `ProfileReport` and TinkerPop metrics mapping; verify `scenario_query_profile_explain_profile_on_gremlin_server`
- [ ] 1.2 Neptune profile parser with fixtures; verify `scenario_query_profile_explain_parse_neptune_profile`
- [ ] 1.3 Explain for TinkerPop and Neptune; verify `scenario_query_profile_explain_explain_on_gremlin_server`

## 2. UI

- [ ] 2.1 Profile view with bars and bottleneck marking; verify `scenario_query_profile_explain_slowest_step_marked`
- [ ] 2.2 Step-to-source highlighting; verify `scenario_query_profile_explain_step_to_source`
- [ ] 2.3 Mutation confirmation; verify `scenario_query_profile_explain_confirm_profiling_a_mutation`

## 3. CLI

- [ ] 3.1 `gremliner-cli profile|explain`; verify `scenario_query_profile_explain_cli_profile_json`

## 4. Verification

- [ ] 4.1 `cargo xtask ci` is green with testenv running
- [ ] 4.2 `openspec validate query-profile-explain --strict` passes
