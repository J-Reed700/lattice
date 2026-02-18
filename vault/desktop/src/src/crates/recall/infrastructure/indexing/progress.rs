use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub total_files: usize,
    pub processed: usize,
    pub failed: usize,
    pub current_file: Option<String>,
    pub status: IndexStatus,
    pub percentage: f32,
    pub estimated_remaining_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndexStatus {
    Idle,
    Scanning,
    Processing,
    Complete,
    Error,
    Cancelled,
}

impl IndexProgress {
    pub fn new() -> Self {
        Self {
            total_files: 0,
            processed: 0,
            failed: 0,
            current_file: None,
            status: IndexStatus::Idle,
            percentage: 0.0,
            estimated_remaining_ms: None,
        }
    }

    pub fn scanning() -> Self {
        Self {
            status: IndexStatus::Scanning,
            ..Self::new()
        }
    }

    pub fn with_total(total: usize) -> Self {
        Self {
            total_files: total,
            status: IndexStatus::Processing,
            ..Self::new()
        }
    }

    pub fn update(&mut self, current_file: Option<String>) {
        self.current_file = current_file;
        self.percentage = if self.total_files > 0 {
            (self.processed as f32 / self.total_files as f32) * 100.0
        } else {
            0.0
        };
    }

    pub fn increment_processed(&mut self) {
        self.processed += 1;
        self.update(None);
    }

    pub fn increment_failed(&mut self) {
        self.failed += 1;
        self.processed += 1;
        self.update(None);
    }

    pub fn complete(&mut self) {
        self.status = IndexStatus::Complete;
        self.current_file = None;
        self.percentage = 100.0;
    }

    pub fn error(&mut self, message: String) {
        self.status = IndexStatus::Error;
        self.current_file = Some(message);
    }

    pub fn cancel(&mut self) {
        self.status = IndexStatus::Cancelled;
        self.current_file = None;
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, IndexStatus::Scanning | IndexStatus::Processing)
    }
}

impl Default for IndexProgress {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ProgressTracker {
    tx: broadcast::Sender<IndexProgress>,
    current: IndexProgress,
    start_time: Option<std::time::Instant>,
}

impl ProgressTracker {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            current: IndexProgress::new(),
            start_time: None,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<IndexProgress> {
        self.tx.subscribe()
    }

    pub fn start_scanning(&mut self) {
        self.current = IndexProgress::scanning();
        self.start_time = Some(std::time::Instant::now());
        let _ = self.tx.send(self.current.clone());
    }

    pub fn set_total(&mut self, total: usize) {
        self.current.total_files = total;
        self.current.status = IndexStatus::Processing;
        self.update_estimates();
        let _ = self.tx.send(self.current.clone());
    }

    pub fn update_current_file(&mut self, path: PathBuf) {
        self.current.current_file = Some(path.display().to_string());
        self.update_estimates();
        let _ = self.tx.send(self.current.clone());
    }

    pub fn increment_processed(&mut self) {
        self.current.increment_processed();
        self.update_estimates();
        let _ = self.tx.send(self.current.clone());
    }

    pub fn increment_failed(&mut self) {
        self.current.increment_failed();
        self.update_estimates();
        let _ = self.tx.send(self.current.clone());
    }

    pub fn complete(&mut self) {
        self.current.complete();
        let _ = self.tx.send(self.current.clone());
        self.start_time = None;
    }

    pub fn error(&mut self, message: String) {
        self.current.error(message);
        let _ = self.tx.send(self.current.clone());
        self.start_time = None;
    }

    pub fn cancel(&mut self) {
        self.current.cancel();
        let _ = self.tx.send(self.current.clone());
        self.start_time = None;
    }

    fn update_estimates(&mut self) {
        if let Some(start_time) = self.start_time {
            if self.current.processed > 0 && self.current.total_files > 0 {
                let elapsed = start_time.elapsed();
                let avg_time_per_file = elapsed.as_millis() as f64 / self.current.processed as f64;
                let remaining_files = self.current.total_files - self.current.processed;
                let estimated_ms = (avg_time_per_file * remaining_files as f64) as u64;
                self.current.estimated_remaining_ms = Some(estimated_ms);
            }
        }
    }

    pub fn get_current(&self) -> IndexProgress {
        self.current.clone()
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

pub fn create_progress_stream(
    capacity: usize,
) -> (ProgressTracker, broadcast::Receiver<IndexProgress>) {
    let tracker = ProgressTracker::new(capacity);
    let receiver = tracker.subscribe();
    (tracker, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_creation() {
        let progress = IndexProgress::new();
        assert_eq!(progress.total_files, 0);
        assert_eq!(progress.processed, 0);
        assert_eq!(progress.failed, 0);
        assert_eq!(progress.status, IndexStatus::Idle);
        assert_eq!(progress.percentage, 0.0);
    }

    #[test]
    fn test_progress_percentage() {
        let mut progress = IndexProgress::with_total(100);
        assert_eq!(progress.percentage, 0.0);

        progress.processed = 50;
        progress.update(None);
        assert_eq!(progress.percentage, 50.0);

        progress.processed = 100;
        progress.update(None);
        assert_eq!(progress.percentage, 100.0);
    }

    #[test]
    fn test_progress_increment() {
        let mut progress = IndexProgress::with_total(10);

        progress.increment_processed();
        assert_eq!(progress.processed, 1);
        assert_eq!(progress.failed, 0);

        progress.increment_failed();
        assert_eq!(progress.processed, 2);
        assert_eq!(progress.failed, 1);
    }

    #[test]
    fn test_progress_complete() {
        let mut progress = IndexProgress::with_total(10);
        progress.processed = 10;
        progress.complete();

        assert_eq!(progress.status, IndexStatus::Complete);
        assert_eq!(progress.percentage, 100.0);
        assert!(progress.current_file.is_none());
    }

    #[test]
    fn test_progress_tracker() {
        let mut tracker = ProgressTracker::new(10);

        tracker.start_scanning();
        assert_eq!(tracker.get_current().status, IndexStatus::Scanning);

        tracker.set_total(5);
        assert_eq!(tracker.get_current().total_files, 5);
        assert_eq!(tracker.get_current().status, IndexStatus::Processing);

        tracker.increment_processed();
        assert_eq!(tracker.get_current().processed, 1);

        tracker.complete();
        assert_eq!(tracker.get_current().status, IndexStatus::Complete);
    }

    #[tokio::test]
    async fn test_progress_broadcast() {
        let mut tracker = ProgressTracker::new(10);
        let mut rx = tracker.subscribe();

        tracker.start_scanning();

        let progress = rx.recv().await.unwrap();
        assert_eq!(progress.status, IndexStatus::Scanning);

        tracker.set_total(5);
        let progress = rx.recv().await.unwrap();
        assert_eq!(progress.total_files, 5);
    }
}
