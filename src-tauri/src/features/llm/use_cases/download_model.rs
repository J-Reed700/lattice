//! Download Model Use Case
//!
//! Downloads a model with validation and progress tracking.
//!
//! # Purpose
//!
//! Downloads an LLM model from the catalog to local storage,
//! with validation, disk space checks, and directory creation.
//!
//! # Dependencies
//! - `ModelCatalogPort` - Verify model exists
//! - `ModelStoragePort` - Check existing, manage files
//!
//! # Example
//! ```rust,no_run
//! let use_case = DownloadModelUseCase::new(
//!     catalog, storage, download_manager, file_system, credentials,
//!     checksum_service, file_system_access, uow_factory,
//! );
//! let request = DownloadModelRequestDto {
//!     model_id: "phi-3-mini".to_string(),
//! };
//! let response = use_case.execute(request).await?;
//! println!("Downloaded to: {}", response.path);
//! ```

use crate::features::download::manager::{DownloadBatchItem, DownloadManager, DownloadRequest};
use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::infrastructure::adapters::fs::model_paths::ModelPaths;
use lattice_core::application::ports::credentials_port::CredentialsPort;
use lattice_core::application::ports::file_system_port::FileSystemPort;
use lattice_core::application::ports::model_catalog::ModelCatalogPort;
use lattice_core::application::ports::model_storage::ModelStoragePort;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::domain::download::DownloadOperationState;
use lattice_core::domain::models::validation::ModelFileValidator;
use lattice_core::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use lattice_core::shared::error::AppError;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

mod metadata;
mod multi_file;
mod single_file;

pub struct DownloadModelUseCase {
    catalog: Arc<dyn ModelCatalogPort>,
    storage: Arc<dyn ModelStoragePort>,
    download_manager: Arc<dyn DownloadManager>,
    file_system: Arc<dyn FileSystemPort>,
    credentials: Arc<dyn CredentialsPort>,
    checksum_service: Arc<dyn ChecksumService>,
    file_system_access: Arc<dyn FileSystemAccess>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
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
        uow_factory: Arc<dyn UnitOfWorkFactory>,
    ) -> Self {
        Self {
            catalog,
            storage,
            download_manager,
            file_system,
            credentials,
            checksum_service,
            file_system_access,
            uow_factory,
        }
    }

    /// Verifies all required files exist on disk for a model
    /// Returns Ok(true) if all files present, Ok(false) if any missing, Err on filesystem errors
    async fn verify_model_files(
        &self,
        _model_id: &str,
        curated: &lattice_core::domain::models::selection::ModelMetadata,
        model_path: &Path,
    ) -> Result<bool, AppError> {
        use lattice_core::domain::models::validation::FileExpectation;

        let mut expectations = Vec::new();

        if !curated.files.is_empty() {
            for file in &curated.files {
                let mut expectation = FileExpectation::new(file.filename.clone());

                // `size_bytes == 0` means "size unknown" in the curated catalog
                // (see `build_safetensors_embedding_file_list` and the
                // transcription entries). Asserting an exact size of 0 would
                // fail verification for every such file and force a needless
                // re-download on every startup.
                if file.size_bytes > 0 {
                    expectation = expectation.with_size(file.size_bytes);
                }

                if let Some(checksum) = &file.checksum {
                    expectation = expectation.with_checksum(checksum.clone());
                }

                expectations.push(expectation);
            }
        } else if let Some(filename) = &curated.default_filename {
            expectations.push(FileExpectation::new(filename.clone()));
        } else {
            return Err(AppError::InvalidInput(
                "No files or default filename specified for model".into(),
            ));
        }

        let validator = ModelFileValidator::new(
            self.checksum_service.clone(),
            self.file_system_access.clone(),
        );
        validator
            .verify_expected_files(model_path, &expectations)
            .await
    }

    /// Reconcile database state with filesystem reality after verification
    async fn reconcile_download_records(
        &self,
        model_id: &str,
        _curated: &lattice_core::domain::models::selection::ModelMetadata,
        _model_path: &std::path::Path,
    ) -> Result<(), AppError> {
        if let Err(e) = self
            .download_manager
            .delete_pending_by_model(model_id)
            .await
        {
            warn!("Failed to cleanup stale downloads for {}: {}", model_id, e);
        }

        // Note: Model addition to database happens in download completion handler
        // This function just ensures cleanup of stale download records

        Ok(())
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
            use crate::features::embedding::compatibility::EmbeddingCompatibility;
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

        let paths = ModelPaths::new(model_id)?;
        let model_path = paths.unified_path();

        self.file_system.create_directory_all(model_path).await?;

        if self.storage.is_model_downloaded(model_id).await? {
            match self
                .verify_model_files(model_id, &model_metadata, model_path)
                .await
            {
                Ok(true) => {
                    // Fast path: All files verified present
                    let path = self.storage.get_model_path(model_id).await?;
                    info!("Model already downloaded and verified: {}", model_id);

                    // Reconcile database and cleanup stale records
                    if let Err(e) = self
                        .reconcile_download_records(model_id, &model_metadata, model_path)
                        .await
                    {
                        warn!(
                            "Failed to reconcile download records for {}: {}",
                            model_id, e
                        );
                    }

                    let verified_files = if !model_metadata.files.is_empty() {
                        model_metadata.files.len()
                    } else {
                        1
                    };

                    return Ok(DownloadModelResponseDto {
                        state: DownloadOperationState::AlreadyDownloaded {
                            verified_files,
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

        if !model_metadata.files.is_empty() {
            // Multi-file model (e.g., BGE-M3 with model.onnx + model.onnx_data)
            info!(
                model_id = %model_id,
                file_count = model_metadata.files.len(),
                "Downloading multi-file model"
            );
            self.download_multi_file_model(model_id, &model_metadata, model_path, &paths)
                .await
        } else {
            // Single-file model (legacy path)
            info!(model_id = %model_id, "Downloading single-file model");
            self.download_single_file_model(model_id, &model_metadata, model_path, &paths)
                .await
        }
    }
}
