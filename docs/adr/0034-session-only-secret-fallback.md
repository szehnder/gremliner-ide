# 0034. Session-only secret fallback and log redaction

- Status: accepted
- Date: 2026-10-11
- Supersedes: the "encrypted file" fallback in [0022](0022-sqlite-and-keychain.md) (the rest of 0022 stands)

## Context
ADR-0022 stores secrets in the OS credential store and falls back to "an encrypted file with a warning" when
none is available. With no keychain, the key for that file has to live on the same disk as the file, so the
encryption protects nothing against anyone who can read the data directory, while suggesting that it does.
Separately, the `connections` change needs a guarantee that secrets never reach log files.

## Decision
- **Fallback:** when the credential store is unavailable (no Secret Service daemon, locked-down CI), secrets
  are kept **in memory for the session only** (`storage::secrets::MemoryStore`). The connection form shows a
  warning that the password will have to be re-entered after a restart. `GREMLINER_SECRET_STORE=memory`
  forces this mode (tests, CI).
- **Probing:** the store is probed lazily, the first time a secret is read or written, never at startup, so a
  locked KWallet/Keyring prompt cannot appear before the user asks for something that needs it.
- **Redaction:** every secret read or written is registered with `storage::secrets`, and the app's logger is
  wrapped in `RedactingLogger`, which masks registered values in every record. Error messages that quote a
  secret (e.g. a server echoing a bad password) are therefore safe to log. This uses the `log` facade the
  app already uses rather than the `tracing` layer the connections design sketched.

## Alternatives considered
- **Encrypted file with an on-disk key**: security theatre (see Context).
- **Encrypted file with a user master password**: a second password to manage for a desktop dev tool; may
  be revisited if users ask for portable, synced connection files.
- **Refuse to save passwords without a keychain**: blocks users on minimal Linux desktops entirely.

## Consequences
- On machines without a keychain, authenticated connections need the password once per session.
- Secrets shorter than 4 characters are not redacted (they would mangle ordinary log text).

## Revisit when
Connection import/export or a team-sharing feature needs secrets outside the OS store.
