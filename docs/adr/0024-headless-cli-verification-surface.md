# 0024. Headless CLI as the agent's primary verification surface

- Status: accepted
- Date: 2026-10-11

## Context
Agents cannot easily look at a GUI. Most behaviour (connecting, querying, schema discovery, NL→Gremlin, debugging,
layout, render cost) is UI-independent.

## Decision
`gremliner-cli` exposes every layer-1/2 capability: `connect`, `query`, `schema`, `nl`, `explain`, `profile`,
`debug`, `layout-bench`, `render-bench`, `eval-assistant`. Output is human-readable by default and `--json` for
machine checking. It reads connections from the same SQLite store (or `--url` ad hoc).
For UI verification, the app accepts `--open-connection <name> --run <query> --perf-hud --exit-after <secs>`, and
`cargo xtask screenshot` captures the window (grim on Wayland, `import` on X11) for agents to inspect.

## Alternatives considered
- **GUI-only testing** — slow, flaky and opaque to agents.

## Consequences
- Every OpenSpec change touching layers 1–2 includes CLI-observable scenarios.

## Revisit when
GPUI gains offscreen rendering to image in tests, enabling pixel snapshots.
