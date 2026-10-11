# 0009. Layouts run off the main thread and stream position frames

- Status: accepted
- Date: 2026-10-11

## Context
Force layouts for 10k+ nodes take tens of milliseconds per iteration; running them on the UI thread would stall input.

## Decision
`graph-layout` runs on a dedicated worker (rayon pool inside). It receives a snapshot (positions, edges, pinned
set) and emits position frames over a lock-free channel (latest-wins, `triple-buffer` style). The canvas animates
to the newest frame. Layout honours pinned nodes and supports incremental placement: newly expanded nodes start
near their parent so the picture stays stable.

## Alternatives considered
- **Compute to convergence, then show** — feels slow; users want to see motion.
- **GPU layout** — not portable under ADR-0006.

## Consequences
- The store owns authoritative positions; the canvas reads the latest frame; drags send pin updates to the worker.

## Revisit when
Layout iterations at 50k nodes exceed 50 ms on the reference machine.
