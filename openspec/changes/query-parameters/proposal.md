# Proposal

## Why

Reusable queries need variables: `g.V().has('airport','code',code)` with `code` supplied separately. G.V()
detects variables, lets users set typed values and defaults, and runs saved queries as simple reports.

## What Changes

- Automatic parameter detection from the language service (free identifiers in argument positions).
- Parameters panel beside the editor: each parameter with a value field (JSON-typed: unquoted numbers and booleans, quoted strings), optional default and display label, and a type hint inferred from the schema when possible.
- Values are sent as bindings (ADR-0030); running with missing values prompts for them.
- Saved queries keep parameter definitions; "Run as report" from the sidebar opens a compact form and a results tab, with several report tabs possible for one query.

Out of scope: dashboards (later).

## Capabilities

### New Capabilities
- `query-parameters`: detecting, editing and binding query parameters, and running saved queries as parameterised reports.

### Modified Capabilities
- None.

## Impact

- Crates: `gremlin-syntax` (parameters already detected), `ui-editor` (parameters panel), `ui-workspace` (report tabs), `storage` (saved settings).
- ADRs: 0030.
