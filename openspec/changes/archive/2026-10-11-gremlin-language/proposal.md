# Proposal

## Why

Highlighting, error squiggles, formatting, autocomplete, mutation safety, parameters, the assistant's validation
and the debugger all need to understand Gremlin text. No tree-sitter grammar exists, so we build one (ADR-0014)
plus an analysis layer every other feature reuses.

## What Changes

- `tree-sitter-gremlin` grammar (in `crates/gremlin-syntax/grammar`) covering the TinkerPop gremlin-language: traversal
  sources and spawns, steps with arguments, anonymous traversals (`__.` and bare), predicates (`P.`, `TextP.`),
  enums (`T`, `Order`, `Scope`, `Column`, `Pop`, `Cardinality`, `Direction`, `Operator`, `Merge`, `Pick`), literals (strings,
  numbers with suffixes `L`/`d`/`f`/`m`/`n`, booleans, null, maps `[k:v]`, lists `[...]`), variables, comments,
  multiple statements, and terminal steps (`next`, `toList`, `iterate`, `hasNext`, `tryNext`, `toSet`, `toBulkSet`, `explain`).
- `gremlin-syntax` analysis API: diagnostics with ranges; statement splitting and "statement at cursor"; a step list
  with spans and nesting; mutation detection; free-variable (parameter) extraction; prefix-at-step for the debugger;
  a formatter following Gremlint conventions; hover documentation for each step from an embedded step catalogue.
- `gremliner-cli lint|fmt|parse`.

Out of scope: editor UI integration (query-editor), schema-aware checks (schema-discovery / nl-to-gremlin).

## Capabilities

### New Capabilities
- `gremlin-language`: understanding Gremlin text: parsing, diagnostics, formatting, statement and step structure, mutation and parameter detection.

### Modified Capabilities
- None.

## Impact

- Crates: `gremlin-syntax` (+ `cc` build dep, `tree-sitter`), `gremliner-cli`.
- ADRs: 0014, 0020, 0030.
