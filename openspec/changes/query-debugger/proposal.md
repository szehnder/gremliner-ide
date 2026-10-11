# Proposal

## Why

When a traversal returns nothing or the wrong thing, users need to see what happens at each step. G.V()'s debugger
(step over/into/out/back, traversal simulation, traverser inspection) is a standout feature; we match it and add per-step counts directly in the editor gutter.

## What Changes

- Debug session (Ctrl+F5) on the current read-only statement; the editor becomes read-only with the current step highlighted.
- Navigation: Step Over (F10), Step Into (F11, into anonymous traversals like `where`, `and`, `repeat`), Step Out (Shift+F11), Step Back (F9), and click any step to jump.
- At each step: run the prefix query up to that step (with limit and timeout) and show intermediate results in the normal results views (table, graph, JSON).
- Per-step traverser counts shown inline after each step (`· 1,204`), computed lazily with capped counts.
- Traversal simulation: list traversers at the selected step with paging (50 per page, up to 1,000), marking which ones produce a final result, with "hide failed".
- Traverser inspection: pick one element and see step by step where it drops out ("filtered at `has('runways', gt(3))`").
- Clear limitations: unavailable for mutating queries; for `by()`-modulated steps, `project`, `group`, `groupCount`, results are shown at the parent step only.

Out of scope: breakpoints across multiple statements; debugging mutations.

## Capabilities

### New Capabilities
- `query-debugger`: stepping through a traversal to inspect intermediate results, counts and why individual traversers are filtered out.

### Modified Capabilities
- None.

## Impact

- Crates: `debugger` (session logic, prefix/simulation query generation), `gremlin-syntax` (prefix-at-step, nested contexts), `ui-editor` (debug mode, inline counts), `ui-results`.
- ADRs: 0014, 0020.
