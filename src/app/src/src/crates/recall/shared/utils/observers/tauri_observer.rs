use crate::infrastructure::indexing::events::IndexingEvent;
use crate::patterns::observer::Observer;
use async_trait::async_trait;
use serde::Serialize;
use tauri::{Emitter, Window};

pub struct TauriIndexingObserver {
    window: Window,
}

impl TauriIndexingObserver {
    pub fn new(window: Window) -> Self {
        Self { window }
    }
}

// DTOs to match frontend expectations
#[derive(Serialize)]
struct IndexingProgressDto {
    current: usize,
    total: usize,
    filename: String,
}

#[derive(Serialize)]
struct IndexingStartedDto {
    path: String,
    total_files: Option<usize>,
}

#[derive(Serialize)]
struct IndexingCompleteDto {
    path: String,
    indexed_count: usize,
    duration_ms: Option<u64>,
}

#[derive(Serialize)]
struct IndexingErrorDto {
    message: String,
    path: String,
    filename: Option<String>,
}

#[async_trait]
impl Observer<IndexingEvent> for TauriIndexingObserver {
    async fn notify(&self, event: &IndexingEvent) {
        match event {
            IndexingEvent::Started { total_files } => {
                let dto = IndexingStartedDto {
                    path: String::new(), // Will be filled by context
                    total_files: Some(*total_files),
                };
                if let Err(e) = self.window.emit_to(self.window.label(), "indexing-started", dto) {
                    tracing::error!("Failed to emit indexing-started: {}", e);
                }
            }
            IndexingEvent::FileStarted { path, current, total } => {
                let dto = IndexingProgressDto {
                    current: *current,
                    total: *total,
                    filename: path.clone(),
                };
                if let Err(e) = self.window.emit_to(self.window.label(), "indexing-progress", dto) {
                    tracing::error!("Failed to emit indexing-progress: {}", e);
                }
            }
            IndexingEvent::FileCompleted { path, current, total, .. } => {
                // Also emit progress for completed files
                let dto = IndexingProgressDto {
                    current: *current,
                    total: *total,
                    filename: path.clone(),
                };
                if let Err(e) = self.window.emit_to(self.window.label(), "indexing-progress", dto) {
                    tracing::error!("Failed to emit indexing-progress: {}", e);
                }
            }
            IndexingEvent::FileError { path, error, .. } => {
                let dto = IndexingErrorDto {
                    message: error.clone(),
                    path: path.clone(),
                    filename: Some(path.clone()),
                };
                if let Err(e) = self.window.emit_to(self.window.label(), "indexing-error", dto) {
                    tracing::error!("Failed to emit indexing-error: {}", e);
                }
            }
            IndexingEvent::Completed { total_files, duration_ms, .. } => {
                let dto = IndexingCompleteDto {
                    path: String::new(), // Will be filled by context
                    indexed_count: *total_files,
                    duration_ms: Some(*duration_ms),
                };
                if let Err(e) = self.window.emit_to(self.window.label(), "indexing-complete", dto) {
                    tracing::error!("Failed to emit indexing-complete: {}", e);
                }
            }
            IndexingEvent::Cancelled => {
                // No specific event for cancelled in frontend
                tracing::debug!("Indexing cancelled");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_creation() {}
}
