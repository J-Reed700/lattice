use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IndexingEvent {
    Started {
        total_files: usize,
    },
    FileStarted {
        path: String,
        current: usize,
        total: usize,
    },
    FileCompleted {
        path: String,
        chunks: usize,
        duration_ms: u64,
        current: usize,
        total: usize,
    },
    FileError {
        path: String,
        error: String,
        current: usize,
        total: usize,
    },
    Completed {
        total_files: usize,
        total_chunks: usize,
        duration_ms: u64,
    },
    Cancelled,
}

impl IndexingEvent {
    pub fn started(total_files: usize) -> Self {
        Self::Started { total_files }
    }

    pub fn file_started(path: String, current: usize, total: usize) -> Self {
        Self::FileStarted {
            path,
            current,
            total,
        }
    }

    pub fn file_completed(
        path: String,
        chunks: usize,
        duration_ms: u64,
        current: usize,
        total: usize,
    ) -> Self {
        Self::FileCompleted {
            path,
            chunks,
            duration_ms,
            current,
            total,
        }
    }

    pub fn file_error(path: String, error: String, current: usize, total: usize) -> Self {
        Self::FileError {
            path,
            error,
            current,
            total,
        }
    }

    pub fn completed(total_files: usize, total_chunks: usize, duration_ms: u64) -> Self {
        Self::Completed {
            total_files,
            total_chunks,
            duration_ms,
        }
    }

    pub fn cancelled() -> Self {
        Self::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = IndexingEvent::started(10);
        match event {
            IndexingEvent::Started { total_files } => assert_eq!(total_files, 10),
            _ => panic!("Expected IndexingEvent::Started, got a different variant"),
        }
    }

    #[test]
    fn test_event_serialization() {
        let event = IndexingEvent::file_completed("test.txt".to_string(), 5, 1000, 1, 10);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("FileCompleted"));
        assert!(json.contains("test.txt"));
    }
}
