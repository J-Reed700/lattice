//! Application-owned background work. Closing admission and joining happen
//! before the database closes; critical writes are never detached by aborting
//! their parent future during shutdown.
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio::task::JoinHandle;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Default)]
pub struct BackgroundTasks {
    admission: Mutex<bool>,
    tracker: TaskTracker,
    cancel: CancellationToken,
}

fn registry() -> &'static Mutex<Weak<BackgroundTasks>> {
    static REGISTRY: OnceLock<Mutex<Weak<BackgroundTasks>>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

impl BackgroundTasks {
    pub fn install() -> Arc<Self> {
        let tasks = Arc::new(Self::default());
        *registry().lock().unwrap_or_else(|e| e.into_inner()) = Arc::downgrade(&tasks);
        tasks
    }

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

fn current() -> Option<Arc<BackgroundTasks>> {
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .upgrade()
}

pub fn cancellation_token() -> CancellationToken {
    current().map(|tasks| tasks.token()).unwrap_or_default()
}

/// Standalone service tests can run without installing an application scope.
pub fn spawn<F>(future: F) -> Option<JoinHandle<F::Output>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match current() {
        Some(tasks) => tasks.spawn(future),
        None => Some(tokio::spawn(future)),
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
