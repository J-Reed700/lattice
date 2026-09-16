use super::progress::{IndexProgress, ProgressTracker};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, Notify};

/// The last N files that failed to index in this session, newest first.
///
/// Deliberately in memory, not in a table: this is the *in-flight* state of a run,
/// which `IndexingState` already owns and which `get_index_progress` already reads.
/// Persisting it would create a second source of truth for "how is indexing going".
/// It is cleared by `reset()` at the start of every run, and the UI says
/// "since this run started" rather than implying a permanent record.
pub const MAX_TRACKED_FAILURES: usize = 50;

/// One file that failed during the current indexing run.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexingFailure {
    /// Absolute path of the file that failed.
    pub path: String,
    /// File name only, for display.
    pub file_name: String,
    /// One-line reason, already `to_string()`d from the error.
    pub reason: String,
    /// RFC3339 timestamp.
    pub failed_at: String,
}

/// Shared state for indexing operations
///
/// Handles:
/// - Progress tracking (files processed, percentage, etc.)
/// - Cancellation signaling
/// - Event broadcasting
#[derive(Clone)]
pub struct IndexingState {
    /// Progress tracker with broadcast capability
    tracker: Arc<RwLock<ProgressTracker>>,

    /// Cancellation flag
    /// If true, running indexing jobs should stop as soon as possible
    cancellation_requested: Arc<AtomicBool>,

    /// Files that failed during the current run, newest first, bounded at
    /// `MAX_TRACKED_FAILURES`.
    failures: Arc<RwLock<VecDeque<IndexingFailure>>>,

    /// Pause flag for the directory-index loop. The indexing actor keeps its
    /// own `PauseGate`; this one governs the run the UI is showing.
    paused: Arc<AtomicBool>,

    /// Wakes loops parked in `wait_while_paused`.
    resume_notify: Arc<Notify>,
}

impl IndexingState {
    pub fn new() -> Self {
        Self {
            tracker: Arc::new(RwLock::new(ProgressTracker::new(100))),
            cancellation_requested: Arc::new(AtomicBool::new(false)),
            failures: Arc::new(RwLock::new(VecDeque::new())),
            paused: Arc::new(AtomicBool::new(false)),
            resume_notify: Arc::new(Notify::new()),
        }
    }

    /// Reset state for a new indexing job
    pub fn reset(&self) {
        if let Ok(_tracker) = self.tracker.write() {
            // Re-create tracker to clear state but keep subscribers?
            // ProgressTracker doesn't have reset(), but start_scanning() resets state.
            // Let's rely on start_scanning for now, or add reset to ProgressTracker.
            // For now, we'll just ensure it's in a clean state.
            // Ideally we'd replace the tracker but keep the channel...
            // ProgressTracker::new creates a NEW channel. We don't want that if we have subscribers.
            // We should use start_scanning() to reset for a new job.
        }
        self.cancellation_requested.store(false, Ordering::SeqCst);
        if let Ok(mut failures) = self.failures.write() {
            failures.clear();
        }
        // A new run never starts paused.
        self.resume();
    }

