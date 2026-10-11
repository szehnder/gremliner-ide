# Tasks

## 1. Grammar

- [x] 1.1 Write `grammar.js`, generate `parser.c`, wire the `cc` build and tree-sitter `Language`; verify `cargo build -p gremlin-syntax` without Node
- [x] 1.2 Add tree-sitter corpus tests and the valid-query corpus (300+ queries); verify `scenario_gremlin_language_valid_corpus_parses_cleanly`
- [x] 1.3 Write `highlights.scm`; verify a snapshot test of captures for a sample query

## 2. Analysis

- [x] 2.1 Step tree extraction; verify `scenario_gremlin_language_nested_anonymous_traversal`
- [x] 2.2 Diagnostics with recovery and friendly messages; verify `scenario_gremlin_language_unclosed_parenthesis` and `scenario_gremlin_language_error_recovery`
- [x] 2.3 Statements and statement-at-cursor; verify `scenario_gremlin_language_pick_the_statement_under_the_cursor`
- [x] 2.4 Step catalogue (`steps.toml`) with docs and mutating flags; verify `scenario_gremlin_language_hover_documentation`
- [x] 2.5 Mutation detection; verify `scenario_gremlin_language_detect_drop`, `scenario_gremlin_language_read_only_query`, `scenario_gremlin_language_unparseable_treated_as_mutating`, and replace the keyword-scan fallback in `connections`
- [x] 2.6 Parameter detection; verify `scenario_gremlin_language_detect_parameters`
- [x] 2.7 Prefix-at-step (top-level and nested); verify `scenario_gremlin_language_top_level_prefix` plus nested cases
- [x] 2.8 Formatter; verify `scenario_gremlin_language_format_long_query` and `scenario_gremlin_language_idempotent_formatting`
- [x] 2.9 Incremental edit API and benchmark; verify `scenario_gremlin_language_keystroke_latency`

## 3. CLI

- [x] 3.1 `gremliner-cli lint|fmt|parse`; verify `scenario_gremlin_language_cli_lint_exit_code`

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate gremlin-language --strict` passes
