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

use crate::interfaces::di::container::Container;
use crate::shared::error::{AppError, Result};

/// What the reranker download costs, taken from the published artifact.
///
/// Reported so the UI can name a size before starting rather than opening an
/// indeterminate progress bar. The tokenizer and config add well under a
/// megabyte between them.
pub const RERANKER_DOWNLOAD_BYTES: u64 = 90_870_598;

/// The cross-encoder Lattice reranks with.
pub const RERANKER_MODEL_NAME: &str = "cross-encoder/ms-marco-MiniLM-L-6-v2";

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
pub async fn reranker_status_impl(container: &Container) -> Result<RerankerStatusDto> {
    let installed = container.model_manager().is_reranker_ready().await;
    let enabled = reranking_enabled(container).await?;
    Ok(RerankerStatusDto::new(installed, enabled))
}

/// Fetch the reranker artifacts if they are missing, then report the new state.
///
/// Downloading does not switch reranking on. Acquiring the model and choosing
/// to use it are separate decisions, and conflating them would turn a download
/// button into a settings change the user did not ask for.
pub async fn download_reranker_impl(container: &Container) -> Result<RerankerStatusDto> {
    let manager = container.model_manager();
    if manager.is_reranker_ready().await {
        let enabled = reranking_enabled(container).await?;
        return Ok(RerankerStatusDto::new(true, enabled));
    }

    manager.ensure_reranker_available().await.map_err(|error| {
        AppError::Other(format!("Could not download the reranker model: {error}"))
    })?;

    // Ask the filesystem rather than trusting the download's own word: a
    // partial write leaves the toggle claiming a capability that still is not
    // there, which is the failure this module exists to prevent.
    let installed = manager.is_reranker_ready().await;
    let enabled = reranking_enabled(container).await?;
    Ok(RerankerStatusDto::new(installed, enabled))
}

async fn reranking_enabled(container: &Container) -> Result<bool> {
    let settings = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(|error| AppError::Other(format!("Could not read settings: {error}")))?;
    Ok(settings.search.enable_reranking)
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
        assert_eq!(status.model_name, "cross-encoder/ms-marco-MiniLM-L-6-v2");
        assert!(
            status.download_bytes > 80_000_000 && status.download_bytes < 100_000_000,
            "a size the UI can show before starting, not a placeholder"
        );
    }
}
