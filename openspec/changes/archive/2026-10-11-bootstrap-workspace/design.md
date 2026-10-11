# Design

## Context

The repository already holds the minimal `gremliner` binary (gpui-kit hello window), ADRs, and OpenSpec scaffolding
produced during planning. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- `cargo xtask ci` is the single source of truth for "green", identical locally and in CI.
- Crate layering (ADR-0004) is machine-checked from day one.

**Non-Goals:**
- Release packaging (ADR-0029, later change).

## Decisions

- **xtask over Makefile/just** (ADR-0002: Rust only). `xtask` uses `std::process::Command` and `cargo metadata` JSON
  (`serde_json`) with no heavy deps.
- **Layering check** reads `cargo metadata`, classifies crates by an explicit table in `xtask/src/layering.rs`, and
  fails if a core crate's (transitive, normal) dependencies include `tokio` or `gpui-kit`, or an I/O crate's include `gpui-kit`.
- **Token check** greps `crates/ui-*/src` and `crates/graph-canvas/src` for `rgb(`, `hsla(`, `px(<digit>` outside
  `theme` modules; fails with file:line. An allowlist comment `// token-check: allow` permits rare exceptions.
- **spec-coverage** parses every `#### Scenario:` in `openspec/specs/**/spec.md` and `openspec/changes/*/specs/**/spec.md`,
  derives `scenario_<capability>_<slug>`, and greps `crates/**` for `fn <name>`. Report-only until `performance-budgets`, then enforced for archived specs.
- **CI matrix:** Linux runs the full `xtask ci` (with the testenv service container from the `testenv` change); macOS
  and Windows run `cargo build --workspace` only, to catch platform breakage early.
- `openspec validate --all --strict` runs inside `xtask ci` when `openspec` is on PATH (CI installs it via npm).

## Risks / Trade-offs

- [GPUI system deps on CI Linux] → install `libxkbcommon-dev libwayland-dev libvulkan-dev libfontconfig-dev libx11-xcb-dev` in the workflow; documented in README.
- [Long cold builds] → `Swatinem/rust-cache` in CI; dependency optimization profile locally.

## Verification

- `cargo xtask ci` passes locally.
- `cargo xtask layering` fails when a test-only edit adds `tokio` to `gremlin-model` (then revert).
- `cargo run -p gremliner` opens a window.
