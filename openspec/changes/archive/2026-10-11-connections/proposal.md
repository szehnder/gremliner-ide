# Proposal

## Why

Users juggle several graph databases (local gremlin-server, LocalStack, AWS Neptune dev/prod). Connections must be
quick to set up, safe to store, clearly labelled and checked before use, like Compass's connection experience.

## What Changes

- The `GraphProvider` abstraction and capability matrix (ADR-0015), with the Gremlin Server provider implemented.
- Connection CRUD: name, colour tag, environment badge (local/dev/staging/prod), provider-specific form,
  read-only flag, default traversal source alias (`g`), default timeout.
- Paste a URL (`ws://`, `wss://`, `host:port`) and the provider and fields are filled in automatically.
- "Test connection" detects server reachability, TinkerPop version, serializer support and script-eval support.
- Secrets go in the OS keychain (ADR-0022); the DB stores only references.
- Connections sidebar: list with status dots, connect/disconnect, duplicate, delete, and search.
- `gremliner-cli connections list|add|test|remove`.

Out of scope: Neptune providers (M6), SSH/SSM tunnels (later), import/export of connection files (later).

## Capabilities

### New Capabilities
- `connections`: creating, editing, testing, organising and securely storing database connections.
- `provider-gremlin-server`: connecting to Apache TinkerPop Gremlin Server-compatible endpoints with optional username/password and TLS.

### Modified Capabilities
- None.

## Impact

- Crates: `providers`, `storage` (connections table, keyring), `ui-workspace` (sidebar, connection dialog), `gremliner-cli`.
- ADRs: 0015, 0022.
