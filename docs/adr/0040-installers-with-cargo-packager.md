# 0040. Installers built with cargo-packager on native CI runners, Linux in Docker

- Status: accepted
- Date: 2026-10-14
- Supersedes: [0029](0029-release-packaging.md)

## Context
People who find the repository should be able to download the app from GitHub Releases and run it,
without Rust, a C toolchain, or the `-dev` packages from `cargo xtask doctor`. Gremliner is a GUI app with a
companion CLI, so each platform needs its usual desktop installer:

| platform | what users expect | why |
|---|---|---|
| macOS | `.dmg` holding `Gremliner.app` | a bare binary gets no Dock icon, menu bar name or Finder launch |
| Windows | a setup `.exe` | Start-menu entry and uninstaller |
| Linux | AppImage, `.deb` | AppImage runs on any distro; `.deb` integrates with apt-based desktops |

ADR-0029 picked `cargo-dist`. cargo-dist 0.32 builds archives plus shell, PowerShell, MSI and Homebrew
installers for command-line tools. It produces no `.app` bundle, no `.dmg`, no AppImage and no `.deb`, so
it doesn't fit a desktop app.

Cross-compiling every platform from one Docker image was considered. macOS needs Apple's SDK, which may only be
used on Apple hardware, and `hdiutil` to make a `.dmg`. Windows GUI builds need the MSVC toolchain and SDK
(`cargo-xwin` downloads them, under Microsoft's licence). GitHub provides free native macOS and Windows
runners for public repositories, so native builds are simpler and cost nothing.

On Linux, Docker helps. The binary links glibc and is only portable to systems with the same or newer glibc.
The `ubuntu-24.04` CI runner (glibc 2.39) would make builds that fail on Ubuntu 22.04 and Debian 12.

## Decision
- **Tool:** `cargo-packager` (CrabNebula, from Tauri's bundler; version pinned in CI and the Docker image),
  configured in `[package.metadata.packager]` of `crates/gremliner/Cargo.toml`. Both binaries,
  `gremliner` and `gremliner-cli`, ship in every package.
- **Formats:** macOS `.app` in a `.dmg`; Windows NSIS setup `.exe` (per-user install, no admin rights);
  Linux AppImage and `.deb`. Each platform also gets a plain archive (`.tar.gz`, or `.zip` on Windows).
- **Where packages are built:**
  - Linux: an Ubuntu 22.04 Docker image (`packaging/linux/Dockerfile`), so the glibc floor is 2.35.
  - macOS: one universal (arm64 + x86_64) build made with `lipo` on an Apple-silicon runner.
  - Windows: x86_64 on a Windows runner.
- **Entry point:** `cargo xtask package` runs the same steps locally and in CI: build release, run
  cargo-packager, add the archive, rename to `Gremliner-<version>-<os>-<arch>.<ext>`, write a SHA-256 for
  each file, and smoke-test the CLI binary that went into the package.
- **Publishing:** `.github/workflows/release.yml`. Pushing a `v<version>` tag builds all platforms and
  creates a GitHub Release with the packages and checksums. The tag must equal the workspace version.
  Running the workflow by hand builds the same packages as workflow artifacts, without a release.
- **No code signing yet.** Without certificates, macOS Gatekeeper and Windows SmartScreen warn on first
  launch. The release notes and README explain how to open the app anyway.

## Alternatives considered
- **cargo-dist (ADR-0029)**: no `.app`, `.dmg`, AppImage or `.deb`; built for CLI tools.
- **cargo-bundle**: covers `.app`, `.deb` and `.msi`, but no `.dmg` or AppImage, and it is barely
  maintained.
- **Cross-compiling everything in Docker** (osxcross, cargo-xwin): Apple SDK licence terms, MSVC SDK
  downloads, and fragile toolchains, all to replace runners that are free.
- **Build Linux directly on `ubuntu-22.04` runners**: would also give glibc 2.35. But it ties the result
  to GitHub's runner images, which are retired over time, and can't be reproduced locally on the
  developer's Manjaro machine. The Docker image does both.
- **Flatpak or Snap**: sandboxing gets in the way of the keychain (Secret Service), Docker test servers and
  SSH tunnels. They could be added later as extra formats.

## Consequences
- Releases need no local toolchain: tag, push, and CI publishes.
- Linux packages run on glibc 2.35 or newer (Ubuntu 22.04+, Debian 12+, Fedora 36+). x86_64 packages need
  an x86-64-v2 CPU (ADR-0031). The release notes state both.
- Linux arm64 and Windows arm64 are not built yet. macOS covers both architectures.
- Unsigned apps make for a rough first launch on macOS. Signing and notarisation can be added through
  cargo-packager's `macos.signingIdentity` / `notarizationCredentials` and `windows.signCommand`,
  once there are certificates to put in repository secrets.
- No auto-update: users download new versions from Releases.

## Revisit when
- Code-signing certificates become available.
- Users ask for auto-update, Linux arm64, Flatpak or `.rpm`.
- cargo-packager stops being maintained.
