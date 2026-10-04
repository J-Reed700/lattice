//! Durable coalesced summary admission: one worker, one active generation,
//! and no task/body allocated per indexing notification.
use crate::features::summaries::use_cases::GenerateDocumentSummariesUseCase;
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
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

fn registry() -> &'static RwLock<Option<Arc<Hook>>> {
    static REGISTRY: OnceLock<RwLock<Option<Arc<Hook>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(None))
}

pub fn register_post_index_hook(use_case: Arc<GenerateDocumentSummariesUseCase>, pool: SqlitePool) {
    clear_post_index_hook();
    let cancel = crate::shared::runtime::background::cancellation_token().child_token();
    let hook = Arc::new(Hook {
        use_case,
        pool,
        wake: Notify::new(),
        cancel: cancel.clone(),
        operation: Mutex::new(()),
        generation_cancel: RwLock::new(cancel.child_token()),
    });
    if let Ok(mut slot) = registry().write() {
        *slot = Some(hook.clone());
    }
    crate::shared::runtime::background::spawn(run(hook));
}

pub fn clear_post_index_hook() {
    if let Ok(mut slot) = registry().write() {
        if let Some(hook) = slot.take() {
            hook.cancel.cancel();
        }
    }
}

fn hook() -> Option<Arc<Hook>> {
    registry().read().ok()?.clone()
}

pub async fn notify_document_indexed(document_id: &str) {
    let Some(hook) = hook().filter(|h| h.use_case.is_enabled() && !h.cancel.is_cancelled()) else {
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

pub fn is_active() -> bool {
    hook().is_some_and(|h| h.use_case.is_enabled() && !h.cancel.is_cancelled())
}

pub async fn notify_document_at_path(pool: &SqlitePool, path: &Path) {
    if !is_active() {
        return;
    }
    match super::repository::document_id_at_path(pool, path).await {
        Ok(Some(id)) => notify_document_indexed(&id).await,
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "Could not resolve summary document"),
    }
}

/// Complete cleanup while summary ids still exist. The shared operation lock
/// prevents an in-flight generation from publishing vectors after deletion.
pub async fn notify_document_deleted(document_id: &str) {
    let Some(hook) = hook() else {
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
