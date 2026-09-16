use crate::infrastructure::indexing::error::{IndexingError, Result};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum IndexTask {
    IndexFile { path: PathBuf },
    IndexFolder { path: PathBuf, recursive: bool },
    ReindexFile { path: PathBuf },
    RemoveFile { path: PathBuf },
    CancelAll,
    Shutdown,
}

pub struct IndexingQueue {
    tx: mpsc::Sender<IndexTask>,
    capacity: usize,
}

impl IndexingQueue {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<IndexTask>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx, capacity }, rx)
    }

    pub async fn enqueue(&self, task: IndexTask) -> Result<()> {
        self.tx
            .send(task)
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub fn try_enqueue(&self, task: IndexTask) -> Result<()> {
        self.tx.try_send(task).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => IndexingError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => {
                IndexingError::Other("Queue is closed".to_string())
            }
        })?;
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub async fn enqueue_with_priority(&self, task: IndexTask) -> Result<()> {
        match &task {
            IndexTask::CancelAll | IndexTask::Shutdown => {
                self.tx
                    .send(task)
                    .await
                    .map_err(|_| IndexingError::QueueFull)?;
                Ok(())
            }
            _ => self.enqueue(task).await,
        }
    }

    pub fn clone_sender(&self) -> mpsc::Sender<IndexTask> {
        self.tx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_creation() {
        let (queue, mut rx) = IndexingQueue::new(10);
        assert_eq!(queue.capacity(), 10);
        assert!(!queue.is_closed());
        drop(queue);
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_enqueue_and_dequeue() {
        let (queue, mut rx) = IndexingQueue::new(10);

        let task = IndexTask::IndexFile {
            path: PathBuf::from("/test/file.txt"),
        };

        queue.enqueue(task.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            IndexTask::IndexFile { path } => {
                assert_eq!(path, PathBuf::from("/test/file.txt"));
            }
            _ => panic!("Expected IndexTask::IndexFile, got a different variant"),
        }
    }

    #[tokio::test]
    async fn test_try_enqueue_full() {
        let (queue, _rx) = IndexingQueue::new(1);

        let task1 = IndexTask::IndexFile {
            path: PathBuf::from("/test/file1.txt"),
        };
        let task2 = IndexTask::IndexFile {
            path: PathBuf::from("/test/file2.txt"),
        };

        queue.try_enqueue(task1).unwrap();

        let result = queue.try_enqueue(task2);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let (queue, mut rx) = IndexingQueue::new(10);

        queue.enqueue(IndexTask::CancelAll).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert!(matches!(received, IndexTask::CancelAll));
    }

    #[tokio::test]
    async fn test_multiple_tasks() {
        let (queue, mut rx) = IndexingQueue::new(10);

        for i in 0..5 {
            let task = IndexTask::IndexFile {
                path: PathBuf::from(format!("/test/file{}.txt", i)),
            };
            queue.enqueue(task).await.unwrap();
        }

        for i in 0..5 {
            let received = rx.recv().await.unwrap();
            match received {
                IndexTask::IndexFile { path } => {
                    assert!(path.to_string_lossy().contains(&format!("file{}.txt", i)));
                }
                _ => panic!("Expected IndexTask::IndexFile, got a different variant"),
            }
        }
    }
}
