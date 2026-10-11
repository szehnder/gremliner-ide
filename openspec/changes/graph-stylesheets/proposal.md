# Proposal

## Why

Readable graphs depend on meaningful colour, size, captions and icons per label. G.V() offers stylesheets with
per-label rules, size-by-property and label density controls; we match this with a polished editor.

## What Changes

- Stylesheet model: per vertex label (colour, shape: circle/square/diamond/hexagon, icon from Lucide, caption template e.g. `{code} – {city}`, size fixed or bound to a numeric property with min/max) and per edge label (colour, width fixed or bound to a property, caption, dashed).
- Global settings: label density, minimum node size for labels, maximum caption length, show edge captions.
- Auto-assign: unknown labels get a stable categorical colour from the theme graph palette.
- Multiple named stylesheets, per-connection default, switchable from the graph toolbar; a side-panel editor with live preview.
- Import/export stylesheets as JSON.

Out of scope: rule conditions beyond label (e.g. "if property > X") — later.

## Capabilities

### New Capabilities
- `graph-stylesheets`: user-defined visual styling of graph elements by label, saved as reusable stylesheets.

### Modified Capabilities
- None.

## Impact

- Crates: `graph-store` (style index), `graph-canvas`, `ui-results` (stylesheet editor), `storage` (stylesheets table).
- ADRs: 0022, 0023.
