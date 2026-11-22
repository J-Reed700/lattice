//! Whether the cross-encoder reranker can run, and how to fetch it.
//!
//! Reranking is always on: once the model is on disk, every chat search
//! reorders its shortlist with it, and until then search keeps its
//! first-stage order. The model is a one-time download Settings offers, and
//! the status says which of the two the user is in.

use lattice_core::application::ports::download_manager::{DownloadManager, DownloadRequest};
use lattice_core::domain::download::part_path;
use lattice_core::domain::models::curated::CURATED_RERANKER;
use lattice_core::shared::error::{AppError, Result};
use std::path::{Path, PathBuf};

/// What the reranker download costs, so the UI can name a size before
/// starting rather than opening an indeterminate progress bar.
pub const RERANKER_DOWNLOAD_BYTES: u64 = CURATED_RERANKER.bytes;

/// The reranker Lattice downloads. ms-marco-MiniLM-L-6-v2 moved nothing on the
/// v3 fixture; this one lifted nDCG@5 (see
/// `evals/retrieval/2026-10-12-RERANKER-FTS-HEADER-FUSION.md`).
pub const RERANKER_MODEL_NAME: &str = CURATED_RERANKER.name;

/// The one place the reranker's GGUF path is decided. Search loads it from
/// here and the status and download commands read and write it, so they
/// cannot drift apart.
pub fn reranker_gguf_path(models_path: &Path) -> PathBuf {
    CURATED_RERANKER.path_in(models_path)
}

/// Whether the reranker is on disk, and so whether search reranks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RerankerStatusDto {
    /// The reranker's model is on disk, so search reranks with it.
    pub installed: bool,
    pub model_name: String,
    pub download_bytes: u64,
    /// Where the model lives, as `to_string_lossy()` of the same path search
    /// loads. It equals the `destination` of the download that fetches it, so
    /// the UI can find that download in the list.
    pub path: String,
}

impl RerankerStatusDto {
    fn new(installed: bool, path: &Path) -> Self {
        Self {
            installed,
            model_name: RERANKER_MODEL_NAME.to_string(),
            download_bytes: RERANKER_DOWNLOAD_BYTES,
            path: path.to_string_lossy().into_owned(),
        }
    }
}

/// The reranker is installed when its file has exactly the pinned size. The
/// download manager renames a verified file into place only when complete,
/// so a file of that size is a finished one.
fn is_installed(gguf: &Path) -> bool {
    std::fs::metadata(gguf).is_ok_and(|file| file.len() == CURATED_RERANKER.bytes)
}

/// Report whether the reranker model is present.
pub fn reranker_status_impl(models_path: &Path) -> RerankerStatusDto {
    let gguf = reranker_gguf_path(models_path);
    RerankerStatusDto::new(is_installed(&gguf), &gguf)
}

/// Start fetching the reranker model through the shared download manager if
/// it is missing, then report the state. It returns as soon as the download
/// is queued; the manager coalesces a repeat start for the same file.
pub async fn download_reranker_impl(
    manager: &dyn DownloadManager,
    models_path: &Path,
) -> Result<RerankerStatusDto> {
    let gguf = reranker_gguf_path(models_path);
    if is_installed(&gguf) {
        return Ok(RerankerStatusDto::new(true, &gguf));
    }

    // The old downloader's partial file is never resumed; drop it.
    match std::fs::remove_file(gguf.with_extension("tmp")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "Could not remove the old reranker partial file"),
    }

    let request = DownloadRequest::pinned(
        CURATED_RERANKER.url(),
        gguf.clone(),
        CURATED_RERANKER.sha256,
        CURATED_RERANKER.bytes,
        CURATED_RERANKER.name,
    )
    .map_err(AppError::from)?;
    let id = manager
        .start_download(request)
        .await
        .map_err(AppError::from)?;
    tracing::info!(%id, "Reranker download started");
    Ok(RerankerStatusDto::new(false, &gguf))
}

/// Delete the reranker's model: the GGUF, any partial download beside it, the
/// old downloader's temp file, and the Downloads history of finished
/// downloads for it. Search keeps its first-stage order until the model is
/// downloaded again. Refuses while a download of it is running, since that
/// download would write the file again; cancel it first.
pub async fn remove_reranker_impl(
    manager: &dyn DownloadManager,
    models_path: &Path,
) -> Result<RerankerStatusDto> {
    let gguf = reranker_gguf_path(models_path);
    let sessions: Vec<_> = manager
        .list_downloads()
        .await
        .map_err(AppError::from)?
        .into_iter()
        .filter(|session| session.destination() == &gguf)
        .collect();
    if sessions
        .iter()
        .any(|session| !session.state().is_terminal())
    {
        return Err(AppError::InvalidState(
            "The reranker is still downloading. Cancel the download in Downloads, then remove it."
                .into(),
        ));
    }

    // Finished downloads of a file that is going away would only mislead.
    for session in &sessions {
        manager
            .delete_download(session.id())
            .await
            .map_err(AppError::from)?;
    }

    for leftover in [part_path(&gguf), gguf.with_extension("tmp")] {
        match std::fs::remove_file(&leftover) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                %error,
                path = %leftover.display(),
                "Could not remove a reranker leftover file"
            ),
        }
    }

    // The GGUF goes last: while it exists `download_reranker_impl` refuses to
    // start a download, so no new `.part` can appear while the leftovers go.
    match std::fs::remove_file(&gguf) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::error!(%error, path = %gguf.display(), "Could not remove the reranker model");
            return Err(AppError::FileSystem(format!(
                "Could not remove the reranker model at {}: {error}",
                gguf.display()
            )));
        }
    }
    tracing::info!("Reranker model removed");
    Ok(reranker_status_impl(models_path))
}

#[cfg(test)]
mod tests;
