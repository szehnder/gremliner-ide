//! `cargo xtask screenshot [--delay S] [--out FILE] [--keep-data] [-- <app args>]`
//!
//! Builds the release app, launches it with a fresh data directory (so it shows first-launch state unless
//! `--keep-data`), waits for it to settle, captures the app window to a PNG and lets the app exit.
//!
//! Capture backends, tried in order:
//! - KDE Plasma (Wayland or X11): `spectacle -b -n -a -o FILE` (the active window; the new window takes
//!   focus on launch).
//! - wlroots compositors: `grim` (window geometry from `swaymsg` or `hyprctl` when available).
//! - X11: `xdotool search --name Gremliner` + `import -window`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

pub struct Options {
    pub delay: f32,
    pub out: Option<PathBuf>,
    pub keep_data: bool,
    pub app_args: Vec<String>,
}

pub fn run(root: &Path, opts: Options) -> Result<()> {
    crate::run(
        "build release app",
        Command::new("cargo").args(["build", "--release", "-p", "gremliner"]),
    )?;
    let bin = root.join("target/release/gremliner");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let shots = root.join("target/screenshots");
    std::fs::create_dir_all(&shots)?;
    let out = opts
        .out
        .unwrap_or_else(|| shots.join(format!("gremliner-{stamp}.png")));

    let mut app = Command::new(&bin);
    // Keep the app alive comfortably past the capture.
    let exit_after = opts.delay + 4.0;
    app.args(["--exit-after", &format!("{exit_after}")]);
    let data_dir = shots.join(format!("data-{stamp}"));
    if !opts.keep_data && !opts.app_args.iter().any(|a| a == "--data-dir") {
        app.arg("--data-dir").arg(&data_dir);
    }
    app.args(&opts.app_args).stdout(Stdio::null());
    eprintln!("\n==> launch {} {:?}", bin.display(), opts.app_args);
    let mut child = app.spawn().context("failed to launch the app")?;

    std::thread::sleep(Duration::from_secs_f32(opts.delay));
    let captured = capture(&out);
    let status = child.wait()?;
    let _ = std::fs::remove_dir_all(&data_dir);
    captured?;
    if !status.success() {
        bail!("the app exited with {status}");
    }
    println!("{}", out.display());
    Ok(())
}

fn has(tool: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn capture(out: &Path) -> Result<()> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let out_str = out.to_string_lossy().to_string();
    if desktop.contains("KDE") && has("spectacle") {
        return crate::run(
            "capture (spectacle)",
            Command::new("spectacle").args(["-b", "-n", "-a", "-o", &out_str]),
        );
    }
    if wayland && has("grim") {
        let geometry = wlroots_window_geometry();
        let mut grim = Command::new("grim");
        if let Some(g) = &geometry {
            grim.args(["-g", g]);
        }
        return crate::run("capture (grim)", grim.arg(&out_str));
    }
    if has("xdotool") && has("import") {
        let id = String::from_utf8(
            Command::new("xdotool")
                .args(["search", "--name", "Gremliner"])
                .output()?
                .stdout,
        )?;
        let id = id
            .lines()
            .last()
            .context("no Gremliner window found")?
            .to_owned();
        return crate::run(
            "capture (import)",
            Command::new("import").args(["-window", &id, &out_str]),
        );
    }
    bail!(
        "no screenshot tool found: install spectacle (KDE), grim (wlroots Wayland) or xdotool + imagemagick (X11)"
    )
}

/// `x,y wxh` of the Gremliner window on sway or Hyprland.
fn wlroots_window_geometry() -> Option<String> {
    if has("hyprctl") {
        let json = Command::new("hyprctl")
            .args(["clients", "-j"])
            .output()
            .ok()?;
        let clients: serde_json::Value = serde_json::from_slice(&json.stdout).ok()?;
        let client = clients
            .as_array()?
            .iter()
            .find(|c| c["class"] == "gremliner")?;
        let at = &client["at"];
        let size = &client["size"];
        return Some(format!("{},{} {}x{}", at[0], at[1], size[0], size[1]));
    }
    if has("swaymsg") {
        let json = Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .output()
            .ok()?;
        let tree: serde_json::Value = serde_json::from_slice(&json.stdout).ok()?;
        let node = find_app(&tree, "gremliner")?;
        let r = &node["rect"];
        return Some(format!(
            "{},{} {}x{}",
            r["x"], r["y"], r["width"], r["height"]
        ));
    }
    None
}

fn find_app<'a>(node: &'a serde_json::Value, app_id: &str) -> Option<&'a serde_json::Value> {
    if node["app_id"] == app_id {
        return Some(node);
    }
    node["nodes"]
        .as_array()
        .into_iter()
        .chain(node["floating_nodes"].as_array())
        .flatten()
        .find_map(|n| find_app(n, app_id))
}
