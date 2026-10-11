# 0005. A dedicated tokio runtime bridged to GPUI executors

- Status: accepted
- Date: 2026-10-11

## Context
GPUI has its own foreground/background executors. The networking ecosystem we need (tokio-tungstenite, reqwest,
AWS SDK) requires a tokio reactor. Blocking the GPUI main thread for even a few ms causes visible jank.

## Decision
The app starts one multi-threaded tokio runtime (2–4 worker threads) owned by a global `Runtime` handle. UI code
spawns I/O futures onto tokio via `runtime.spawn(..)` and awaits the `JoinHandle`/oneshot from a GPUI task
(`cx.spawn`), then updates entities on the foreground thread. CPU-heavy pure work (layout, big JSON normalisation)
goes to rayon or GPUI's background executor, never tokio workers.

Layer-2 crates expose plain `async fn` APIs and know nothing about GPUI.

## Alternatives considered
- **Run everything on GPUI's executor** — tokio-based crates panic without a reactor.
- **async-std / smol everywhere** — AWS SDK and reqwest are tokio-first.

## Consequences
- A small `bridge` module in `ui-workspace` provides `spawn_io(cx, fut) -> Task<T>`.
- Cancellation: dropping the GPUI task drops a `CancellationToken`/abort handle so queries can be cancelled.

## Revisit when
GPUI ships a first-class tokio compatibility layer in gpui-kit.
