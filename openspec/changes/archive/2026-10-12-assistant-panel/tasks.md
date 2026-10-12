# Tasks

## 1. Session and modes

- [x] 1.1 Per-tab `Session` with streaming Markdown and clear; verify `scenario_assistant_panel_separate_conversations`
- [x] 1.2 Explain mode with step mapping; verify `scenario_assistant_panel_explain_with_step_mapping`
- [x] 1.3 Fix mode reusing pipeline verification; verify `scenario_assistant_panel_fix_a_failing_query`
- [x] 1.4 Optimize mode with equivalence-on-sample; verify `scenario_assistant_panel_optimize_with_profile`
- [x] 1.5 Describe mode honouring privacy; verify `scenario_assistant_panel_describe_without_samples`

## 2. Tools and context

- [x] 2.1 Read-only tools with executor safety; verify `scenario_assistant_panel_visible_tool_call` and `scenario_assistant_panel_mutation_refused_by_tool`
- [x] 2.2 Context chips; verify `scenario_assistant_panel_remove_context`

## 3. UI

- [x] 3.1 Panel UI with quick actions and verified code blocks; verify `scenario_assistant_panel_insert_from_answer`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate assistant-panel --strict` passes
- [ ] 4.3 Screenshot review of the panel with an explanation — **pending:** manual review (agent-launched windows stop after the first frame)
