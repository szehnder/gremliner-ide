# Gremliner IDE

A native desktop IDE for developing and debugging [Apache TinkerPop](https://tinkerpop.apache.org/) Gremlin
queries. It aims for G.V()-level features (schema-aware editor, step-through debugger, fast graph view with
stylesheets, profile/explain), MongoDB Compass-level polish, and an AI assistant that turns plain English into
Gremlin **checked against your actual schema and test-run read-only** before you run it.

- **Stack:** Rust everywhere; GPUI via [gpui-kit](https://github.com/longbridge/gpui-kit) for the UI; tokio for I/O.
- **Databases:** Gremlin Server, Amazon Neptune (IAM/SigV4) and LocalStack Neptune, behind a provider abstraction so more can be added.
- **LLMs:** Anthropic, Google Gemini, and any OpenAI-compatible API (OpenAI, Ollama, LM Studio, OpenRouter, vLLM).

> Status: foundations. Features land change by change in the order given in [`openspec/ROADMAP.md`](openspec/ROADMAP.md).

## Quickstart

```sh
cargo run -p gremliner              # the desktop app
cargo run -p gremliner-cli -- --help
cargo xtask ci                      # everything CI checks
```

Linux needs the GPUI system libraries (Debian/Ubuntu names):
`libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libfontconfig-dev libx11-xcb-dev libsecret-1-dev`.

## How this repo is organised

| Path | What |
|---|---|
| [`AGENTS.md`](AGENTS.md) | How to work in this repo (humans and AI agents) |
| [`docs/adr/`](docs/adr/README.md) | Architecture decisions, one per file |
| [`openspec/`](openspec/) | Spec-driven development: current specs plus in-flight changes |
| [`openspec/ROADMAP.md`](openspec/ROADMAP.md) | Order of changes and milestone gates |
| `crates/` | Cargo workspace (layering rules in [ADR-0004](docs/adr/0004-layered-cargo-workspace.md)) |
| `xtask/` | `cargo xtask ci | layering | tokens | spec-coverage | testenv | bench | screenshot` |
| [`perf-budgets.toml`](perf-budgets.toml) | Performance budgets ([ADR-0027](docs/adr/0027-performance-budgets.md)) |

## License

Apache-2.0
