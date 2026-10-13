# Design

## Context

`gremlin-syntax::parameters` exists (gremlin-language); bindings are supported by `gremlin-client`. ADR-0030. See proposal.md.

## Goals / Non-Goals

**Goals:**
- Parameters feel like part of the editor, not a separate form.

**Non-Goals:**
- Parameter types beyond JSON scalars, lists and maps.

## Decisions

- **Value parsing:** `serde_json::from_str(input)` → on failure, string. JSON → `GValue` mapping: integers fitting i32 → Int32? No: integers → Int64 (`g:Int64`) unless the schema property type is Int32, in which case Int32 is used so `has()` comparisons match exactly on TinkerGraph.
- **Panel:** a collapsible section under the editor toolbar listing params with inline inputs; state stored in tab options.
- **Report tab:** a lightweight tab type (`ReportTab { saved_query_id, values }`) with form + results dock embedded.

## Risks / Trade-offs

- [Numeric type mismatches (Int32 vs Int64) cause empty results on some providers] → schema-informed typing as above, with a per-parameter type override.

## Verification

- Unit tests for value parsing and type selection.
- Integration test confirming bindings arrive separately (testenv request echo via result correctness).
- Headless UI tests for panel, prompt and report tab.
