# Proposal

## Why

The editor is where users spend their time. It must be as responsive as Zed and as Gremlin-aware as G.V():
instant highlighting, inline errors, formatting, multiple queries per tab, and run-at-cursor.

## What Changes

- Gremlin editor tabs built on the gpui-kit code editor with the tree-sitter-gremlin highlighter.
- Inline diagnostics (squiggles + gutter + problems list under the editor), hover docs for steps.
- Format (Shift+Alt+F), toggle comment (Ctrl/Cmd+/), find/replace, multi-cursor (from gpui-kit), bracket matching.
- Multiple statements per tab; the current statement is subtly highlighted; Run at cursor (Ctrl/Cmd+Enter or Alt+Enter); run selection.
- Tabs bound to a connection (connection picker in the tab toolbar), drag to reorder, split into two columns, dirty marker, restored on restart.
- Per-tab options toolbar: timeout, "fetch edges between vertices", default results view.

Out of scope: schema autocomplete (autocomplete change), history/saved queries (query-history-saved), debugger gutter (query-debugger).

## Capabilities

### New Capabilities
- `query-editor`: editing Gremlin queries in tabs with language-aware assistance and keyboard-driven execution.

### Modified Capabilities
- None.

## Impact

- Crates: `ui-editor`, `ui-workspace`, `storage` (open tabs).
- ADRs: 0003, 0014, 0022.
