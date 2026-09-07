//! Track Download Use Case
//!
//! Records a model download completion in the models table.

use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use tracing::info;

/// Use case for tracking completed model downloads
pub struct TrackDownloadUseCase {
    repository: DownloadedModelRepository,
}

impl TrackDownloadUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case to track a download
    ///
    /// # Arguments
    ///
    /// * `model_name` - Human-readable name
    /// * `model_id` - Unique model identifier
    /// * `file_path` - Path to downloaded file
    /// * `file_size_bytes` - Size of file in bytes
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Returns
    ///
    /// The ID of the created record
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
                crate::features::model_management::catalog_cache::ModelCatalogCache::instance()
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
