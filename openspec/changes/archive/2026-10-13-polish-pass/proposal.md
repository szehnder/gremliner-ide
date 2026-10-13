# Proposal

## Why

Features alone don't make the experience; polish does. Before calling v1 done, every surface must meet the
Compass-level bar: consistent tokens, motion, keyboard access, helpful empty, loading and error states, and accessibility.

## What Changes

- Audit and fix every surface against the token rules (ADR-0023): spacing, type scale, colour roles, radii.
- Motion: panel open/close, tab switches, graph focus animations, toast transitions using standard durations; respect "reduce motion".
- Empty, loading and error states for every panel, with clear copy and next actions.
- Onboarding: first-run tour (3 steps) and "Try a sample" that starts the testenv-like sample connection guide.
- Keyboard: every action reachable from the keyboard; a keyboard shortcuts sheet (Ctrl/Cmd+K Ctrl/Cmd+S style chord or `?`); visible focus rings.
- Accessibility: AccessKit roles and names for all interactive elements; screen-reader labels for graph selection summaries.
- App icon and window titles; toasts for background events.

Out of scope: new features.

## Capabilities

### New Capabilities
- `ux-quality`: cross-cutting user-experience standards: states, motion, keyboard access, onboarding and accessibility.

### Modified Capabilities
- None.

## Impact

- Touches all UI crates. No new architecture.
- ADRs: 0023.
