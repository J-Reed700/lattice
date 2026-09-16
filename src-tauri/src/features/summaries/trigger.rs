//! Post-index trigger.
//!
//! Indexing must not wait for a language model, so the hook a completed import
//! calls is a fire-and-forget notification: one synchronous function that, when
//! the tier is switched off (the default), does nothing but read an atomic.
//!
//! The registry is process-global because the call sites — `index_file` and the
//! indexing actor — construct no container and hold no summary dependencies;
//! giving them one would mean threading an optional service through every
//! import path to reach code that is disabled by default.

use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use sqlx::SqlitePool;

use crate::features::summaries::use_cases::GenerateDocumentSummariesUseCase;

type Registry = RwLock<Option<Arc<GenerateDocumentSummariesUseCase>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(None))
}

/// Install the post-index summary generator. Replaces any previous one, so a
/// settings change can re-register without restarting.
pub fn register_post_index_hook(use_case: Arc<GenerateDocumentSummariesUseCase>) {
    if let Ok(mut slot) = registry().write() {
        *slot = Some(use_case);
    }
}

/// Remove the hook. Subsequent notifications are no-ops.
pub fn clear_post_index_hook() {
    if let Ok(mut slot) = registry().write() {
        *slot = None;
    }
}

fn hook() -> Option<Arc<GenerateDocumentSummariesUseCase>> {
    registry().read().ok()?.clone()
}

/// Tell the summary tier a document finished indexing.
///
/// Returns immediately. Nothing about the caller's result depends on what
/// happens next, including whether it happens at all.
pub fn notify_document_indexed(document_id: &str) {
    let Some(use_case) = hook().filter(|use_case| use_case.is_enabled()) else {
        return;
    };
    // No runtime means a synchronous test harness, not a missed summary worth
    // panicking over.
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let document_id = document_id.to_owned();
    handle.spawn(async move {
        if let Err(error) = use_case.execute(&document_id).await {
            tracing::warn!(%error, document_id, "Document summary generation failed");
        }
    });
}

/// Is the tier switched on? Call sites use this to avoid paying for the lookup
/// a notification would need.
pub fn is_active() -> bool {
    hook().is_some_and(|use_case| use_case.is_enabled())
}

/// Notify for a document identified by its file path.
///
/// The id lookup only happens when the tier is on, so an import with summaries
/// disabled — the default — costs one atomic read and no query.
pub async fn notify_document_at_path(pool: &SqlitePool, path: &Path) {
    if !is_active() {
        return;
    }
    match sqlx::query_scalar::<_, String>("SELECT id FROM documents WHERE file_path = ?")
        .bind(path.to_string_lossy().as_ref())
        .fetch_optional(pool)
        .await
    {
        Ok(Some(document_id)) => notify_document_indexed(&document_id),
        Ok(None) => tracing::debug!(path = %path.display(), "No document row to summarize"),
        Err(error) => tracing::warn!(%error, "Could not resolve a document id to summarize"),
    }
}

/// Tell the summary tier a document is about to be deleted.
///
/// Fire-and-forget like [`notify_document_indexed`], and a no-op while the
/// tier is off. Call it *before* the document row goes: the cascade takes the
/// summary rows with it, and their ids are what names the vectors to drop.
pub fn notify_document_deleted(document_id: &str) {
    let Some(use_case) = hook().filter(|use_case| use_case.is_enabled()) else {
        return;
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let document_id = document_id.to_owned();
    handle.spawn(async move {
        if let Err(error) = use_case.forget(&document_id).await {
            tracing::warn!(%error, document_id, "Could not drop a document's summaries");
        }
    });
}
