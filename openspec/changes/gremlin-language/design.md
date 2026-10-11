# Design

## Context

ADR-0014 picks tree-sitter. TinkerPop's `gremlin-language/src/main/antlr4/Gremlin.g4` defines the canonical grammar
(~1.5k lines, one rule per step signature). Tree-sitter needs a more generic shape. See proposal.md.

## Goals / Non-Goals

**Goals:**
- A generic grammar (`traversal := source ('.' step)*`, `step := identifier '(' args? ')'`) so new steps never need grammar changes.
  Step knowledge (names, signatures, mutating flag, docs) lives in a data catalogue, not the grammar.

**Non-Goals:**
- Full Groovy (closures/lambdas are parsed as opaque `lambda` nodes so they don't produce errors).

## Decisions

- **Grammar shape (generic):** `source_file := statement*`; `statement := expression terminator?`;
  `expression := traversal | literal | identifier | assignment`; `traversal := root ('.' call)*` where root is
  `identifier` (g, __, P, T, …) or a bare `call` (anonymous traversal like `out()`); `call := name arguments`; literals
  per Gremlin numeric suffix rules; `map_literal := '[' (pair (',' pair)* | ':') ']'`; `lambda := '{' … '}'` opaque.
  `grammar.js` + generated `src/parser.c` checked in; `xtask grammar` regenerates with `tree-sitter-cli` (dev only).
- **Highlight queries:** `queries/highlights.scm` captures step names (`@function.method`), sources (`@variable.builtin`),
  enums/predicates (`@constant.builtin`), strings, numbers, comments; `queries/injections.scm` empty.
- **Step catalogue:** `steps.toml` embedded with `include_str!`: name, signatures, kind (filter/map/flatMap/sideEffect/
  branch/barrier/terminal/mutation), mutating bool, short doc, link. Generated once from TinkerPop reference docs, then curated.
- **Analysis API:** `Document::parse(text)`; `edit(range, new_text)` (incremental); `diagnostics()`; `statements()`;
  `statement_at(offset)`; `steps(stmt) -> Vec<StepNode{name, span, args, children, depth}>`; `is_mutating(stmt)`;
  `parameters(stmt)`; `prefix_at(stmt, step_id) -> String` (for nested steps, wraps the anonymous traversal into its
  parent context; see debugger design); `format(text, width)`.
- **Diagnostics beyond syntax (cheap, local):** unknown step name (not in catalogue) as a warning; missing terminal
  step is not an error (the editor appends `.toList()`).
- **Formatter:** Gremlint rules: break before `.` when line > width; indent nested traversals by 2; normalise `,` spacing; preserve comments.

## Risks / Trade-offs

- [Generic grammar accepts invalid arities] → arity/signature checks in the analysis layer as warnings using the catalogue.
- [Groovy idioms in pasted scripts] → opaque lambda nodes + "script mode" fallback flag.

## Verification

- `cargo test -p gremlin-syntax` (tree-sitter corpus tests via `xtask grammar test`, analysis unit tests, insta snapshots for formatter).
- A corpus of 300+ real queries (TinkerPop docs recipes, Practical Gremlin examples) in `crates/gremlin-syntax/tests/corpus/valid/*.groovy` parses without errors.
- criterion bench `incremental_reparse_200_lines`.
- `cargo run -p gremliner-cli -- lint tests/corpus/invalid/unclosed.groovy` exits non-zero.
