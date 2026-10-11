# Design

## Context

Selection comes from the canvas; elements have property side-tables (graph-view). Mutations are user-initiated
(ADR-0020 forbids only LLM-initiated mutations). Bindings per ADR-0030. See proposal.md.

## Goals / Non-Goals

**Goals:**
- Generated statements are readable and portable across Gremlin Server and Neptune.

**Non-Goals:**
- Transactions across several edits (each edit is one statement).

## Decisions

- **Edit commands:** `enum EditCommand { SetProperty{el, key, value, cardinality}, RemoveProperty{..}, AddVertex{label, props},
  AddEdge{from, to, label, props}, DropElements{ids} }` → `fn to_statement(&self) -> (String, Bindings)` e.g.
  `g.V(vid).property(single, k, v)` with bindings `{vid, k, v}`; inverse computed from the pre-edit snapshot.
- **Id typing:** ids are bound with their original GValue type (Neptune string ids vs TinkerGraph longs).
- **Undo stack:** per tab, holding `(command, inverse)`; cleared on connection change.
- **Edge gesture:** Alt+drag from a node draws a rubber-band line in the canvas; drop on a node opens the label picker (labels from the schema when available).
- **Apply:** after server success, patch the store locally; on failure show the error and leave the store unchanged.

## Risks / Trade-offs

- [Meta-property and multi-property semantics differ by provider (Neptune: set cardinality default)] → cardinality explicit in the property editor; default from provider capabilities.

## Verification

- Unit tests for statement generation and inverses (insta snapshots).
- Integration tests on a scratch graph (`gSynth` reset per test) for each scenario.
- Headless UI tests for confirmation flows.
