//! Whether the cross-encoder reranker can run, and how to fetch it.
//!
//! Reranking is always on: once the model is on disk, every chat search
//! reorders its shortlist with it, and until then search keeps its
//! first-stage order. The model is a one-time download Settings offers, and
//! the status says which of the two the user is in.

use crate::infrastructure::services::traits::ModelManagerTrait;
use lattice_core::domain::models::curated::CURATED_RERANKER;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// What reranker setup needs: the model manager that can fetch the
/// cross-encoder. Kept as a small named bundle so the command layer hands
/// over exactly this and the setup code never sees the whole container.
#[derive(Clone)]
pub struct RerankerSetup {
    pub model_manager: Arc<dyn ModelManagerTrait>,
}

/// What the reranker download costs, so the UI can name a size before
/// starting rather than opening an indeterminate progress bar.
pub const RERANKER_DOWNLOAD_BYTES: u64 = CURATED_RERANKER.bytes;

/// The reranker Lattice downloads. ms-marco-MiniLM-L-6-v2 moved nothing on the
/// v3 fixture; this one lifted nDCG@5 (see
/// `evals/retrieval/2026-10-12-RERANKER-FTS-HEADER-FUSION.md`).
pub const RERANKER_MODEL_NAME: &str = CURATED_RERANKER.name;

/// Whether the reranker is on disk, and so whether search reranks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RerankerStatusDto {
    /// The reranker's model is on disk, so search reranks with it.
    pub installed: bool,
    pub model_name: String,
    pub download_bytes: u64,
}

impl RerankerStatusDto {
    fn new(installed: bool) -> Self {
        Self {
            installed,
            model_name: RERANKER_MODEL_NAME.to_string(),
            download_bytes: RERANKER_DOWNLOAD_BYTES,
        }
    }
}

/// Report whether the reranker model is present.
pub async fn reranker_status_impl(setup: &RerankerSetup) -> Result<RerankerStatusDto> {
    Ok(RerankerStatusDto::new(
        setup.model_manager.is_reranker_ready().await,
    ))
}

/// Fetch the reranker model if it is missing, then report the new state.
pub async fn download_reranker_impl(setup: &RerankerSetup) -> Result<RerankerStatusDto> {
    let manager = &setup.model_manager;
    if manager.is_reranker_ready().await {
        return Ok(RerankerStatusDto::new(true));
    }

    manager.ensure_reranker_available().await.map_err(|error| {
        AppError::Other(format!("Could not download the reranker model: {error}"))
    })?;

    // Ask the filesystem rather than trusting the download's own word: a
    // partial write would report a reranker search cannot use.
    Ok(RerankerStatusDto::new(manager.is_reranker_ready().await))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_names_the_model_and_what_it_costs() {
        let status = RerankerStatusDto::new(false);
        assert!(!status.installed);
        assert_eq!(status.model_name, "Qwen/Qwen3-Reranker-0.6B");
        assert_eq!(
            status.download_bytes, 639_153_184,
            "a size the UI can show before starting, not a placeholder"
        );
    }
}
