//! Downloads a model with validation and progress tracking.

use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use crate::infrastructure::persistence::repositories::model::ops as model_ops;
use crate::infrastructure::persistence::repositories::model_file::ops as model_file_ops;
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::application::ports::download_manager::{
    DownloadBatchItem, DownloadManager, DownloadRequest,
};
use lattice_core::application::ports::file_system_port::FileSystemPort;
use lattice_core::application::ports::model_catalog::ModelCatalogPort;
use lattice_core::application::ports::model_storage::ModelStoragePort;
use lattice_core::domain::download::DownloadOperationState;
use lattice_core::domain::entities::model::Model;
use lattice_core::domain::entities::model_file::ModelFile;
use lattice_core::domain::models::validation::ModelFileValidator;
use lattice_core::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use lattice_core::domain::value_objects::model_status::FileStatus;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;
use plan::{planned_files, PlannedFile};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

mod metadata;
mod plan;
mod start;

#[cfg(test)]
mod write_tests;

pub struct DownloadModelUseCase {
    catalog: Arc<dyn ModelCatalogPort>,
    storage: Arc<dyn ModelStoragePort>,
    download_manager: Arc<dyn DownloadManager>,
    file_system: Arc<dyn FileSystemPort>,
    credentials: Arc<dyn CredentialsPort>,
    checksum_service: Arc<dyn ChecksumService>,
    file_system_access: Arc<dyn FileSystemAccess>,
    database: Database,
}

impl DownloadModelUseCase {
    pub fn new(
        catalog: Arc<dyn ModelCatalogPort>,
        storage: Arc<dyn ModelStoragePort>,
        download_manager: Arc<dyn DownloadManager>,
        file_system: Arc<dyn FileSystemPort>,
        credentials: Arc<dyn CredentialsPort>,
        checksum_service: Arc<dyn ChecksumService>,
        file_system_access: Arc<dyn FileSystemAccess>,
        database: Database,
    ) -> Self {
        Self {
            catalog,
            storage,
            download_manager,
            file_system,
            credentials,
            checksum_service,
            file_system_access,
            database,
        }
    }

