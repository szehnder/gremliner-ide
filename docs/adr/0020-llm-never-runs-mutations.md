# 0020. The LLM never executes mutations; dry runs are read-only and bounded

- Status: accepted
- Date: 2026-10-11

## Context
Dry runs execute model-generated Gremlin against a real database, possibly production Neptune.

## Decision
Before any automatic execution, `gremlin-syntax` mutation detection checks for `addV, addE, property, drop,
mergeV, mergeE, sideEffect` with closures, `io()`, `call()` and any parse failure. If anything is detected or the
parse is incomplete, the query is **never** auto-run; it is shown with a "writes data" warning. Read dry runs
inject `.limit(N)` (default 25) before the terminal step and a server-side `evaluationTimeout` (default 5 s).
Connections may be marked **read-only**, in which case the editor also refuses mutations unless overridden per query.

## Alternatives considered
- **Run inside a transaction and roll back** — not supported uniformly (Neptune scripts autocommit).

## Consequences
- Some legit read queries using `sideEffect` with lambdas will not be dry-run; acceptable.

## Revisit when
A provider offers a reliable read-only session flag (then use it in addition).
