//! Minimal, dependency-free progress reporting for long operations.
//!
//! Draws a single carriage-return-updated line to stderr, and only when stderr
//! is a terminal — so piped or redirected output (the data on stdout) is never
//! touched, and scripts/CI stay silent. Set `MESH_NO_PROGRESS` to disable it
//! even on a terminal. `MESHX_NO_PROGRESS` is still accepted for older setups.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// A live counter rendered to stderr. Construct one, call [`Progress::inc`] per
/// item, and [`Progress::finish`] when done. Inactive instances are no-ops.
pub(crate) struct Progress {
    active: bool,
    label: &'static str,
    total: Option<u64>,
    done: u64,
    width: usize,
    start: Instant,
    last_draw: Option<Instant>,
}

impl Progress {
    /// A progress line with a known total (renders `done/total (pct%)`).
    pub(crate) fn sized(label: &'static str, total: u64) -> Self {
        Self::new(label, Some(total))
    }

    /// A progress line without a known total (renders a rising count).
    pub(crate) fn counter(label: &'static str) -> Self {
        Self::new(label, None)
    }

    fn new(label: &'static str, total: Option<u64>) -> Self {
        let active = std::io::stderr().is_terminal()
            && std::env::var_os("MESH_NO_PROGRESS").is_none()
            && std::env::var_os("MESHX_NO_PROGRESS").is_none();
        Progress {
            active,
            label,
            total,
            done: 0,
            width: 0,
            start: Instant::now(),
            last_draw: None,
        }
    }

    /// Record one completed item and redraw (throttled to ~10 fps).
    pub(crate) fn inc(&mut self) {
        self.done += 1;
        if !self.active {
            return;
        }
        let now = Instant::now();
        let due = self
            .last_draw
            .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(100));
        let complete = self.total == Some(self.done);
        if due || complete {
            self.last_draw = Some(now);
            self.draw(self.body());
        }
    }

    fn body(&self) -> String {
        match self.total {
            Some(total) => {
                let pct = if total == 0 {
                    100
                } else {
                    self.done.saturating_mul(100) / total
                };
                format!("{}: {}/{} ({}%)", self.label, self.done, total, pct)
            }
            None => format!("{}: {}", self.label, self.done),
        }
    }

    fn draw(&mut self, line: String) {
        let mut err = std::io::stderr();
        let pad = self.width.saturating_sub(line.chars().count());
        let _ = write!(err, "\r  {}{}", line, " ".repeat(pad));
        let _ = err.flush();
        self.width = line.chars().count();
    }

    /// Clear the line and print a one-line completion summary.
    pub(crate) fn finish(self) {
        if !self.active {
            return;
        }
        let secs = self.start.elapsed().as_secs_f64();
        let line = format!("{}: {} in {:.1}s", self.label, self.done, secs);
        let pad = (self.width + 2).saturating_sub(line.chars().count());
        let _ = writeln!(std::io::stderr(), "\r  {}{}", line, " ".repeat(pad));
    }
}
