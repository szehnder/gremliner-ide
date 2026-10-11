# 0001. Record architecture decisions as atomic MADR ADRs

- Status: accepted
- Date: 2026-10-11

## Context
Gremliner is developed largely by AI agents iterating from specs. Agents (and humans) need to know *why* the
codebase is shaped the way it is, so they do not undo deliberate trade-offs or re-litigate settled questions.

## Decision
Every substantial architectural choice gets its own file in `docs/adr/NNNN-kebab-title.md`, using the MADR-style
template in `template.md`. One decision per file. ADRs are immutable once accepted; a change of mind is a **new**
ADR that marks the old one `superseded by`. `docs/adr/README.md` is the index.

An ADR is required when a change: adds a third-party dependency of consequence, introduces or changes a crate
boundary, picks a protocol/format, sets a performance budget, or changes a security/privacy posture.

## Alternatives considered
- **One big ARCHITECTURE.md** — becomes stale and hides the history of decisions.
- **Decisions only in OpenSpec design.md** — design docs are per-change and get archived; cross-cutting decisions get lost.

## Consequences
- OpenSpec `design.md` files link to the ADRs they rely on rather than restating them.
- Slight overhead per decision; offset by agents not re-deriving context.

## Revisit when
The ADR count makes the index hard to scan (>100) — consider grouping by area.
