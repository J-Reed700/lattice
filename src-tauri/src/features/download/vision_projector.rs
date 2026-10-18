//! A downloaded catalog chat model's vision projector, fetched on request
//! into the model's own directory beside its weights.
//!
//! The projector is never part of the model's download: a reader who never
//! attaches a picture never fetches it. It is one pinned file, so the
//! download manager hashes it as it writes and deletes a file whose digest
//! does not match; a projector of the pinned size on disk is a verified one.

use serde::{Deserialize, Serialize};
use tracing::info;

use lattice_core::application::ports::download_manager::{DownloadManager, DownloadRequest};
use lattice_core::domain::download::{Checksum, ChecksumAlgorithm};
use lattice_core::domain::models::curated::chat_model_vision;
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::{AppError, Result};

/// What asking for a projector started.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VisionProjectorDownloadDto {
    /// The download to follow in the download list and its events; `None`
    /// when the whole projector is already on disk.
    pub download_id: Option<String>,
}

/// Start downloading the vision projector for the downloaded model `id` (the
/// downloaded model's own id, as `delete_model` takes).
pub async fn start_vision_projector_download_impl(
    manager: &dyn DownloadManager,
    repository: &dyn DownloadedModelRepository,
    id: &str,
) -> Result<VisionProjectorDownloadDto> {
    let model = repository
        .find_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No downloaded model {id}")))?;
    let weights = model.loadable_path().ok_or_else(|| {
        AppError::InvalidInput(format!("{} is not a local model", model.model_name()))
    })?;
    let vision = chat_model_vision(weights).ok_or_else(|| {
        AppError::InvalidInput(format!(
            "The catalog lists no vision projector for {}",
            model.model_name()
        ))
    })?;
    // repository-barrier-allow: the projector is a sibling file the model record does not list; its full size is what makes it usable.
    let on_disk = std::fs::metadata(&vision.projector_path).map_or(0, |file| file.len());
    if vision.is_complete(on_disk) {
        return Ok(VisionProjectorDownloadDto { download_id: None });
    }
    let checksum = vision
        .projector
        .checksum
        .clone()
        .map(|digest| Checksum::new(ChecksumAlgorithm::Sha256, digest))
        .transpose()
        .map_err(|e| AppError::InvalidState(format!("Invalid projector checksum: {e}")))?;
    let download_id = manager
        .start_download(DownloadRequest {
            url: vision.projector.url.clone(),
            destination: vision.projector_path.clone(),
            checksum,
            auth_token: None,
            model_name: Some(format!("{} vision projector", vision.model_name)),
            // Not a model of its own: no model record waits on this file.
            model_id: None,
            model_file_name: None,
        })
        .await
        .map_err(AppError::from)?;
    info!(
        id = %download_id,
        model = %model.model_id(),
        bytes = vision.projector.size_bytes,
        "Vision projector download started"
    );
    Ok(VisionProjectorDownloadDto {
        download_id: Some(download_id),
    })
}

#[cfg(test)]
mod tests;
