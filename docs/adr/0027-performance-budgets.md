# 0027. Performance budgets as CI gates

- Status: accepted
- Date: 2026-10-11

## Context
"Extremely performant" must be measurable or it regresses silently.

## Decision
`perf-budgets.toml` defines budgets checked by `cargo xtask bench --check`:
- Cold start to first frame < 300 ms (release, reference machine); idle RSS < 150 MB.
- Keystroke → highlighted & diagnosed < 8 ms for a 200-line query.
- GraphSON decode of 10 MB < 60 ms.
- Graph canvas paint CPU < 4 ms at 10k nodes / 20k edges; ≥ 60 fps at 50k / 100k.
- ForceAtlas2 air-routes settle < 2 s.
- Result table scroll at 100k rows: no frame > 16 ms.
CI runners are noisy, so CI checks relative regressions (>15% vs main baseline); absolute budgets are checked
locally on the reference machine before archiving perf-sensitive changes.

## Alternatives considered
- **Ad hoc profiling** — no regression protection.

## Consequences
- Benchmarks are first-class deliverables of relevant OpenSpec changes.

## Revisit when
Budgets are consistently beaten by 2× — tighten them.
