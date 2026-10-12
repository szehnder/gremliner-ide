# Design

## Context

`gremlin-syntax` provides CST, step catalogue and incremental documents; `schema` provides labels, keys, triples,
samples; gpui-kit's editor exposes a completion provider trait (LSP-shaped). See proposal.md; ADR-0014.

## Goals / Non-Goals

**Goals:**
- Label-flow inference good enough to scope keys and edge labels in linear traversals.

**Non-Goals:**
- Full type inference through `select`/`project` maps (best effort).

## Decisions

- **Completion context:** `gremlin_syntax::completion_context(doc, offset) -> Ctx { kind: AfterDot{receiver} | Arg{step, index, partial} | EnumMember{enum} | TopLevel, prefix }`.
- **Argument roles:** the step catalogue annotates each signature parameter with a role (`VertexLabel`, `EdgeLabel`, `PropertyKey`, `Value(key_ref)`, `Predicate`, `Traversal`, `Number`…).
- **Label-flow:** walk steps left to right tracking a set of possible element labels: `V()`→all vertex labels; `hasLabel(x)`→{x};
  `out(e)`→in-labels of triples with that edge label from current; `outE(e)`→edge label set; `inV()`→…; unknown steps reset to "any".
- **Ranking:** prefix match > fuzzy (nucleo); boost catalogue "common" steps, schema-reachable labels, and recently used items in this session.
- **Snippets:** defined in `snippets.toml` with tab stops (gpui-kit snippet support if available, else plain insertion with cursor placement).
- **Warnings:** run after each analysis when a schema is loaded; Levenshtein ≤ 2 for quick fixes.

## Risks / Trade-offs

- [Schema not yet discovered] → degrade to catalogue-only completions; warnings disabled until a schema exists.

## Verification

- Unit tests for completion context and label-flow on fixtures with the modern and air-routes schemas (schema JSON fixtures, no server needed).
- Headless editor tests for each scenario.
- criterion `completion_latency`.

## As built

- **Context without tree-sitter.** `gremlin_syntax::complete::scan` is a tolerant token scanner over the
  text before the cursor. Half-typed text such as an unclosed string, an open call or a trailing `.`
  is the normal case here, and tree-sitter's error recovery makes those shapes unreliable. For each nesting
  level it keeps the chain of calls so far and the call whose parentheses come next. The tail is
  `AfterDot{chain}`, `InString{args,index}`, `InArg{args,index}` or `Other`. A bare identifier at depth 0
  after a finished expression starts a new chain, so one-statement-per-line tabs work. The same scanner,
  run over the whole text, records every closed call for the schema warnings.
- **Roles live in code, not `steps.toml`.** `complete::role(step, index, args, schema)` decides the role
  from the arguments so far. For example, `has('airport', ·)` is a key when `airport` is a known label and not a key, and
  `has(k, ·)` is a value of `k`. `property()` keys are never flagged, since that step creates them.
- **Label flow** (`complete::flow`) follows linear steps over the triples. Inside anonymous traversals,
  flow continues from the enclosing step. `by()` sees the input of the step it modulates, skipping other
  modulators.
- **Ranking.** Prefix match (case-sensitive, then ignoring case), then subsequence, then the boost, then
  catalogue order. Common steps, reachable edge labels and non-terminal steps get boosts. Values use
  prefix matches only. The editor moves labels accepted earlier in the session to the front among exact
  prefix matches. nucleo was not needed.
- **Insertion.** gpui-kit 0.7 has no snippet tab stops. Each `Item` carries a cursor offset; the editor
  remembers the inserts it offered and, after an edit that inserted one, moves the cursor (into `has()`, or to
  a snippet's `|`). gpui-kit auto-closes `(` and quotes, so string completions add a closing quote only when none follows.
- **Signature help** is our own strip under the editor (gpui-kit has no signature popover). It shows the
  best-fitting form, with argument types checked against the literals so far and the label form of `has`
  preferred for a known label, and the current parameter in bold.
- **Warnings and fixes.** Schema warnings are skipped while discovery is in progress and for parts the
  schema marks partial. The quick fix is a gpui-kit `CodeActionProvider` (Ctrl+.). It applies its edit in
  `window.defer`, because it is called while the editor state is being updated.
- **Wiring.** `EditorPanel` observes the `SchemaRegistry` and calls `QueryEditor::set_schema` for its
  connection.
- **Measured.** `completion_latency` (criterion, release) takes about 0.37 ms. In the scenario test, the
  p95 over 100 positions in a 200-line document is about 0.6 ms in a debug build. The budget is 30 ms.
- **Test infrastructure.** gpui-kit dialogs slide in over real time, so a click computed mid-animation can
  miss its target. This was the source of an intermittent `scenario_connections_duplicate_name_rejected` failure.
  UI tests now click dialog buttons through `tests/support::click_settled`, which waits for stable bounds.
