# 0030. Query parameters are sent as native bindings

- Status: accepted
- Date: 2026-10-11

## Context
G.V() supports variables in queries with typed values, defaults and "run as report". String interpolation risks
injection and defeats server-side script caching.

## Decision
Free identifiers in a query that are not Gremlin keywords/steps/enums are detected by `gremlin-syntax` as
parameters. Values entered in the Parameters panel are parsed as JSON (unquoted `true`→bool, `42`→long; quote to
force string) and sent as `bindings` in the eval request. Parameter definitions (name, default, label) are saved
with the query.

## Alternatives considered
- **Textual substitution** — injection risk, breaks Neptune query plan cache.

## Consequences
- The assistant may emit parameterised queries; the dry run uses defaults or sample values.

## Revisit when
A provider rejects bindings (Cosmos DB has limits) — handle per capability.
