# Tasks

## 1. Decisions and assets

- [x] 1.1 ADR-0040 (cargo-packager, native runners, Linux in Docker) supersedes ADR-0029; verify the ADR index lists both with the right status
- [x] 1.2 App icon `assets/icon/gremliner.svg` plus rendered PNGs; verify by viewing the 256 px PNG

## 2. Packaging

- [x] 2.1 `[package.metadata.packager]` in `crates/gremliner/Cargo.toml` (both binaries, icon, `.deb` dependencies, macOS minimum version, NSIS per-user); verify `scenario_release_packaging_packages_contain_the_app_and_the_cli` and `scenario_release_packaging_debian_package_declares_runtime_libraries`
- [x] 2.2 `cargo xtask package` with the formats table, names, checksums, tag check and smoke test; verify `scenario_release_packaging_package_formats_per_platform`, `scenario_release_packaging_artifact_names_include_version_os_and_architecture`, `scenario_release_packaging_every_artifact_has_a_checksum`, `scenario_release_packaging_mismatched_tag_is_rejected`, `scenario_release_packaging_wrong_binary_fails_the_smoke_test`
- [x] 2.3 `packaging/linux/Dockerfile` and `--docker`; verify `cargo xtask package --docker` produces the AppImage, `.deb` and `.tar.gz` in `target/packages/`, and `sha256sum -c` passes
- [x] 2.4 Verify the AppImage launches the app on the dev machine, and the `.deb` installs in `ubuntu:22.04` and `gremliner-cli --version` runs
- [x] 2.5 `--universal` for macOS (lipo); verified by the release workflow's macOS job

## 3. CI and release

- [x] 3.1 `.github/workflows/release.yml` (tag → release; manual run → artifacts) and `packaging/release-notes.md`; verify a manual run is green on all three runners

## 4. Documentation

- [x] 4.1 README: Download section, first launch of unsigned apps, system requirements, how to cut a release; AGENTS.md lists `cargo xtask package`; ROADMAP lists this change; verify by reading the rendered README on GitHub

## 5. Verification

- [x] 5.1 `cargo xtask ci` is green
- [x] 5.2 `openspec validate release-packaging --strict` passes
- [x] 5.3 Tag `v0.1.0`; verify the GitHub Release lists every package and its checksum