    /// Signal cancellation
    pub fn cancel(&self) {
        self.cancellation_requested.store(true, Ordering::SeqCst);
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.cancel();
        }
        // Wake a paused run so it can observe the cancellation instead of
        // parking forever on the resume notification.
        self.resume();
    }

    /// Record a file that failed. Oldest entries fall off at `MAX_TRACKED_FAILURES`.
    pub fn record_failure(&self, path: &std::path::Path, reason: String) {
        let failure = IndexingFailure {
            path: path.display().to_string(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            // Errors can carry a whole multi-line message; one line is what the UI shows.
            reason: reason
                .lines()
                .next()
                .unwrap_or("Unknown error")
                .trim()
                .to_string(),
            failed_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Ok(mut failures) = self.failures.write() {
            failures.push_front(failure);
            while failures.len() > MAX_TRACKED_FAILURES {
                failures.pop_back();
            }
        }
    }

    /// Newest first. Empty when nothing has failed since the last `reset()`.
    pub fn failures(&self) -> Vec<IndexingFailure> {
        self.failures
            .read()
            .map(|f| f.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Drop one failure once the user has retried or removed it.
    pub fn clear_failure(&self, path: &str) {
        if let Ok(mut failures) = self.failures.write() {
            failures.retain(|f| f.path != path);
        }
    }

    /// Pause the directory-index loop.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume a paused directory-index loop.
    pub fn resume(&self) {
        if self.paused.swap(false, Ordering::SeqCst) {
            self.resume_notify.notify_waiters();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Block while paused. Returns immediately when not paused, and also when
    /// cancellation has been requested so Stop can break a paused run.
    ///
    /// The `notified()` future is created *and enabled* before the flag is
    /// re-read. `Notify::notified()` does not register the waiter until the
    /// future is first polled, and `notify_waiters()` stores no permit — so
    /// without `enable()` a `resume()` landing between the flag read and the
    /// `.await` is dropped and the run parks forever with nothing left to wake
    /// it. `enable()` registers interest up front, which closes that window.
    pub async fn wait_while_paused(&self) {
        loop {
            let notified = self.resume_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if !self.paused.load(Ordering::SeqCst) || self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }

    /// Check if cancellation was requested
    pub fn is_cancelled(&self) -> bool {
        self.cancellation_requested.load(Ordering::SeqCst)
    }

    /// Update progress: start scanning
    pub fn start_scanning(&self) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.start_scanning();
        }
    }

    /// Update progress: set total files and switch to processing
    pub fn set_total_files(&self, total: usize) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.set_total(total);
        }
    }

    /// Update progress: current file being processed
    pub fn set_current_file(&self, path: String) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.update_current_file(std::path::PathBuf::from(path));
        }
    }

    /// Update progress: file processed successfully
    pub fn file_processed(&self) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.increment_processed();
        }
    }

    /// Update progress: file failed
    pub fn file_failed(&self) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.increment_failed();
        }
    }

    /// Update progress: job complete
    pub fn complete(&self) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.complete();
        }
    }

    /// Update progress: error occurred
    pub fn error(&self, message: String) {
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.error(message);
        }
    }

    /// Get current progress snapshot
    pub fn get_snapshot(&self) -> IndexProgress {
        self.tracker
            .read()
            .map(|t| t.get_current())
            .unwrap_or_else(|_| IndexProgress::new())
    }

    /// Subscribe to progress events
    pub fn subscribe(&self) -> broadcast::Receiver<IndexProgress> {
        self.tracker
            .read()
            .map(|t| t.subscribe())
            .unwrap_or_else(|_| {
                // Fallback if lock fails (shouldn't happen)
                let (_tx, rx) = broadcast::channel(1);
                rx
            })
    }
}

impl Default for IndexingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn record_failure_keeps_newest_first() {
        let state = IndexingState::new();
        state.record_failure(Path::new("/vault/a.pdf"), "boom a".to_string());
        state.record_failure(Path::new("/vault/b.pdf"), "boom b".to_string());

