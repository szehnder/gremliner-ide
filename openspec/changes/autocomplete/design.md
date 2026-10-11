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
