# Design

## Context

Schema types (schema-discovery), graph view (graph-view), parameters as bindings (ADR-0030). See proposal.md.

## Goals / Non-Goals

**Goals:**
- Generated queries are idiomatic, readable and efficient (use `has(label, key, pred)` early).

**Non-Goals:**
- Arbitrary branching patterns (linear paths only).

## Decisions

- **Model:** `ExplorerPath { steps: Vec<Step::Vertex{label: Option, filters, fixed_id} | Step::Edge{label: Option, dir, filters}>, limit, randomize }`.
- **Generation:** `g.V()[.hasId(id)|.hasLabel(l)].has(k, pred)… .outE(l).has(…).inV()… .path().limit(n)`; "any" label omits `hasLabel`;
  `randomize` adds `.order().by(shuffle)` before limit (warn on large graphs). Values bound as `p0, p1, …`.
- **Operator table by type:** numbers/dates: eq, neq, gt, gte, lt, lte, between (`P.between` lower-inclusive, noted in UI), inside, outside, within, without;
  strings: eq, neq, startingWith, endingWith, containing, notContaining, within, without (TextP); booleans: eq.
- **UI:** horizontal chip builder with label dropdowns (search), filter popovers, and a live read-only code preview using the editor highlighter.

## Risks / Trade-offs

- [Unbounded "any" paths explode] → enforced limit and timeout; hint when using "any" at multiple steps.

## Verification

- Unit tests (insta) for generation from explorer models.
- Integration tests on air-routes for scenarios.
- Headless UI tests for builder validation and operator lists.