        let failures = state.failures();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].path, "/vault/b.pdf");
        assert_eq!(failures[0].file_name, "b.pdf");
        assert_eq!(failures[1].path, "/vault/a.pdf");
    }

    #[test]
    fn record_failure_is_bounded_at_max_tracked_failures() {
        let state = IndexingState::new();
        for i in 0..(MAX_TRACKED_FAILURES + 10) {
            state.record_failure(Path::new(&format!("/vault/{}.pdf", i)), "boom".to_string());
        }

        let failures = state.failures();
        assert_eq!(failures.len(), MAX_TRACKED_FAILURES);
        // Newest first: the last file recorded is at the head, the oldest fell off.
        assert_eq!(
            failures[0].path,
            format!("/vault/{}.pdf", MAX_TRACKED_FAILURES + 9)
        );
        assert!(failures.iter().all(|f| f.path != "/vault/0.pdf"));
    }

    #[test]
    fn record_failure_takes_only_the_first_line_of_a_multiline_reason() {
        let state = IndexingState::new();
        state.record_failure(
            Path::new("/vault/a.pdf"),
            "  Couldn't extract text from this PDF.  \nCaused by: bad xref\nand more".to_string(),
        );

        let failures = state.failures();
        assert_eq!(failures[0].reason, "Couldn't extract text from this PDF.");
    }

    #[test]
    fn reset_clears_failures_and_unpauses() {
        let state = IndexingState::new();
        state.record_failure(Path::new("/vault/a.pdf"), "boom".to_string());
        state.pause();
        assert!(state.is_paused());

        state.reset();

        assert!(state.failures().is_empty());
        assert!(!state.is_paused());
        assert!(!state.is_cancelled());
    }

    #[test]
    fn clear_failure_removes_only_the_matching_path() {
        let state = IndexingState::new();
        state.record_failure(Path::new("/vault/a.pdf"), "boom".to_string());
        state.record_failure(Path::new("/vault/b.pdf"), "boom".to_string());

        state.clear_failure("/vault/a.pdf");

        let failures = state.failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, "/vault/b.pdf");
    }

    /// `clear_indexing_failure` is this call behind a command. Dismissing a row
    /// has to change what every later reader of the snapshot sees, and an
    /// unknown path (aged out past `MAX_TRACKED_FAILURES`, or cleared by a new
    /// run) must be a no-op rather than a failure the UI has to explain.
    #[test]
    fn clearing_a_failure_changes_the_snapshot_every_view_reads() {
        let state = IndexingState::new();
        state.record_failure(Path::new("/vault/a.pdf"), "boom".to_string());
        state.record_failure(Path::new("/vault/b.pdf"), "boom".to_string());

        state.clear_failure("/vault/a.pdf");
        assert!(
            state.failures().iter().all(|f| f.path != "/vault/a.pdf"),
            "a dismissed file must not come back on the next read"
        );

        state.clear_failure("/vault/never-seen.pdf");
        assert_eq!(
            state.failures().len(),
            1,
            "clearing an unknown path leaves the list alone"
        );
    }

    #[tokio::test]
    async fn wait_while_paused_returns_immediately_when_not_paused() {
        let state = IndexingState::new();
        tokio::time::timeout(Duration::from_millis(200), state.wait_while_paused())
            .await
            .expect("wait_while_paused must not block when the run is not paused");
    }

    #[tokio::test]
    async fn wait_while_paused_wakes_on_resume() {
        let state = IndexingState::new();
        state.pause();

        let resumer = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            resumer.resume();
        });

        tokio::time::timeout(Duration::from_secs(2), state.wait_while_paused())
            .await
            .expect("resume() must wake a parked waiter");
        assert!(!state.is_paused());
    }

    #[tokio::test]
    async fn wait_while_paused_returns_when_cancelled_while_paused() {
        let state = IndexingState::new();
        state.pause();

        let canceller = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            canceller.cancel();
        });

        tokio::time::timeout(Duration::from_secs(2), state.wait_while_paused())
            .await
            .expect("cancel() must break a paused run instead of hanging forever");
        assert!(state.is_cancelled());
    }

    /// A `resume()` racing the waiter's registration must not be lost.
    ///
    /// `Notify::notify_waiters()` stores no permit, so a waiter that has not yet
    /// registered when the resume lands would park forever. Multi-threaded and
    /// repeated so the window is actually exercised.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn wait_while_paused_does_not_lose_a_racing_resume() {
        for _ in 0..500 {
            let state = IndexingState::new();
            state.pause();

            let resumer = state.clone();
            let handle = tokio::spawn(async move {
                resumer.resume();
            });

            tokio::time::timeout(Duration::from_secs(5), state.wait_while_paused())
                .await
                .expect("a resume() racing registration must still wake the waiter");
            let _ = handle.await;
        }
    }
}
