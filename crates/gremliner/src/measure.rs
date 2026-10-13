//! App-level measurements for `cargo xtask bench --check` (ADR-0027): time to first frame and idle
//! resident memory. Each prints one `measure <name>=<value> <unit>` line on stdout.

use std::sync::OnceLock;
use std::time::Instant;

static MAIN_START: OnceLock<Instant> = OnceLock::new();

/// Call first thing in `main`.
pub fn mark_start() {
    MAIN_START.get_or_init(Instant::now);
}

/// Milliseconds since [`mark_start`].
pub fn since_start_ms() -> f64 {
    MAIN_START
        .get()
        .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0)
}

pub fn report(name: &str, value: f64, unit: &str) {
    println!("measure {name}={value:.1} {unit}");
}

/// Resident set size of this process in MB, when the platform tells us.
pub fn resident_mb() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let kb: f64 = status
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))?
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kb / 1024.0)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()?;
        let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(kb / 1024.0)
    }
    #[cfg(target_os = "windows")]
    {
        // "gremliner.exe","1234","Console","1","123,456 K"
        let out = std::process::Command::new("tasklist")
            .args([
                "/FI",
                &format!("PID eq {}", std::process::id()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let field = text.trim().rsplit("\",\"").next()?;
        let kb: f64 = field
            .trim_matches(|c: char| !c.is_ascii_digit())
            .replace([',', '.', '\u{a0}'], "")
            .parse()
            .ok()?;
        Some(kb / 1024.0)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn reads_own_rss() {
        let mb = super::resident_mb().unwrap();
        assert!(mb > 1.0 && mb < 100_000.0, "{mb}");
    }
}
