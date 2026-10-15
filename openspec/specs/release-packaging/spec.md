# release-packaging Specification

## Purpose
Lets anyone download and run Gremliner from the project's releases without a development toolchain. CI builds
and publishes installers for macOS, Windows and Linux from a version tag.

## Requirements

### Requirement: Native installers per platform
Packaging SHALL produce the platform's usual installer plus a plain archive: a disk image with an application
bundle on macOS, a setup program on Windows, and a portable AppImage and a Debian package on Linux.

#### Scenario: Package formats per platform
- **WHEN** packaging runs on Linux, macOS and Windows
- **THEN** Linux produces an AppImage, a Debian package and a tar.gz archive
- **AND** macOS produces a disk image and a tar.gz archive
- **AND** Windows produces a setup program and a zip archive

### Requirement: App and CLI ship together
Every package SHALL contain both the desktop application and the headless command-line tool. The desktop
application SHALL be the program the installer launches.

#### Scenario: Packages contain the app and the CLI
- **WHEN** the packaging configuration is read
- **THEN** it lists the desktop application as the main program and the command-line tool as a second program

### Requirement: Predictable artifact names
Each artifact name SHALL include the product name, version, operating system and architecture, so downloads
can be told apart.

#### Scenario: Artifact names include version, OS and architecture
- **WHEN** version 0.1.0 is packaged for Linux on x86_64
- **THEN** the AppImage is named `Gremliner-0.1.0-linux-x86_64.AppImage`
- **AND** the Windows setup program for the same version is named `Gremliner-0.1.0-windows-x86_64-setup.exe`

### Requirement: Checksums
Every artifact SHALL have a SHA-256 checksum file in the standard `sha256sum` format.

#### Scenario: Every artifact has a checksum
- **WHEN** packaging writes an artifact
- **THEN** a `.sha256` file beside it holds the artifact's SHA-256 and file name, and verifying it succeeds

### Requirement: Releases match the version
A release built from a tag SHALL be refused when the tag does not equal `v` followed by the version being built.

#### Scenario: Mismatched tag is rejected
- **WHEN** packaging runs for tag `v0.2.0` while the version is 0.1.0
- **THEN** it fails before building and names both versions

### Requirement: Linux runtime dependencies declared
The Debian package SHALL declare the system libraries the application needs at run time, so the package manager
installs them.

#### Scenario: Debian package declares runtime libraries
- **WHEN** the Debian package configuration is read
- **THEN** it depends on the D-Bus, XCB, xkbcommon, xkbcommon-X11 and Vulkan loader libraries

### Requirement: Packages are smoke-tested
Packaging SHALL run the packaged command-line tool and SHALL fail if it does not report the version being
released.

#### Scenario: Wrong binary fails the smoke test
- **WHEN** the packaged command-line tool reports a different version, or cannot run
- **THEN** packaging fails and nothing is published
