use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Owns worker handles so shutdown can wait before closing their database.
pub(super) struct BackgroundWorkers(tokio::sync::Mutex<Vec<JoinHandle<()>>>);

impl BackgroundWorkers {
    pub(super) fn new(handles: Vec<JoinHandle<()>>) -> Self {
        Self(tokio::sync::Mutex::new(handles))
    }

    pub(super) async fn stop(&self, cancel: &CancellationToken) {
        cancel.cancel();
        let mut handles = self.0.lock().await;
        let completed = tokio::time::timeout(WORKER_SHUTDOWN_TIMEOUT, async {
            for handle in handles.iter_mut() {
                if let Err(error) = handle.await {
                    tracing::warn!(%error, "Background worker failed during shutdown");
                }
            }
        })
        .await;
        if completed.is_err() {
            tracing::warn!("Background worker shutdown timed out; aborting remaining workers");
            for handle in handles.iter() {
                handle.abort();
            }
            // Completed handles have already been polled. Only await those
            // still running; the runtime must drop them before DB closure.
            for handle in handles.iter_mut() {
                if !handle.is_finished() {
                    let _ = handle.await;
                }
            }
        }
        handles.clear();
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn shutdown_waits_for_cooperative_cleanup() {
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let cleaned = Arc::new(AtomicBool::new(false));
        let worker_cleaned = cleaned.clone();
        let worker = tokio::spawn(async move {
            worker_cancel.cancelled().await;
            tokio::task::yield_now().await;
            worker_cleaned.store(true, Ordering::SeqCst);
        });
        let workers = BackgroundWorkers::new(vec![worker]);
        workers.stop(&cancel).await;
        assert!(cleaned.load(Ordering::SeqCst));
        // Repeated exit requests must not poll completed handles again.
        workers.stop(&cancel).await;
    }

    #[tokio::test]
    async fn shutdown_aborts_unresponsive_workers_after_grace_period() {
        struct Cleanup(Arc<AtomicBool>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Cleanup(dropped.clone());
        let worker = tokio::spawn(async move {
            let _guard = guard;
            std::future::pending::<()>().await;
        });
        let workers = BackgroundWorkers::new(vec![tokio::spawn(async {}), worker]);
        workers.stop(&CancellationToken::new()).await;
        assert!(dropped.load(Ordering::SeqCst));
    }
}
