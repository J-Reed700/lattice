//! Track Download Use Case
//!
//! Records a model download completion in the models table.

use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::Result;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// Use case for tracking completed model downloads
pub struct TrackDownloadUseCase {
    repository: Arc<dyn DownloadedModelRepository>,
}

impl TrackDownloadUseCase {
    pub fn new(repository: Arc<dyn DownloadedModelRepository>) -> Self {
        Self { repository }
    }

    /// Execute the use case to track a download
    pub async fn execute(
        &self,
        model_name: String,
        model_id: String,
        file_path: PathBuf,
        file_size_bytes: i64,
        metadata: Option<JsonValue>,
    ) -> Result<String> {
        // Generate unique ID
        let id = uuid::Uuid::new_v4().to_string();

        // Create domain model
        // Extract architecture from model_id (e.g., "mistral-7b" -> "mistral")
        let architecture = model_id
            .split('-')
            .next()
            .unwrap_or("unknown")
            .to_lowercase();

        let model = DownloadedModel::new_with_catalog(
            id.clone(),
            model_name.clone(),
            model_id.clone(),
            ModelLocation::LocalFile {
                path: file_path.clone(),
            },
            file_size_bytes,
            architecture,
            metadata,
            |identifier| {
                lattice_core::domain::models::catalog_cache::ModelCatalogCache::instance()
                    .lookup(identifier)
            },
        )?;

        // Save to repository
        self.repository.save(&model).await?;

        info!(
            model_id = %model_id,
            file_path = %file_path.display(),
            "Model download tracked successfully"
        );

        Ok(id)
    }
}
