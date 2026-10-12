# Proposal

## Why

Typing Gremlin from memory is slow and error-prone. G.V()'s schema-aware autocomplete suggests the right steps,
labels and property keys in context; this is one of the biggest productivity wins in a Gremlin IDE.

## What Changes

- Completion provider for the editor: step names valid after the current expression (from the step catalogue, ranked by usage), with signature help as you type arguments.
- Schema-aware argument completions: vertex labels in `hasLabel`/`addV`/`V().has(label, …)`; edge labels in `out/in/both/outE/inE/bothE`, ranked by what is reachable from the current label using connectivity triples; property keys in `has/values/properties/valueMap/elementMap/by/order().by`; sample values for `has(key, <value>)`.
- Enum and predicate completions (`P.gt`, `TextP.containing`, `T.id`, `Order.desc`, `Scope.local`…).
- Snippets for common patterns (`repeat-until`, `project-by`, `group-by-count`, path with `by`).
- Semantic warnings: unknown label/property (relative to the discovered schema) as warnings with "did you mean" quick fixes.

Out of scope: LLM inline completion (could be a later change).

## Capabilities

### New Capabilities
- `autocomplete`: context-aware completions, signature help and schema-based warnings in the query editor.

### Modified Capabilities
- None.

## Impact

- Crates: `gremlin-syntax` (completion context, label-flow inference), `ui-editor` (completion provider), `schema`.
- ADRs: 0014.
