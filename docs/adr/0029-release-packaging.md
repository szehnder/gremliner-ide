# 0029. Release packaging with cargo-dist

- Status: superseded by [0040](0040-installers-with-cargo-packager.md)
- Date: 2026-10-11

## Context
We target Linux (primary dev platform), macOS and Windows.

## Decision
Use `cargo-dist` for GitHub Release artifacts: Linux (tar.gz, AppImage/.deb later), macOS (.dmg, signed &
notarized when certificates exist), Windows (.msi). Release profile: thin LTO, codegen-units=1.

## Alternatives considered
- **Hand-rolled scripts** — more maintenance.

## Consequences
- Release work is its own late change; not on the v1 critical path.

## Revisit when
Auto-update is required.
