# 0023. Design tokens and theming for Compass-level polish

- Status: accepted
- Date: 2026-10-11

## Context
"High polish" means consistent spacing, typography, colour, motion and states across every surface — the quality
bar of MongoDB Compass, applied to a dense IDE.

## Decision
A single token layer in `ui-workspace::theme` maps onto gpui-kit's `Theme`: colour roles (surface, elevated,
border, accent, success, warning, danger, graph palette of 12 categorical colours validated for both themes),
spacing scale (4px base), radii, typography (Inter for UI, JetBrains Mono for code, embedded in `assets/fonts`),
motion durations (120/200/320 ms, ease-out). Light and dark themes ship; system preference by default. No raw
colours or pixel values in UI code — only tokens (enforced by a grep check in `xtask ci`).

## Alternatives considered
- **Use gpui-kit default theme as-is** — good base but not distinctive; we extend rather than replace it.

## Consequences
- A "polish pass" change (M8) audits every screen against the token rules and empty/loading/error states.

## Revisit when
User-defined themes are requested.
