# Proposal

## Why

Performance debugging needs to show where a traversal spends its time. TinkerPop's `profile()` and Neptune's
profile/explain APIs produce dense text; a visual breakdown makes slow steps obvious and feeds the assistant's Optimize mode.

## What Changes

- "Profile" (Ctrl/Cmd+Shift+Enter) and "Explain" actions on the current statement.
- Profile view: a tree of steps with traverser counts, time and percentage bars, the slowest step highlighted, and clicking a step highlighting its source range when it can be matched.
- Explain view: the traversal strategies applied and the final optimised traversal (TinkerPop `explain()`), or Neptune's explain output (with its operator table rendered as a table).
- Parsing for TinkerPop `TraversalMetrics` (GraphSON) and Neptune `/gremlin/profile` and `/gremlin/explain` text output.
- `gremliner-cli profile|explain`.

Out of scope: comparing profiles across runs (possible later).

## Capabilities

### New Capabilities
- `query-profile-explain`: profiling and explaining queries with visual breakdowns of time and strategies.

### Modified Capabilities
- None.

## Impact

- Crates: `providers` (profile/explain implementations), `gremlin-model` (metrics model), `ui-results` (views), `gremliner-cli`.
- ADRs: 0015.
