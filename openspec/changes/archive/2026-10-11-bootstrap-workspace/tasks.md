# Tasks

## 1. Workspace

- [x] 1.1 Create stub crates for every crate in ADR-0004 (`Cargo.toml` using workspace package/lints, `src/lib.rs` with a doc comment and one passing test); verify `cargo build --workspace` succeeds
- [x] 1.2 Create `crates/gremliner-cli` with clap and a `version` subcommand; verify `cargo run -p gremliner-cli -- version` prints the version
- [x] 1.3 Keep `crates/gremliner` opening a gpui-kit window; verify `cargo run -p gremliner` shows a window (manual, once)
- [x] 1.4 Add `perf-budgets.toml` with the ADR-0027 budgets; verify it parses with `cargo xtask bench --list`

## 2. xtask

- [x] 2.1 Implement `cargo xtask layering` from `cargo metadata`; verify it passes on the clean tree and fails when `tokio` is added to `gremlin-model`
- [x] 2.2 Implement `cargo xtask tokens` (design-token grep); verify it flags an `rgb(` literal in a temporary file under `crates/ui-workspace/src`
- [x] 2.3 Implement `cargo xtask spec-coverage` (report mode); verify it lists unmapped scenarios for the in-flight changes
- [x] 2.4 Implement `cargo xtask ci` = fmt --check, clippy -D warnings, layering, tokens, test, `openspec validate --all --strict`; verify exit code 0
- [x] 2.5 Stub `testenv`, `bench`, `screenshot` subcommands with "not yet implemented (see change X)" messages; verify `cargo xtask --help` lists all subcommands

## 3. CI

- [x] 3.1 Add `.github/workflows/ci.yml` (Linux full CI with system deps and rust-cache; macOS/Windows build-only); verify with `actionlint` if available or by YAML lint
- [x] 3.2 Add `README.md` (what it is, quickstart, links to AGENTS.md, ADR index and roadmap); verify links resolve

## 4. Verification

- [x] 4.1 `cargo xtask ci` is green
- [x] 4.2 `openspec validate bootstrap-workspace --strict` passes

## Workflow follow-up

- Archive with `/opsx:archive bootstrap-workspace`.
