# Design

## Context

ADR-0040 chooses cargo-packager, native macOS and Windows runners, and an Ubuntu 22.04 Docker image for
Linux. It supersedes ADR-0029. The binary links `libdbus-1`, `libxcb` and `libxkbcommon(-x11)` and loads
Vulkan, Wayland and fonts at run time. SQLite is bundled. x86_64 builds target x86-64-v2 (ADR-0031,
`.cargo/config.toml`).

## Goals / Non-Goals

**Goals:**
- One command, `cargo xtask package`, that produces the same files locally and in CI.
- A release is a tag push; no manual upload steps.

**Non-Goals:**
- Signed or notarised builds, auto-update, more architectures (see the proposal).

## Decisions

- **Configuration** lives in `[package.metadata.packager]` of `crates/gremliner/Cargo.toml` (ADR-0040):
  product name, identifier, icons, both binaries, `.deb` dependencies, minimum macOS version, and NSIS
  per-user install. `xtask` passes the formats for the host OS on the command line, so the metadata stays
  OS-neutral.
- **Formats per OS** (one table in `xtask/src/package.rs`, which tests check):

  | OS | packager formats | archive |
  |---|---|---|
  | linux | `appimage`, `deb` | `.tar.gz` |
  | macos | `dmg` (contains the `.app`) | `.tar.gz` |
  | windows | `nsis` | `.zip` |

- **Names:** `Gremliner-<version>-<os>-<arch>.<ext>`. `os` is `linux`, `macos` or `windows`; `arch` is
  `x86_64`, `aarch64` or `universal`. Setup programs get a `-setup` suffix before `.exe`. Each file has a
  `<file>.sha256` next to it in `sha256sum` format, so `sha256sum -c` works. Everything goes to
  `target/packages/`.
- **Linux in Docker:** `packaging/linux/Dockerfile` (Ubuntu 22.04, rustup with the pinned toolchain, the
  `-dev` packages, cargo-packager). `--docker` runs `cargo xtask package` inside it as the calling user,
  with the repository mounted at `/work`. It uses a separate target directory (`target/docker`) so host
  and container builds don't overwrite each other, and `HOME`/`CARGO_HOME` under that directory so
  downloads are cached between runs. AppImage tools run with `APPIMAGE_EXTRACT_AND_RUN=1` (no FUSE in
  containers).
- **Build profile:** `dist`, which is `release` with no debug info and stripped symbols: the app binary goes
  from 307 MB to 61 MB, and each package is about 32 MB. cargo-packager finds the binaries from `--profile`
  and `--target`. Its `--binaries-dir` flag is parsed but ignored in 0.11.8.
- **macOS universal:** build `aarch64-apple-darwin` and `x86_64-apple-darwin`, combine them with `lipo` into
  `target/universal-apple-darwin/dist`, and run cargo-packager with `--target universal-apple-darwin`.
- **Windows subsystem:** release builds of the app use `windows_subsystem = "windows"`, so no console window
  opens beside it. Output on Windows comes from `gremliner-cli`.
- **Desktop entry:** `packaging/linux/gremliner.desktop` replaces cargo-packager's default to add
  `StartupWMClass=gremliner` (the window's app id) and search keywords. The AppImage uses it too.
- **Smoke test:** run the packaged `gremliner-cli --version` (from the staged archive) and require the
  workspace version in its output. This catches a missing or wrong-arch binary before anything is
  uploaded. The GUI is not launched, because CI has no display.
- **Tag check:** `--tag vX.Y.Z` must equal the workspace version, or packaging stops before building.
- **Release workflow:**
  - Three package jobs, each uploading `target/packages/*` as an artifact: `ubuntu-24.04` with `--docker`,
    `macos-15` with `--universal`, and `windows-2022`.
  - A `release` job, run only for tags, downloads the artifacts and runs
    `gh release create <tag> --notes-file packaging/release-notes.md`. The notes have the version
    filled in and describe download choices, first launch of unsigned apps, and system requirements.
- **Icon:** `assets/icon/gremliner.svg` is the source; the PNGs in `assets/icon/png/` are rendered with
  `rsvg-convert`. cargo-packager builds `.icns` and `.ico` from the PNGs.

## Risks / Trade-offs

- [Unsigned builds alarm users] → The README and release notes give the exact steps: right-click → Open
  on macOS, or `xattr -dr com.apple.quarantine`; on Windows, "More info → Run anyway".
- [Downloaded AppImage tooling (linuxdeploy) breaks] → `.deb` and `.tar.gz` are still built; a release can
  go out without the AppImage if needed.
- [Release builds are slow (thin LTO, codegen-units=1)] → `Swatinem/rust-cache` on native runners, and a
  persistent target directory for local Docker runs.

## Verification

- `cargo test -p xtask` covers formats, names, checksums, the tag check, `.deb` dependencies and the smoke
  check.
- `cargo xtask package --docker` on the dev machine produces the AppImage, `.deb` and `.tar.gz` in
  `target/packages/`, with checksums that pass `sha256sum -c`.
- The AppImage launches the app on the dev machine (Manjaro).
- `.deb` installs and launches in an `ubuntu:22.04` container (`dpkg -i`, `apt-get -f install`,
  `gremliner-cli --version`).
- A manual `release.yml` run is green on all three runners. Download its macOS and Windows artifacts and
  check the file list.
- Pushing the `v0.1.0` tag creates a release listing every package and checksum.
