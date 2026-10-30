//! Whether the cross-encoder reranker can actually run, and how to make it.
//!
//! Reranking has a settings toggle, a wired provider, tuning knobs and tests.
//! What it did not have was a way to obtain the model. `ensure_reranker_available`
//! existed on the model manager and was covered by tests, but nothing in the
//! running app ever called it, and no model role offered to download one. So the
//! artifacts were never on disk, `LazyReranker::is_available` was always false,
//! and search quietly returned its unreranked shortlist — while Settings showed
//! a switch implying otherwise.
//!
//! Silently doing nothing is the part worth fixing. A toggle that cannot take
//! effect should say so, which means the frontend needs to see the same two
//! facts the retrieval path does: is it switched on, and is it installed.

use crate::infrastructure::services::traits::ModelManagerTrait;
use async_trait::async_trait;
use lattice_core::domain::models::curated::CURATED_RERANKER;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// The one settings value reranker setup reads: the user's reranking
/// toggle. Defined here so search never names the settings feature; the
/// composition root implements it over the settings use case.
#[async_trait]
pub trait RerankingToggle: Send + Sync {
    /// Whether the user has switched reranking on.
    async fn enabled(&self) -> Result<bool>;
}

/// The two things reranker setup needs: the model manager that can fetch the
/// cross-encoder, and the settings toggle that says whether to use it. Kept as
/// a small named bundle so the command layer hands over exactly this and the
/// setup code never sees the whole container.
#[derive(Clone)]
pub struct RerankerSetup {
    pub model_manager: Arc<dyn ModelManagerTrait>,
    pub toggle: Arc<dyn RerankingToggle>,
}

/// What the reranker download costs, so the UI can name a size before
/// starting rather than opening an indeterminate progress bar.
pub const RERANKER_DOWNLOAD_BYTES: u64 = CURATED_RERANKER.bytes;

/// The reranker Lattice downloads. ms-marco-MiniLM-L-6-v2 moved nothing on the
/// v3 fixture; this one lifted nDCG@5 (see
/// `evals/retrieval/2026-10-12-RERANKER-FTS-HEADER-FUSION.md`).
pub const RERANKER_MODEL_NAME: &str = CURATED_RERANKER.name;

/// Whether reranking is switched on, and whether it could run if it were.
///
/// Both are needed to describe the state honestly: on-but-missing and
/// off-but-installed are different situations with different next steps.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RerankerStatusDto {
    /// Every artifact the reranker needs is on disk.
    pub installed: bool,
    /// The user's `search.enableReranking` setting.
    pub enabled: bool,
    /// True only when reranking will actually happen. This is the field a UI
    /// should believe; the other two explain why.
    pub active: bool,
    pub model_name: String,
    pub download_bytes: u64,
}

impl RerankerStatusDto {
    fn new(installed: bool, enabled: bool) -> Self {
        Self {
            installed,
            enabled,
            active: installed && enabled,
            model_name: RERANKER_MODEL_NAME.to_string(),
            download_bytes: RERANKER_DOWNLOAD_BYTES,
        }
    }
}

/// Report whether reranking is on and whether the model is present.
pub async fn reranker_status_impl(setup: &RerankerSetup) -> Result<RerankerStatusDto> {
    let installed = setup.model_manager.is_reranker_ready().await;
    let enabled = reranking_enabled(setup).await?;
    Ok(RerankerStatusDto::new(installed, enabled))
}

/// Fetch the reranker artifacts if they are missing, then report the new state.
///
/// Downloading does not switch reranking on. Acquiring the model and choosing
/// to use it are separate decisions, and conflating them would turn a download
/// button into a settings change the user did not ask for.
pub async fn download_reranker_impl(setup: &RerankerSetup) -> Result<RerankerStatusDto> {
    let manager = &setup.model_manager;
    if manager.is_reranker_ready().await {
        let enabled = reranking_enabled(setup).await?;
        return Ok(RerankerStatusDto::new(true, enabled));
    }

    manager.ensure_reranker_available().await.map_err(|error| {
        AppError::Other(format!("Could not download the reranker model: {error}"))
    })?;

    // Ask the filesystem rather than trusting the download's own word: a
    // partial write leaves the toggle claiming a capability that still is not
    // there, which is the failure this module exists to prevent.
    let installed = manager.is_reranker_ready().await;
    let enabled = reranking_enabled(setup).await?;
    Ok(RerankerStatusDto::new(installed, enabled))
}

async fn reranking_enabled(setup: &RerankerSetup) -> Result<bool> {
    setup
        .toggle
        .enabled()
        .await
        .map_err(|error| AppError::Other(format!("Could not read settings: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reranking_is_active_only_when_installed_and_switched_on() {
        assert!(RerankerStatusDto::new(true, true).active);
    }

    #[test]
    fn a_switched_on_reranker_with_no_model_is_not_active() {
        let status = RerankerStatusDto::new(false, true);
        assert!(!status.active, "nothing can rerank without the artifacts");
        assert!(
            status.enabled,
            "the user's setting is reported as they left it, so the UI can \
             explain why the switch is not taking effect"
        );
    }

    #[test]
    fn an_installed_reranker_that_is_switched_off_is_not_active() {
        let status = RerankerStatusDto::new(true, false);
        assert!(!status.active);
        assert!(status.installed);
    }

    #[test]
    fn the_status_names_the_model_and_what_it_costs() {
        let status = RerankerStatusDto::new(false, false);
        assert_eq!(status.model_name, "Qwen/Qwen3-Reranker-0.6B");
        assert_eq!(
            status.download_bytes, 639_153_184,
            "a size the UI can show before starting, not a placeholder"
        );
    }
}
