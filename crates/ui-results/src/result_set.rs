//! The data of one query run, filled as batches stream in. Views observe it.

use std::ops::Range;
use std::time::{Duration, Instant};

use gpui_kit::{Context, EventEmitter};
use gremlin_model::GValue;
use gremlin_model::normalize::{self, ResultShape};
use gremlin_model::profile::{ExplainReport, ProfileReport};

/// Raw response text kept for the console's raw mode (beyond this, raw frames are dropped).
pub const RAW_LIMIT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunError {
    pub message: String,
    pub code: Option<u16>,
    /// Exception class names reported by the server, outermost first.
    pub exceptions: Vec<String>,
    /// Range in the editor document the error refers to, when the message names a position.
    pub range: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed(RunError),
    Cancelled,
}

/// A profile or explain run's report, shown instead of items.
#[derive(Debug, Clone, PartialEq)]
pub enum Analysis {
    /// `ranges[i]`: where top-level step `i` came from in the editor document, when it could be matched.
    Profile {
        report: ProfileReport,
        ranges: Vec<Option<Range<usize>>>,
    },
    Explain(ExplainReport),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultSetEvent {
    /// New items arrived or the status changed.
    Updated,
}

pub struct ResultSet {
    pub run_id: u64,
    /// The text the user ran.
    pub query: String,
    /// What was sent to the server (with `.toList()` appended when needed).
    pub sent: String,
    pub connection: String,
    pub items: Vec<GValue>,
    pub raw: Vec<String>,
    raw_bytes: usize,
    pub raw_truncated: bool,
    pub status: RunStatus,
    /// Maximum items kept.
    pub cap: usize,
    pub truncated: bool,
    started: Instant,
    pub elapsed: Option<Duration>,
    /// Server-side execution time, when the server reports it.
    pub server_time: Option<Duration>,
    /// Profile or explain report (profile and explain runs).
    pub analysis: Option<Analysis>,
    /// The view to show first (a tab setting), instead of choosing by the result's shape.
    pub preferred_view: Option<crate::ViewKind>,
}

impl EventEmitter<ResultSetEvent> for ResultSet {}

impl ResultSet {
    pub fn new(
        run_id: u64,
        query: impl Into<String>,
        sent: impl Into<String>,
        connection: impl Into<String>,
        cap: usize,
    ) -> Self {
        Self {
            run_id,
            query: query.into(),
            sent: sent.into(),
            connection: connection.into(),
            items: Vec::new(),
            raw: Vec::new(),
            raw_bytes: 0,
            raw_truncated: false,
            status: RunStatus::Running,
            cap,
            truncated: false,
            started: Instant::now(),
            elapsed: None,
            server_time: None,
            analysis: None,
            preferred_view: None,
        }
    }

    /// A finished result from known items (tests, previews).
    pub fn from_items(items: Vec<GValue>) -> Self {
        let mut set = Self::new(0, "", "", "", usize::MAX);
        set.items = items;
        set.status = RunStatus::Succeeded;
        set.elapsed = Some(Duration::ZERO);
        set
    }

    /// Attach a profile or explain report and finish successfully.
    pub fn set_analysis(&mut self, analysis: Analysis, cx: &mut Context<'_, Self>) {
        self.analysis = Some(analysis);
        self.finish(RunStatus::Succeeded, cx);
    }

    pub fn is_running(&self) -> bool {
        self.status == RunStatus::Running
    }

    pub fn shape(&self) -> ResultShape {
        normalize::shape(&self.items)
    }

    /// Time since the run started, or its final duration.
    pub fn elapsed_so_far(&self) -> Duration {
        self.elapsed.unwrap_or_else(|| self.started.elapsed())
    }

    /// Append a batch, honouring the cap. Returns false once the cap is reached (stop streaming).
    pub fn append(
        &mut self,
        items: Vec<GValue>,
        raw: Option<String>,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if !self.is_running() {
            return false;
        }
        if let Some(raw) = raw {
            if self.raw_bytes + raw.len() <= RAW_LIMIT_BYTES {
                self.raw_bytes += raw.len();
                self.raw.push(raw);
            } else {
                self.raw_truncated = true;
            }
        }
        let room = self.cap.saturating_sub(self.items.len());
        if items.len() > room {
            self.items.extend(items.into_iter().take(room));
            self.truncated = true;
        } else {
            self.items.extend(items);
        }
        cx.emit(ResultSetEvent::Updated);
        cx.notify();
        !self.truncated
    }

    pub fn finish(&mut self, status: RunStatus, cx: &mut Context<'_, Self>) {
        if !self.is_running() {
            return;
        }
        // A capped run that was stopped early still succeeded.
        self.status = status;
        self.elapsed = Some(self.started.elapsed());
        cx.emit(ResultSetEvent::Updated);
        cx.notify();
    }
}

/// Find `line N, column M` (Groovy compile errors) in a server message and map it to a byte range in
/// `script` (one character wide), offset by `base`.
pub fn error_range(message: &str, script: &str, base: usize) -> Option<Range<usize>> {
    let re = regex::Regex::new(r"line (\d+), column (\d+)").ok()?;
    let caps = re.captures(message)?;
    let line: usize = caps[1].parse().ok()?;
    let column: usize = caps[2].parse().ok()?;
    let line_start = script
        .split_inclusive('\n')
        .take(line.checked_sub(1)?)
        .map(str::len)
        .sum::<usize>();
    let line_text = script[line_start..].lines().next().unwrap_or("");
    let col_byte = line_text
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(line_text.len(), |(i, _)| i);
    let start = (line_start + col_byte).min(script.len());
    let end = script[start..]
        .chars()
        .next()
        .map_or(start, |c| start + c.len_utf8());
    Some(base + start..base + end)
}

#[cfg(test)]
mod tests {
    use super::error_range;

    #[test]
    fn maps_groovy_positions() {
        let script = "g.V().\n  nosuch(1)";
        let msg = "startup failed:\nScript1.groovy: 2: unexpected token @ line 2, column 3.";
        assert_eq!(
            error_range(msg, script, 10),
            Some(19..20),
            "line 2 starts at byte 7; column 3 is byte 9"
        );
        assert_eq!(error_range("no position here", script, 0), None);
    }
}
