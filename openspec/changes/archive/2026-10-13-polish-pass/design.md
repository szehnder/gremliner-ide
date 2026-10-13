# Design

## Context

Tokens (ADR-0023) exist; gpui-kit provides AccessKit integration and `ElementSnapshot` for accessibility assertions in tests.

## Goals / Non-Goals

**Goals:**
- A repeatable audit: a checklist per surface plus automated checks where possible.

**Non-Goals:**
- Visual redesign.

## Decisions

- **Audit checklist:** `docs/ux/audit.md` lists every surface with checkboxes for tokens, states, keyboard, a11y, and screenshot links.
- **Automated checks:** accessibility snapshot test across main surfaces; token grep (already in CI); a test that every registered action with a keybinding appears in the shortcuts sheet.
- **Reduce motion:** read the OS preference via GPUI if exposed; else a setting; motion helper `animate(token)` returns zero duration when reduced.
- **Screenshots:** `cargo xtask screenshot --all-surfaces` scripts the app through each surface (flags open specific panels) and writes a gallery `target/screenshots/index.html` for review.

## Risks / Trade-offs

- [Subjective polish] → the screenshot gallery reviewed by the maintainer is the final gate.

## Verification

- Automated tests above; screenshot gallery review in light and dark themes.