    /// Writes the pending model row and its file rows in one unit of work.
    async fn record_pending(&self, model: &Model, files: &[ModelFile]) -> Result<(), AppError> {
        let mut tx = self.database.begin_write().await.map_err(|e| {
            AppError::Database(format!("Failed to begin model record transaction: {}", e))
        })?;
        model_ops::upsert_model(&mut tx, model)
            .await
            .map_err(|e| AppError::Database(format!("Failed to create model record: {}", e)))?;
        for file in files {
            model_file_ops::create(&mut tx, file).await.map_err(|e| {
                AppError::Database(format!("Failed to create model_file record: {}", e))
            })?;
        }
        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit model record transaction: {}", e))
        })
    }

    /// Marks the model and the given files (name, size) failed in one unit of work.
    async fn record_failed(
        &self,
        model_id: &str,
        files: &[(&str, Option<i64>)],
    ) -> Result<(), AppError> {
        let mut tx = self.database.begin_write().await.map_err(|e| {
            AppError::Database(format!("Failed to begin failure transaction: {}", e))
        })?;
        model_ops::update_status_failed(&mut tx, model_id).await?;
        for (file_name, size_bytes) in files {
            model_file_ops::update_file_status_by_model_and_name(
                &mut tx,
                model_id,
                file_name,
                FileStatus::Failed,
                *size_bytes,
            )
            .await?;
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to mark download failed: {}", e)))
    }

    /// Verifies every planned file exists on disk.
    /// Returns Ok(true) if all files present, Ok(false) if any missing, Err on filesystem errors
    async fn verify_model_files(
        &self,
        plan: &[PlannedFile],
        model_path: &Path,
    ) -> Result<bool, AppError> {
        use lattice_core::domain::models::validation::FileExpectation;

        let expectations: Vec<FileExpectation> = plan
            .iter()
            .map(|file| {
                let mut expectation = FileExpectation::new(file.filename.clone());
                if let Some(size) = file.expected_size {
                    expectation = expectation.with_size(size);
                }
                if let Some(checksum) = &file.checksum {
                    expectation = expectation.with_checksum(checksum.clone());
                }
                expectation
            })
            .collect();

        let validator = ModelFileValidator::new(
            self.checksum_service.clone(),
            self.file_system_access.clone(),
        );
        validator
            .verify_expected_files(model_path, &expectations)
            .await
    }

    pub async fn execute(
        &self,
        request: DownloadModelRequestDto,
    ) -> Result<DownloadModelResponseDto, AppError> {
        let model_id = &request.model_id;

        if model_id.is_empty() {
            return Err(AppError::InvalidInput("Model ID cannot be empty".into()));
        }

        info!("Starting model download: {}", model_id);

        let model_metadata = self.resolve_model_metadata(model_id).await?;

        // STEP 2.5: Refuse incompatible embedding models server-side. The
        // frontend should also disable the download button, but defense-
        // in-depth — never trust a UI-only gate for a long-running, large-
        // file operation that will fail at model-load time anyway.
        if model_metadata.category
            == lattice_core::domain::models::selection::ModelCategory::Embedding
        {
            use lattice_core::domain::models::selection::EmbeddingCompatibility;
            match &model_metadata.embedding_compatibility {
                Some(EmbeddingCompatibility::Incompatible {
                    architecture,
                    reason,
                }) => {
                    return Err(AppError::InvalidInput(format!(
                        "Embedding model '{}' uses architecture '{}' which the local \
                         runtime cannot load yet: {}. Pick a compatible model from the \
                         catalog instead.",
                        model_metadata.name, architecture, reason
                    )));
                }
                Some(EmbeddingCompatibility::Unknown) => {
                    return Err(AppError::InvalidInput(format!(
                        "Embedding model '{}' has an unrecognized architecture. \
                         The local embedding runtime cannot safely load it.",
                        model_metadata.name
                    )));
                }
                Some(EmbeddingCompatibility::Compatible { .. }) | None => {
                    // None happens for curated models (compatibility wasn't
                    // populated on the curated path); allow them through.
                }
            }
        }

        let plan = planned_files(model_id, &model_metadata)?;

        let paths = ModelPaths::new(model_id)?;
        let model_path = paths.unified_path();

        self.file_system.create_directory_all(model_path).await?;

        if self.storage.is_model_downloaded(model_id).await? {
            match self.verify_model_files(&plan, model_path).await {
                Ok(true) => {
                    // Fast path: All files verified present
                    let path = self.storage.get_model_path(model_id).await?;
                    info!("Model already downloaded and verified: {}", model_id);

                    // Reconcile database and cleanup stale records
                    if let Err(e) = self
                        .download_manager
                        .delete_pending_by_model(model_id)
                        .await
                    {
                        warn!("Failed to cleanup stale downloads for {}: {}", model_id, e);
                    }

                    return Ok(DownloadModelResponseDto {
                        state: DownloadOperationState::AlreadyDownloaded {
                            verified_files: plan.len(),
                            total_size_bytes: model_metadata.total_size_bytes,
                        },
                        path: Some(path.to_string_lossy().to_string()),
                    });
                }
                Ok(false) => {
                    // DB says downloaded but files missing
                    warn!(
                        model_id = %model_id,
                        "Database marked model as downloaded but files are missing on disk. Re-downloading."
                    );
                    // Fall through to download
                }
                Err(e) => {
                    // Filesystem error during verification
                    warn!(
                        model_id = %model_id,
                        error = %e,
                        "Error verifying model files. Proceeding with download."
                    );
                    // Fall through to download
                }
            }
        }

        self.download_planned(model_id, &model_metadata, model_path, &paths, &plan)
            .await
    }
}
