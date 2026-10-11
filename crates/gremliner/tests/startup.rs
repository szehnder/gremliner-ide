//! Launches the real binary. It opens a window, so it runs only with `GREMLINER_GUI_TESTS=1` and a
//! display (the default run skips with a message).

use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn scenario_app_shell_exit_after_timeout() {
    if std::env::var_os("GREMLINER_GUI_TESTS").is_none() {
        eprintln!(
            "skipping: set GREMLINER_GUI_TESTS=1 to launch the app (opens a window for ~2 s)"
        );
        return;
    }
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_gremliner"))
        .args(["--exit-after", "2", "--no-persist"])
        .env("RUST_LOG", "gremliner=info")
        .output()
        .expect("failed to launch gremliner");
    let elapsed = started.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "exit status {:?}\n{stderr}",
        output.status
    );
    assert!(
        stderr.contains("first frame rendered"),
        "no first frame:\n{stderr}"
    );
    assert!(
        elapsed >= Duration::from_millis(1900) && elapsed < Duration::from_secs(10),
        "exited after {elapsed:?}"
    );
}
