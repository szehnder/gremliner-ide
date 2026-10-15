Native installers for macOS, Windows and Linux. You don't need Rust or any developer tools to try Gremliner;
download the file for your system below.

| System | Download | Notes |
|---|---|---|
| macOS 11+ (Apple silicon and Intel) | `Gremliner-{{version}}-macos-universal.dmg` | Open it and drag Gremliner to Applications |
| Windows 10/11 (x64) | `Gremliner-{{version}}-windows-x86_64-setup.exe` | Installs for your user; no admin rights needed |
| Linux, any distro (x64) | `Gremliner-{{version}}-linux-x86_64.AppImage` | `chmod +x` it, then run it |
| Debian, Ubuntu (x64) | `Gremliner-{{version}}-linux-x86_64.deb` | `sudo apt install ./Gremliner-{{version}}-linux-x86_64.deb` |
| Anything else | `.tar.gz` / `.zip` | Both programs, unpacked and ready to run |

Every package contains the desktop app (`gremliner`) and the headless CLI (`gremliner-cli`). Each file has a
`.sha256` beside it; check it with `sha256sum -c <file>.sha256` (on macOS, `shasum -a 256 -c`).

### First launch

These builds are not code-signed yet, so the first launch shows a warning:

- **macOS** says the app "can't be opened" or "is damaged". Right-click Gremliner in Applications and choose
  **Open**. If that doesn't work, go to **System Settings → Privacy & Security** and click **Open Anyway**. Or
  run `xattr -dr com.apple.quarantine /Applications/Gremliner.app`.
- **Windows** SmartScreen says it "protected your PC". Click **More info → Run anyway**.

### Requirements

- **Linux:** glibc 2.35 or newer (Ubuntu 22.04+, Debian 12+, Fedora 36+), a Vulkan driver (Mesa is fine), and
  a Secret Service keyring such as GNOME Keyring or KWallet for saved passwords. Without one, set
  `GREMLINER_SECRET_STORE=memory`.
- **x86_64:** a CPU from about 2009 or later (x86-64-v2: SSE4.2 and POPCNT).
- **A Gremlin database to connect to.** To try one locally with Docker, with TinkerPop's small "modern"
  sample graph: `docker run -p 8182:8182 tinkerpop/gremlin-server:3.7.3 conf/gremlin-server-modern.yaml`.
  Then connect to `ws://localhost:8182/gremlin`.

See the [README](https://github.com/szehnder/gremliner-ide#readme) for connecting to Amazon Neptune and setting up the AI assistant.
