# 0035. A `ui-theme` crate for design tokens

- Status: accepted
- Date: 2026-10-11
- Amends: [0004](0004-layered-cargo-workspace.md) (adds a crate), [0023](0023-design-tokens-and-theming.md) (moves the token layer)

## Context
ADR-0023 put the token layer in `ui-workspace::theme`. `ui-workspace` depends on the feature UI crates
(`ui-editor`, `ui-results`, `ui-assistant`), so those crates could not use the tokens without a dependency
cycle, and would have had to duplicate spacing and colour decisions.

## Decision
Move the theme module (tokens, theme choice, bundled themes and fonts, the contrast test) into a leaf crate
`ui-theme` that depends only on gpui-kit. Every UI crate depends on it; `ui-workspace` re-exports it as
`ui_workspace::theme` so existing paths keep working. `ui-theme` is the one UI crate allowed raw colours
and pixel sizes (it is not scanned by `cargo xtask tokens`).

## Consequences
- Feature crates style themselves with the same tokens as the shell.
- Layering: `ui-theme` is a UI crate (may use gpui-kit) with no internal dependencies.

## Revisit when
User-defined themes need a runtime theme loader with its own lifecycle.
