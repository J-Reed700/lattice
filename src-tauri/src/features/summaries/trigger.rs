//! Durable coalesced summary admission: one worker, one active generation,
//! and no task/body allocated per indexing notification.
use crate::application::ports::DocumentIndexObserver;
use crate::features::summaries::use_cases::GenerateDocumentSummariesUseCase;
use crate::shared::runtime::background::BackgroundTasks;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

struct Hook {
    use_case: Arc<GenerateDocumentSummariesUseCase>,
    pool: SqlitePool,
    wake: Notify,
    cancel: CancellationToken,
    operation: Mutex<()>,
    generation_cancel: RwLock<CancellationToken>,
}

/// Where indexing reports to the summary tier: the post-index hook the tier's
/// registration installs, or nothing while the tier is off. The container
/// owns one and hands it to every indexer as its [`DocumentIndexObserver`].
#[derive(Default)]
pub struct SummaryTrigger {
    hook: RwLock<Option<Arc<Hook>>>,
}

impl SummaryTrigger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_post_index_hook(
        &self,
        use_case: Arc<GenerateDocumentSummariesUseCase>,
        pool: SqlitePool,
        background: &BackgroundTasks,
    ) {
        self.clear_post_index_hook();
        let cancel = background.token().child_token();
        let hook = Arc::new(Hook {
            use_case,
            pool,
            wake: Notify::new(),
            cancel: cancel.clone(),
            operation: Mutex::new(()),
            generation_cancel: RwLock::new(cancel.child_token()),
        });
        if let Ok(mut slot) = self.hook.write() {
            *slot = Some(hook.clone());
        }
        background.spawn(run(hook));
    }

    pub fn clear_post_index_hook(&self) {
        if let Ok(mut slot) = self.hook.write() {
            if let Some(hook) = slot.take() {
                hook.cancel.cancel();
            }
        }
    }

    fn hook(&self) -> Option<Arc<Hook>> {
        self.hook.read().ok()?.clone()
    }

    pub async fn notify_document_indexed(&self, document_id: &str) {
        let Some(hook) = self
            .hook()
            .filter(|h| h.use_case.is_enabled() && !h.cancel.is_cancelled())
        else {
            return;
        };
        // Queue admission is on the indexing notification path, but summary
        // generation must never make the primary document index fail. Retry a
        // transient SQLite error briefly before giving up and logging the loss.
        for attempt in 0..4 {
            match super::repository::enqueue_work(&hook.pool, document_id).await {
                Ok(()) => {
                    hook.wake.notify_one();
                    return;
                }
                Err(error) if attempt < 3 => {
                    let delay = std::time::Duration::from_millis(100 * (1 << attempt));
                    tracing::warn!(%error, document_id, attempt = attempt + 1, ?delay, "Summary queue admission failed; retrying");
                    tokio::select! {
                        biased;
                        _ = hook.cancel.cancelled() => return,
                        _ = tokio::time::sleep(delay) => {},
                    }
                }
                Err(error) => {
                    tracing::error!(%error, document_id, "Could not persist summary request after retries");
                }
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.hook()
            .is_some_and(|h| h.use_case.is_enabled() && !h.cancel.is_cancelled())
    }

    pub async fn notify_document_at_path(&self, pool: &SqlitePool, path: &Path) {
        if !self.is_active() {
            return;
        }
        match super::repository::document_id_at_path(pool, path).await {
            Ok(Some(id)) => self.notify_document_indexed(&id).await,
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Could not resolve summary document"),
        }
    }

    /// Complete cleanup while summary ids still exist. The shared operation lock
    /// prevents an in-flight generation from publishing vectors after deletion.
    pub async fn notify_document_deleted(&self, document_id: &str) {
        let Some(hook) = self.hook() else {
            return;
        };
        // A delete must not wait through every section's model timeout. Interrupt
        // model work first; the unacknowledged job stays durable for later retry.
        if let Ok(cancel) = hook.generation_cancel.read() {
            cancel.cancel();
        }
        let _operation = hook.operation.lock().await;
        if let Err(error) = super::repository::remove_work(&hook.pool, document_id).await {
            tracing::warn!(%error, "Could not clear summary request");
        }
        if let Err(error) = hook.use_case.forget(document_id).await {
            tracing::warn!(%error, document_id, "Could not remove document summaries");
        }
    }
}

#[async_trait]
impl DocumentIndexObserver for SummaryTrigger {
    async fn document_indexed(&self, document_id: &str) {
        self.notify_document_indexed(document_id).await;
    }

    async fn document_deleted(&self, document_id: &str) {
        self.notify_document_deleted(document_id).await;
    }
}

async fn run(hook: Arc<Hook>) {
    loop {
        if hook.cancel.is_cancelled() {
            break;
        }
        let cleanup_result = {
            let _operation = tokio::select! {
                biased;
                _ = hook.cancel.cancelled() => break,
                guard = hook.operation.lock() => guard,
            };
            hook.use_case.drain_vector_cleanups().await
        };
        if let Err(error) = cleanup_result {
            tracing::warn!(%error, "Summary vector cleanup failed; tombstones retained for retry");
            tokio::select! {
                biased;
                _ = hook.cancel.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {},
            }
            continue;
        }
        match super::repository::next_work(&hook.pool).await {
            Ok(Some((id, revision))) => {
                let _operation = tokio::select! {
                    biased;
                    _ = hook.cancel.cancelled() => break,
                    guard = hook.operation.lock() => guard,
                };
                let generation_cancel = hook
                    .generation_cancel
                    .read()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                let result = hook
                    .use_case
                    .execute_cancellable(&id, &generation_cancel)
                    .await;
                *hook
                    .generation_cancel
                    .write()
                    .unwrap_or_else(|error| error.into_inner()) = hook.cancel.child_token();
                match result {
                    Ok(_) => {
                        if let Err(error) =
                            super::repository::acknowledge_work(&hook.pool, &id, &revision).await
                        {
                            tracing::warn!(%error, "Could not acknowledge summary work");
                        }
                        continue;
                    }
                    Err(error) => {
                        tracing::warn!(%error, document_id = id, "Summary failed; retained for retry");
                        let _ = super::repository::defer_work(&hook.pool, &id, &revision).await;
                    }
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Could not read summary queue"),
        }
        tokio::select! {
            biased;
            _ = hook.cancel.cancelled() => break,
            _ = hook.wake.notified() => {},
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {},
        }
    }
}
