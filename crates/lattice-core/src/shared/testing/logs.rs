//! Captures the log lines a test's own thread writes, through the test process's one subscriber.

use std::cell::RefCell;
use std::sync::Arc;
use tracing::{subscriber::Interest, Level, Subscriber};
use tracing_subscriber::{
    filter::{dynamic_filter_fn, FilterExt},
    fmt::MakeWriter,
    registry::LookupSpan,
    Layer,
};

type Lines = Arc<parking_lot::Mutex<Vec<u8>>>;

struct Capture {
    level: Level,
    lines: Lines,
}

thread_local! {
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

/// The log lines written on this thread since [`capture_logs`].
pub struct CapturedLogs {
    lines: Lines,
}

/// Starts capturing this thread's log lines at `level` and more severe.
pub fn capture_logs(level: Level) -> CapturedLogs {
    super::subscriber::install();
    let lines = Lines::default();
    CAPTURE.with(|capture| {
        let mut capture = capture.borrow_mut();
        assert!(capture.is_none(), "one log capture per thread at a time");
        *capture = Some(Capture {
            level,
            lines: lines.clone(),
        });
    });
    CapturedLogs { lines }
}

impl CapturedLogs {
    /// Everything captured so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.lines.lock()).into_owned()
    }
}

impl Drop for CapturedLogs {
    fn drop(&mut self) {
        CAPTURE.with(|capture| *capture.borrow_mut() = None);
    }
}

struct ThisThread;

struct ThisThreadWriter;

impl std::io::Write for ThisThreadWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        CAPTURE.with(|capture| {
            if let Some(capture) = capture.borrow().as_ref() {
                capture.lines.lock().extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for ThisThread {
    type Writer = ThisThreadWriter;
    fn make_writer(&'a self) -> Self::Writer {
        ThisThreadWriter
    }
}

pub(super) fn layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(ThisThread)
        .with_filter(
            dynamic_filter_fn(|metadata, _| {
                CAPTURE.with(|capture| {
                    capture
                        .borrow()
                        .as_ref()
                        .is_some_and(|capture| metadata.level() <= &capture.level)
                })
            })
            .with_callsite_filter(|_| Interest::sometimes()),
        )
}

static PROCESS_LOG: parking_lot::Mutex<Option<std::fs::File>> = parking_lot::Mutex::new(None);

/// Writes this process's log lines, in the app's file-log format and at its
/// release filter, to `path` until the returned guard drops.
pub fn write_process_log(path: &std::path::Path) -> std::io::Result<ProcessLogGuard> {
    super::subscriber::install();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    *PROCESS_LOG.lock() = Some(file);
    Ok(ProcessLogGuard)
}

/// Closes the process log when dropped.
#[must_use = "the process log closes when this guard drops"]
pub struct ProcessLogGuard;

impl Drop for ProcessLogGuard {
    fn drop(&mut self) {
        *PROCESS_LOG.lock() = None;
    }
}

struct ProcessLog;

struct ProcessLogWriter;

impl std::io::Write for ProcessLogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Some(file) = PROCESS_LOG.lock().as_mut() {
            file.write_all(bytes)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for ProcessLog {
    type Writer = ProcessLogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        ProcessLogWriter
    }
}

pub(super) fn process_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    crate::shared::observability::file_log_layer(ProcessLog).with_filter(
        dynamic_filter_fn(|_, _| PROCESS_LOG.lock().is_some())
            .with_callsite_filter(|_| Interest::sometimes())
            .and(crate::shared::observability::env_filter(
                crate::shared::observability::DEFAULT_FILTER,
            )),
    )
}

#[cfg(test)]
mod tests;
