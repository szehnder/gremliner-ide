# 0002. Rust as the single implementation language

- Status: accepted
- Date: 2026-10-11

## Context
The product goal is "absolute peak performance" and a native desktop feel: instant startup, low memory, 120 fps
interaction with large graphs, and heavy CPU work (layouts, parsing, JSON decoding) that must never stall the UI.

## Decision
All application code — UI, networking, parsing, layout, LLM integration, CLI, build tooling (`xtask`) — is Rust.
The only non-Rust sources are: the tree-sitter grammar definition (`grammar.js`, compiled to C at dev time and
checked in), Groovy seed scripts for the test gremlin-server, and shell glue in CI.

## Alternatives considered
- **TypeScript (Electron/Tauri front end)** — fastest path to web-like polish, but a WebView costs 100–300 MB RAM
  and WebKitGTK on Linux is noticeably slower for WebGL; splits the codebase across two languages.
- **C++ (Qt)** — comparable performance, but weaker safety for concurrent network/UI code and slower agentic iteration.
- **Zig / Odin** — immature GUI ecosystems.

## Consequences
- One toolchain, one test runner, one dependency graph; agents need only `cargo`.
- Shared crates between the GUI and the headless CLI (ADR-0024).
- Longer compile times, mitigated by the crate split (ADR-0004) and dependency optimization in dev profile.

## Revisit when
Never for the core; plugin/extension scripting may introduce a second language via a separate ADR.
