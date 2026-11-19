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

    /// Spawns `future` onto `handle` so callers on synchronous OS threads
    /// (such as Tauri setup, window, or native dialog callbacks) can register
    /// tracked background work without requiring an active ambient Tokio reactor.
    pub fn spawn_on<F>(
        &self,
        handle: &tokio::runtime::Handle,
        future: F,
    ) -> Option<JoinHandle<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let closed = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if *closed {
            return None;
        }
        Some(self.tracker.spawn_on(future, handle))
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

    #[test]
    fn spawn_on_works_from_a_thread_without_an_active_tokio_reactor() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let tasks = BackgroundTasks::default();
        let handle = runtime.handle().clone();

        // Main test thread is NOT entered into `runtime`.
        let join = tasks
            .spawn_on(&handle, async { 42u32 })
            .expect("admission is open");
        assert_eq!(runtime.block_on(join).unwrap(), 42);
        tasks.close();
        runtime.block_on(tasks.wait());
        assert!(tasks.spawn_on(&handle, async {}).is_none());
    }
}
