# Proposal

## Why

Trying Gremliner today means installing Rust 1.99, a C toolchain and several `-dev` packages, then waiting for a
long release build. Someone who finds the repository should be able to download an installer from GitHub Releases
and start playing in a minute.

## What Changes

- Packaging configuration for the desktop app and CLI. Packages: `.dmg` (macOS, universal), NSIS setup `.exe`
  (Windows), AppImage and `.deb` (Linux), plus a plain archive per platform.
- An app icon (`assets/icon/`), used by every installer.
- `cargo xtask package`: build release, package, give the files predictable names, write SHA-256 checksums,
  and smoke-test the packaged CLI. `--docker` builds the Linux packages in a pinned Ubuntu 22.04 image, and
  `--universal` makes the macOS universal binary.
- `.github/workflows/release.yml`: a `v*` tag builds every platform and publishes a GitHub Release. A
  manual run builds the same packages as workflow artifacts.
- README "Download" section, troubleshooting for first launches of unsigned apps, AGENTS.md and release
  notes.

Out of scope: code signing and notarisation (no certificates yet), auto-update, Linux and Windows arm64,
Flatpak, Snap and `.rpm`, and package-manager channels (Homebrew, winget, AUR).

## Capabilities

### New Capabilities
- `release-packaging`: downloadable installers for macOS, Windows and Linux, built and published by CI from
  a version tag.

### Modified Capabilities
- None.

## Impact

- `xtask` (new `package` command), `crates/gremliner/Cargo.toml` (packager metadata), `packaging/`,
  `assets/icon/`, the new release workflow, README, AGENTS.md.
- ADRs: 0040 (new; supersedes 0029), 0031 (x86-64-v2 baseline stated in the release notes), 0022 (the
  keychain on each OS, which packaged apps rely on).
