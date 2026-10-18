//! Application-owned background work. Closing admission and joining happen
//! before the database closes; critical writes are never detached by aborting
//! their parent future during shutdown.
use std::future::Future;
use std::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

/// The scope background work runs in. App setup builds one and hands it to
/// every service and command that spawns work; a test builds its own.
#[derive(Default)]
pub struct BackgroundTasks {
    admission: Mutex<bool>,
    tracker: TaskTracker,
    cancel: CancellationToken,
}

impl BackgroundTasks {
    pub fn token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn spawn<F>(&self, future: F) -> Option<JoinHandle<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let closed = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if *closed {
            return None;
        }
        Some(self.tracker.spawn(future))
    }

    pub fn close(&self) {
        *self.admission.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.tracker.close();
        self.cancel.cancel();
    }

    pub async fn wait(&self) {
        self.tracker.wait().await;
    }
}

impl Drop for BackgroundTasks {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closing_rejects_new_work_and_waits_for_cleanup() {
        let tasks = BackgroundTasks::default();
        let cancel = tasks.token();
        let (release, released) = tokio::sync::oneshot::channel();
        let worker = tasks.spawn(async move {
            cancel.cancelled().await;
            let _ = released.await;
        });
        assert!(worker.is_some());
        tasks.close();
        assert!(tasks.spawn(async {}).is_none());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), tasks.wait())
                .await
                .is_err()
        );
        assert!(release.send(()).is_ok());
        tasks.wait().await;
        tasks.close();
        tasks.wait().await;
    }
}
