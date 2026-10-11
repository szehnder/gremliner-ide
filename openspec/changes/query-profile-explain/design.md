# Design

## Context

`GraphProvider::profile/explain` defaults append `.profile()` / `.explain()` (ADR-0015); Neptune uses HTTP
endpoints (provider-neptune-aws). `gremlin-model` already decodes `g:TraversalMetrics`/`g:Metrics`.

## Goals / Non-Goals

**Goals:**
- One `ProfileReport` model for every provider.

**Non-Goals:**
- Flame graphs (tree with bars is enough).

## Decisions

- **Model:** `ProfileReport { total_ms, steps: Vec<StepMetric{name, count, traversers, dur_ms, pct, children, annotations}>, raw: String, extras: Vec<(title, text)> }`.
- **TinkerPop:** run `stmt.profile()`; map `Metrics` tree (`name`, `counts.elementCount/traverserCount`, `dur` ns, `annotations.percentDur`).
- **Neptune:** POST `/gremlin/profile` (`{"gremlin": stmt}` with `profile.results=false`?) returns a text report with sections
  (Original Traversal, Optimized Traversal, Physical Pipeline, Runtime, Traversal Metrics table, Index Operations);
  a section-splitting parser plus a table parser for the metrics block; unknown sections kept as extras.
- **Explain:** TinkerPop `explain()` returns `TraversalExplanation` (string form) → parse strategy lines; Neptune `/gremlin/explain` text kept with sections.
- **Step ↔ source matching:** match profile step names (e.g. `HasStep([~label.eq(person)])`) to `gremlin-syntax` step nodes in order by step kind; unmatched rows simply don't highlight.

## Risks / Trade-offs

- [Neptune text formats vary by engine version] → parser tolerant to unknown sections; fixtures for at least two engine versions.

## Verification

- Unit tests: TinkerPop metrics mapping (testenv), Neptune profile/explain parsing (fixtures, insta).
- Headless UI tests for bottleneck marking and step-to-source.
- `cargo run -p gremliner-cli -- profile --conn Local --json "g.V().out().out().path()"`.
