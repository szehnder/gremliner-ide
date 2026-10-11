# Design

## Context

ADR-0015 defines the provider trait; ADR-0022 storage and keychain. `gremlin-client` (previous change) handles the
wire. See proposal.md.

## Goals / Non-Goals

**Goals:**
- Provider-agnostic UI driven by a config enum and capability struct.

**Non-Goals:**
- Connection folders/groups (later; the schema leaves room via a nullable `group_id`).

## Decisions

- **Config model:** `ConnectionConfig { id: Uuid, name, color, environment, read_only, timeout_ms, provider: ProviderConfig }`
  with `ProviderConfig::GremlinServer { host, port, path, tls, accept_invalid_certs, username, password_ref, alias }`.
  Stored as JSON in `connections` table; secrets as `SecretRef(keyring service "gremliner", account = "<conn-id>/password")`.
- **Alias handling:** Gremlin Server evaluates `g` scripts against bound aliases; we send `aliases: {"g": "<alias>"}` in request args, which is standard TinkerPop.
- **Version detection:** try `Gremlin.version()` (script) → fall back to `g.inject(1)` success-only; TinkerPop
  version mapped to `Capabilities`. Failures don't block connecting.
- **URL parsing:** `url` crate; Neptune hostname regex `^[\w-]+\.(cluster-|cluster-ro-)?[\w]+\.([a-z0-9-]+)\.neptune\.amazonaws\.com$` → region.
- **Session registry:** `ui-workspace` holds `Entity<ConnectionManager>` with `HashMap<Uuid, ConnectionState>`; connecting creates a `Client` via the provider on the tokio runtime.
- **Mutation check for read-only:** relies on `gremlin-syntax::analyze().is_mutating` from the `gremlin-language`
  change; until that lands, a conservative keyword scan is used (documented TODO removed in gremlin-language).
- **Secret scrubbing:** a `tracing` layer redacts known secret values registered at runtime.

## Risks / Trade-offs

- [No keychain daemon on some Linux setups] → fallback encrypted file (ADR-0022) with a warning banner in the form.

## Verification

- `cargo test -p providers -p storage` (unit + testenv integration for test-connection and alias).
- `cargo run -p gremliner-cli -- connections add --name Local --url ws://localhost:8182/gremlin && cargo run -p gremliner-cli -- connections test Local`.
- Headless UI tests for the form (paste-to-fill, validation) and sidebar status.

## As built (deviations and additions)

- **Secrets fallback** is session-only memory, not an encrypted file, and the keychain is probed lazily
  (ADR-0034). `GREMLINER_SECRET_STORE=memory` forces it. Redaction wraps the `log` facade
  (`storage::secrets::RedactingLogger`) instead of a `tracing` layer.
- **Config:** `GremlinServerConfig.has_password` marks a stored secret (account `<id>/password`); the
  `password_ref` field of the sketch is implied by the id. `NeptuneAws` exists as a config variant so
  paste-to-fill can select it; connecting reports "not available yet" until `provider-neptune-aws`.
- **Provider trait** currently has `kind()` and `connect()`; profile/explain/schema join with the changes
  that use them. Connect runs `g.inject(1)` (proves alias and credentials, measures latency) and then
  `Gremlin.version()` for the TinkerPop version.
- **URL parsing** uses the `url` crate plus a suffix check for Neptune hostnames (no regex dependency).
- **TLS:** `accept_invalid_certs` is implemented in `gremlin-client` with a rustls verifier that skips chain
  and host-name checks but still verifies handshake signatures.
- **Paste detection:** a multi-character insertion into the host field is treated as a paste, so typing
  `host:8182` key by key is never rewritten mid-word.
- **Production badge:** shown in the status bar and the sidebar; the per-tab badge lands with
  `query-editor` (no editor tabs exist yet). **Read-only:** `providers::read_only::check` implements the
  block and the explicit override; the "Run anyway" confirmation UI lands with `query-execution`.
- **`--open-connection NAME`** now connects on startup.
