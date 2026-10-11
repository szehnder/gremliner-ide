# Design

## Context

`gremlin-syntax` provides the step tree and `prefix_at` (gremlin-language). Results views accept any `ResultSet`. ADR-0020 forbids automatic mutations, and debugging only runs read-only prefixes.

## Goals / Non-Goals

**Goals:**
- Exact semantics for linear traversals and common nested contexts (`where`, `and`, `or`, `not`, `filter`, `repeat` body, `union`, `coalesce`, `local`, `optional`).

**Non-Goals:**
- Simulating server-side strategies (we execute real prefixes).

## Decisions

- **Prefix for nested steps:** to show results "inside" `where(A.B)` at step `B`, generate
  `<outer-prefix-before-where>.flatMap(A.B)` for map-like contexts or `<outer>.local(A.B)` (choice by parent kind table:
  filter-parents → `flatMap`, `repeat` body → `<outer>.flatMap(<body prefix>)` for the first iteration with a note).
- **Counts:** `prefix.limit(cap).count()` per step (cap 100k), run lazily in parallel (≤ 3 concurrent), cached per session.
- **Simulation:** at step k, fetch `prefix_k.limit(1000)` elements (ids); determine success by evaluating
  `g.V(ids).<remaining steps>` per element in batches via `g.V(ids).as('s').<remaining>.select('s').dedup().id()` (elements that survive). Works for vertex/edge streams; for value streams simulation is disabled with a note.
- **Inspection:** for one element id, evaluate prefixes of the remaining steps one by one (`g.V(id).<rem_1..j>.limit(1).count()`) until the count drops to 0 → that step is the drop point.
- **Session model:** `debugger::Session { stmt, steps, current, cache: HashMap<StepId, ResultSet|Count> }`; pure query generation is unit-tested without a server.

## Risks / Trade-offs

- [Re-executing prefixes on large graphs is expensive] → limits, timeouts, and cancellation of stale requests when the user steps quickly.
- [`repeat()` without bounds] → prefix inside repeat uses one iteration; counts use `times(1)` semantics with a note.

## Verification

- Unit tests (insta) for generated prefix, count, simulation and inspection queries.
- Integration tests on air-routes for each scenario.
- Headless editor tests for debug mode, navigation keys and inline counts.
