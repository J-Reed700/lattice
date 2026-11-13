//! Captures the log lines a test's own thread writes.

use tracing::subscriber::DefaultGuard;

#[derive(Clone, Default)]
struct Captured(std::sync::Arc<parking_lot::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The log lines written on this thread since [`capture_logs`].
pub struct CapturedLogs {
    lines: Captured,
    _guard: DefaultGuard,
}

/// Starts capturing this thread's log lines at `level` and more severe.
pub fn capture_logs(level: tracing::Level) -> CapturedLogs {
    let lines = Captured::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();
    CapturedLogs {
        lines,
        _guard: guard,
    }
}

impl CapturedLogs {
    /// Everything captured so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.lines.0.lock()).into_owned()
    }
}

#[cfg(test)]
mod tests;
