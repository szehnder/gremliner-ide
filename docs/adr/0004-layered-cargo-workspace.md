# 0004. Layered Cargo workspace with strict dependency direction

- Status: accepted
- Date: 2026-10-11

## Context
Agents work best on small, well-bounded units with fast feedback. GPUI is heavy to compile; most logic (protocol,
parsing, layout, LLM) does not need it.

## Decision
A single Cargo workspace under `crates/` with three layers. Dependencies only point **downward**:

1. **Core (no UI, no tokio unless I/O):** `gremlin-model`, `gremlin-syntax`, `graph-store`, `graph-layout`, `schema`, `debugger`.
2. **I/O services (tokio):** `gremlin-client`, `providers`, `llm`, `assistant`, `storage`.
3. **UI (gpui-kit):** `graph-canvas`, `ui-editor`, `ui-results`, `ui-assistant`, `ui-workspace`, and the `gremliner` binary.

`gremliner-cli` depends on layers 1–2 only. `xtask` depends on nothing internal. Shared versions live in
`[workspace.dependencies]`; shared lints in `[workspace.lints]`.

## Alternatives considered
- **Single crate with modules** — slower incremental builds and no enforced boundaries.
- **Many micro-crates (one per feature)** — too much manifest overhead for the size of the project.

## Consequences
- Core crates compile and test in seconds and can be fully exercised by the headless CLI and unit tests.
- `cargo xtask ci` checks the layering rule (no layer-1 crate depends on tokio/gpui; no layer-2 crate depends on gpui).

## Revisit when
A crate exceeds ~15k lines or two crates are always changed together.
