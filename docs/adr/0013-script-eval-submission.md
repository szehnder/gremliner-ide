# 0013. Submit queries as scripts (`eval`), not bytecode

- Status: accepted
- Date: 2026-10-11

## Context
Users type Gremlin text and the LLM produces Gremlin text. Converting text to bytecode requires a full Groovy-ish
interpreter. Neptune, Gremlin Server and JanusGraph all accept script submission; Neptune uses its own
grammar-based (non-Groovy) evaluator for scripts.

## Decision
All queries are submitted as `eval` scripts with bindings for parameters (ADR-0030). The client appends a terminal
step (`.toList()`) when missing, mirroring G.V()'s behaviour.

## Alternatives considered
- **Bytecode via our own text→bytecode translator** — large effort, little user value for an IDE.

## Consequences
- Servers with script evaluation disabled are unsupported (rare for dev/IDE use); detected on connection test.

## Revisit when
We add a server that only accepts bytecode.
