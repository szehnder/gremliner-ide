# 0031. x86-64-v2 as the minimum x86 CPU target

- Status: accepted
- Date: 2026-10-11

## Context
The default `x86_64` Rust target assumes only SSE2. Without SSE4.1, `f32::floor`/`round` compile to libm
calls. In the graph rasteriser's inner loop (ADR-0032) that cost ~1.5× throughput (4.76 → 3.16 ns per
pixel blend, `graph_store::view::raster::perf_probe`).

## Decision
Build x86_64 targets (Linux, Windows, macOS Intel) with `-C target-cpu=x86-64-v2` (SSE3/SSSE3/SSE4.1/SSE4.2/
POPCNT/CMPXCHG16B), configured in `.cargo/config.toml`. ARM64 targets are unaffected.

## Alternatives considered
- **x86-64-v3 (AVX2)** — faster still, but excludes some 2013–2015-era and low-end CPUs still in use.
- **Runtime dispatch (`multiversion`)** — more code for a handful of hot loops; revisit if needed.

## Consequences
- The app won't start on x86 CPUs older than ~2009 (pre-Nehalem/Bulldozer). Acceptable for a developer tool.
- Release notes must state the requirement.

## Revisit when
Profiling shows AVX2 would materially help a hot path, or user reports show v2 is too high.
