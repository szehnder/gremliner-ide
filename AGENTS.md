# Gremliner IDE: guide for agents

Gremliner is a native desktop IDE for Gremlin (Apache TinkerPop) development and debugging, and contains a schema-checked natural-language assistant. Everything is
Rust; the UI is GPUI via `gpui-kit`.

## Where things are
- `docs/adr/`: architecture decisions, one per file. **Read the index before changing architecture.** Never edit
  an accepted ADR; write a new one that supersedes it.
- `openspec/`: spec-driven development. `specs/` is the current truth (filled as changes are archived).
  `changes/<name>/` holds in-flight work (proposal, design, tasks, delta specs). `ROADMAP.md` gives the order.
- `crates/`: the Cargo workspace (layering in ADR-0004):
  - core: `gremlin-model`, `gremlin-syntax`, `graph-store`, `graph-layout`, `schema`, `debugger`
  - I/O: `gremlin-client`, `providers`, `llm`, `assistant`, `storage`
  - UI: `ui-theme` (tokens, ADR-0035), `graph-canvas`, `ui-editor`, `ui-results`, `ui-assistant`, `ui-workspace`, `gremliner` (binary)
  - headless: `gremliner-cli`
- `xtask/`: `cargo xtask <ci|doctor|demo|testenv|bench|screenshot|package|spec-coverage|layering>`.
- `testenv/`: Docker gremlin-server plus seed data (ADR-0026).
- `packaging/`: the Linux packaging Docker image and the release notes template (ADR-0040). `.github/workflows/release.yml`
  publishes installers when a `v<version>` tag is pushed.

## Workflow
1. Pick the next unarchived change from `openspec/ROADMAP.md` (or `/opsx:propose` for new work).
2. `/opsx:apply <change>`: work through `tasks.md` top to bottom, ticking each box only after its verification passes.
3. Run `cargo xtask ci` (fmt, clippy -D warnings, layering, design-token check, tests, `openspec validate --all --strict`).
4. `/opsx:archive <change>` once CI is green and every scenario maps to a test.

## Rules
- **Layering:** core crates must not depend on tokio or gpui-kit; I/O crates must not depend on gpui-kit. `cargo xtask layering` enforces this.
- **Never block the UI thread.** I/O goes to the tokio runtime and CPU-heavy work to rayon or the background executor (ADR-0005).
- **No raw colours or pixel sizes in UI code;** use theme tokens (ADR-0023).
- **Scenario traceability:** every `#### Scenario:` in a spec has a test named `scenario_<capability>_<snake_slug>`.
- **New dependency of consequence, protocol or format choice, crate boundary, or perf budget → new ADR.**
- **The LLM never auto-executes mutations** (ADR-0020). Keep it that way.
- Integration tests need the testenv: `cargo xtask testenv up && cargo xtask testenv seed`. They skip with a message when it is not reachable.
- Prefer verifying through `gremliner-cli` (ADR-0024). For UI, use `cargo xtask screenshot -- --theme dark --command workspace::OpenCommandPalette` (fresh data dir; `--command` dispatches any action after the first frame) and look at the PNG. Prefer headless `#[gpui_kit::test]` UI tests (see `crates/ui-workspace/tests/shell.rs`); GUI launches pop windows on the developer's desktop.

## Useful commands
```
cargo xtask doctor                   # prerequisites, with install commands for what's missing
cargo xtask demo                     # testenv + air-routes + the app connected to it
cargo xtask ci                       # everything CI runs
cargo xtask testenv up|seed|down     # Docker gremlin-server with modern/crew/air-routes
cargo run -p gremliner               # the app
cargo run -p gremliner-cli -- query --url ws://localhost:8182/gremlin "g.V().count()"
cargo xtask bench --check            # performance budgets (ADR-0027); app measurements open windows: keep them visible
cargo xtask bench --check --ci       # relative-only, no windows (what CI runs against the main baseline)
cargo xtask package --docker         # Linux installers into target/packages (needs Docker; macOS: --universal)
openspec list && openspec validate --all --strict
```

## GPUI notes
GPUI (gpui-kit 0.7, re-exporting `gpui-pre` 0.3.x) changes quickly. When you learn an API pattern the hard way,
add it to `docs/gpui-notes.md` so the next agent doesn't have to rediscover it. The gpui-kit source is in
`~/.cargo/registry/src/*/gpui-kit-*`, `gpui-component-*` and `gpui-pre-*`; read it rather than guessing.
