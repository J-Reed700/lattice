use super::progress::{IndexProgress, IndexStatus, ProgressTracker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

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
}

impl IndexingState {
    pub fn new() -> Self {
        Self {
            tracker: Arc::new(RwLock::new(ProgressTracker::new(100))),
            cancellation_requested: Arc::new(AtomicBool::new(false)),
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
    }

    /// Signal cancellation
    pub fn cancel(&self) {
        self.cancellation_requested.store(true, Ordering::SeqCst);
        if let Ok(mut tracker) = self.tracker.write() {
            tracker.cancel();
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
                let (tx, rx) = broadcast::channel(1);
                rx
            })
    }
}

impl Default for IndexingState {
    fn default() -> Self {
        Self::new()
    }
}
