# 0033. Streaming GraphSON decoding with simd-json

- Status: accepted
- Date: 2026-10-11
- Refines: [0012](0012-graphson-v3-first.md)

## Context
The first GraphSON decoder parsed into a `serde_json::Value` tree, then converted it to `GValue`: 268 ms for a
10 MB edge payload (163 ms of parsing at ~60 MB/s plus 99 ms of conversion), against a 60 ms budget (ADR-0027).

## Decision
Decode response frames in one streaming pass: `GValue` implements `serde::Deserialize` via a visitor that
reads `@type`, then decodes `@value` with a type-specific seed straight into the model (`gremlin_model::fast`).
Frames are parsed with **simd-json** (`simd_json::serde::from_slice`, in place on the owned frame buffer).
The tree-based `graphson::decode` stays as the reference implementation; tests assert that both decoders
produce identical values for every fixture captured from a real Gremlin Server 3.7.3.

## Alternatives considered
- **serde_json streaming (same visitor)**: 78 ms, simpler, no new dependency; misses the budget.
- **Native simd-json tape → GValue**: possibly faster, but duplicates the visitor logic; revisit if needed.
- **mimalloc**: about 4% faster here; not worth a global allocator change on its own.
- **FxHash maps**: no measurable effect on decoding; kept, since it is cheaper for small maps.

## Consequences
- 10 MB decodes in ~60 ms (measured 58.9–60.0 ms), right at the budget with no margin; reference decoder 190–270 ms.
- Integration tests run ~2× faster.
- simd-json uses runtime CPU detection with a SWAR fallback, so it works on every x86-64-v2 and ARM CPU.

## Revisit when
Decoding shows up in profiles of real workloads, or simd-json maintenance stalls.
