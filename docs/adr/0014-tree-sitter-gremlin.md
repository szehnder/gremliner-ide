# 0014. A single tree-sitter Gremlin grammar powers all language features

- Status: accepted
- Date: 2026-10-11

## Context
We need highlighting, syntax diagnostics, completion context, hover, formatting, mutation detection, parameter
extraction, and debugger step spans. No tree-sitter Gremlin grammar exists. The gpui-kit editor highlights via
tree-sitter. TinkerPop maintains an ANTLR `Gremlin.g4` for the canonical gremlin-language.

## Decision
Write `tree-sitter-gremlin` (grammar.js derived from TinkerPop's gremlin-language grammar, permissive enough to
parse Groovy-console idioms like `g.V().has('name','x').next()` and multiple statements separated by newlines/`;`).
Generated C sources are checked in under `crates/gremlin-syntax/grammar/src` and compiled with `cc` — no Node needed
to build. `gremlin-syntax` exposes a typed analysis API over the CST used by the editor, assistant, and debugger.

## Alternatives considered
- **Hand-written recursive descent parser** — more control, but duplicates work for highlighting and lacks
  incremental reparsing.
- **ANTLR Rust target** — immature; no incremental parsing.

## Consequences
- Incremental, error-tolerant parsing on every keystroke (< 1 ms for typical queries).
- A grammar test corpus (`test/corpus/*.txt`) plus a corpus of real queries guards regressions.

## Revisit when
The grammar can't express a construct users need (e.g. full Groovy closures) — then add a fallback "script mode".
