# Proposal

## Why

Agents need a workspace where every crate boundary, lint, check and convention is already in place, so each later
change can focus on behaviour and get fast, trustworthy feedback from one command.

## What Changes

- Cargo workspace with all crates from ADR-0004 as compiling stubs (`lib.rs` with a module doc comment and a smoke test).
- `rust-toolchain.toml` pinned to stable 1.99; workspace lints; dev profile that optimizes dependencies (GPUI is slow at `opt-level=0`).
- `xtask` binary with: `ci`, `layering`, `tokens`, `spec-coverage`, `testenv`, `bench`, `screenshot` (the last three as stubs until their changes land).
- GitHub Actions workflow running `cargo xtask ci` on Linux, plus a build-only job on macOS and Windows.
- `AGENTS.md`/`CLAUDE.md`, `docs/adr/` (0001–0030), `docs/gpui-notes.md`, OpenSpec config and roadmap (already authored during planning; this change makes them pass CI).
- `perf-budgets.toml` with the ADR-0027 budgets (checked starting with `performance-budgets`).

Out of scope: any product behaviour, Docker test environment (`testenv`).

## Capabilities

### New Capabilities
- None. This change is developer tooling only (`skip_specs: true`).

### Modified Capabilities
- None.

## Impact

- Creates every crate directory under `crates/` and `xtask/`.
- Relies on ADR-0001, 0002, 0004, 0024, 0025, 0027.
