# Proposal

## Why

Not everyone writes Gremlin fluently, and even experts want to browse quickly. G.V()'s Graph Data Explorer builds
path searches from labels and filters with no code; it pairs naturally with our schema discovery and shows the generated Gremlin so users learn as they go.

## What Changes

- Graph Explorer tab per connection: a path builder alternating vertex and edge steps (starting and ending on a vertex), each chosen from schema labels or "any" (with direction for edges).
- Per-step filters by property with operators that depend on the property type (equals, not equals, greater/less than (or equal), between, inside, outside, starting with, ending with, containing, not containing, within, without).
- Options: limit (default 200), randomise, clear viewport on new search.
- "Search path from here" on any graph node pre-fills the start step with that vertex's id.
- The generated Gremlin is shown live and can be opened in an editor tab.
- Results render in the graph view with the normal interactions.

Out of scope: saved explorer searches (covered by saving the generated query).

## Capabilities

### New Capabilities
- `graph-explorer`: building path searches visually from schema labels and typed filters without writing Gremlin.

### Modified Capabilities
- None.

## Impact

- Crates: `ui-results` (explorer UI), `schema`, `gremlin-syntax` (query builder helpers).
- ADRs: 0030.
