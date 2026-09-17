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
//! let use_case = DownloadModelUseCase::new(catalog_port, storage_port);
//! let request = DownloadModelRequestDto {
//!     model_id: "phi-3-mini".to_string(),
//! };
//! let response = use_case.execute(request).await?;
//! println!("Downloaded to: {}", response.path);
//! ```

use crate::application::ports::credentials_port::CredentialsPort;
use crate::application::ports::file_system_port::FileSystemPort;
use crate::application::ports::model_catalog::{ExternalModelMetadata, ModelCatalogPort};
use crate::application::ports::model_storage::ModelStoragePort;
use crate::application::ports::UnitOfWorkFactory;
use crate::domain::curated_models::get_all_curated_models;
use crate::domain::download::DownloadOperationState;
use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::model_file_validator::ModelFileValidator;
use crate::domain::model_paths::ModelPaths;
use crate::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::features::download::manager::{DownloadBatchItem, DownloadManager, DownloadRequest};
use crate::features::llm::dto::{DownloadModelRequestDto, DownloadModelResponseDto};
use crate::shared::error::AppError;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

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
    #[allow(clippy::too_many_arguments)]
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
        curated: &crate::features::model_management::domain::ModelMetadata,
        model_path: &Path,
    ) -> Result<bool, AppError> {
        use crate::domain::model_file_validator::FileExpectation;

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
        _curated: &crate::features::model_management::domain::ModelMetadata,
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

    fn fallback_url_for_onnx_metadata(file_name: &str, url: &str) -> Option<String> {
        // In some repos (e.g. Xenova), ONNX weights live in /onnx/ but tokenizer/config files
        // live at repo root. If curated metadata uses /onnx/ for these files, retry at root.
        if file_name == "model.onnx" || file_name == "model.onnx_data" {
            return None;
        }

        let onnx_segment = "/resolve/main/onnx/";
        if url.contains(onnx_segment) {
            Some(url.replacen(onnx_segment, "/resolve/main/", 1))
        } else {
            None
        }
    }

    /// Build a file list for Candle-backed embedding models.
    ///
    /// These require exactly:
    /// - `weights_filename` — the weights, `model.safetensors` for almost
    ///   every repo and `pytorch_model.bin` for the ones that never published
    ///   a safetensors conversion (BAAI/bge-m3). The caller picks which,
    ///   mirroring the catalog adapter's preference order.
    /// - `tokenizer.json` — tokenizer vocab
    /// - `config.json` — model configuration (hidden_size, architecture)
    ///
    /// Optionally includes `1_Pooling/config.json` if it exists at the repo
    /// root — this tells the inference service whether to CLS- or mean-pool.
    /// Absent is fine: `CandleEmbeddingService` defaults to CLS.
    ///
    /// Also probes for `sparse_linear.pt`, the learned sparse head BGE-M3 and
    /// its derivatives publish. It is a few kilobytes and only exists on
    /// hybrid dense/sparse checkpoints; fetching it is what lets
    /// `CandleEmbeddingService` report `supports_sparse()` and lets the sparse
    /// retrieval branch run. A repo without one is unaffected.
    async fn build_safetensors_embedding_file_list(
        repo_id: &str,
        weights_filename: &str,
        auth_token: Option<&str>,
    ) -> Vec<crate::domain::model_metadata::ModelFileMetadata> {
        use crate::domain::model_metadata::ModelFileMetadata;

        let base_url = format!("https://huggingface.co/{}/resolve/main", repo_id);
        let mut files = vec![
            ModelFileMetadata::new(
                weights_filename.to_string(),
                format!("{}/{}", base_url, weights_filename),
                0,
            ),
            ModelFileMetadata::new(
                "tokenizer.json".to_string(),
                format!("{}/tokenizer.json", base_url),
                0,
            ),
            ModelFileMetadata::new(
                "config.json".to_string(),
                format!("{}/config.json", base_url),
                0,
            ),
        ];

        let sparse_head = crate::features::embedding::sparse_head::SPARSE_HEAD_PT;
        let sparse_url = format!("{}/{}", base_url, sparse_head);
        if Self::remote_file_exists(&sparse_url, auth_token).await {
            files.push(ModelFileMetadata::new(
                sparse_head.to_string(),
                sparse_url,
                0,
            ));
        }

        files
    }

    /// Build a file list for ONNX embedding models with required companion files.
    ///
    /// ONNX embedding models require at minimum:
    /// - The ONNX model file itself (e.g., `model.onnx` or `onnx/model.onnx`)
    /// - `tokenizer.json` — required by OnnxEmbeddingService for tokenization
    /// - `config.json` — model configuration
    ///
    /// Companion files are resolved at repo root since most HF repos store
    /// tokenizer/config at root even when the ONNX file is in a subdirectory.
    /// The `fallback_url_for_onnx_metadata` method handles the onnx/ → root
    /// fallback for repos that use the onnx/ subdirectory convention.
    async fn build_onnx_embedding_file_list(
        repo_id: &str,
        onnx_filename: &str,
    ) -> Vec<crate::domain::model_metadata::ModelFileMetadata> {
        use crate::domain::model_metadata::ModelFileMetadata;

        let base_url = format!("https://huggingface.co/{}/resolve/main", repo_id);

        // Many modern ONNX embedding models (EmbeddingGemma, BGE-M3, etc.) ship
        // external weights in a sibling `*.onnx_data` file. The `.onnx` graph
        // alone is ~500KB and will fail to load without its companion weights.
        // Probe HF for the sibling and include it if present.
        let onnx_data_filename = format!("{}_data", onnx_filename);
        let onnx_data_url = format!("{}/{}", base_url, onnx_data_filename);
        // Note: caller `resolve_model_metadata` does not currently thread an
        // auth token here. ONNX is the legacy path on its way out; if we
        // ever need gated-model ONNX support, add an auth_token parameter.
        let include_onnx_data = Self::remote_file_exists(&onnx_data_url, None).await;

        let mut files = vec![
            ModelFileMetadata::new(
                onnx_filename.to_string(),
                format!("{}/{}", base_url, onnx_filename),
                0,
            ),
            ModelFileMetadata::new(
                "tokenizer.json".to_string(),
                format!("{}/tokenizer.json", base_url),
                0,
            ),
            ModelFileMetadata::new(
                "config.json".to_string(),
                format!("{}/config.json", base_url),
                0,
            ),
        ];

        if include_onnx_data {
            files.push(ModelFileMetadata::new(onnx_data_filename, onnx_data_url, 0));
        }

        files
    }

    /// HEAD-check a URL on Hugging Face to confirm a file exists before adding
    /// it to the download list. HF returns 200 on HEAD (after redirects) for
    /// existing files and 404 otherwise.
    async fn remote_file_exists(url: &str, auth_token: Option<&str>) -> bool {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
        {
            Ok(c) => c,
            Err(_) => return false,
        };
        let mut req = client.head(url);
        if let Some(token) = auth_token {
            req = req.bearer_auth(token);
        }
        req.send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    fn infer_model_name_from_repo(repo_id: &str) -> String {
        repo_id
            .split('/')
            .next_back()
            .unwrap_or(repo_id)
            .replace(['-', '_'], " ")
    }

    async fn resolve_model_metadata(
        &self,
        model_id: &str,
    ) -> Result<crate::features::model_management::domain::ModelMetadata, AppError> {
        // 1) Curated model IDs are still supported for backward compatibility.
        if let Some(curated) = get_all_curated_models()
            .into_iter()
            .find(|m| m.id == model_id)
        {
            return Ok(curated);
        }

        // 2) New Hugging Face internal ID path.
        if let Some((repo_id, filename)) = ExternalModelMetadata::decode_hf_download_id(model_id) {
            let mut resolved = match self.catalog.get_model_by_id(&repo_id).await? {
                Some(meta) => meta.to_domain_model().map_err(AppError::InvalidInput)?,
                None => crate::features::model_management::domain::ModelMetadata {
                    id: model_id.to_string(),
                    name: Self::infer_model_name_from_repo(&repo_id),
                    category: crate::features::model_management::domain::ModelCategory::LLM,
                    description: format!("Hugging Face model: {}", repo_id),
                    size_gb: 0.0,
                    minimum_ram_gb: 4.0,
                    recommended_ram_gb: 8.0,
                    context_length: 4096,
                    performance_tier:
                        crate::features::model_management::domain::PerformanceTier::Balanced,
                    supported_quantizations: vec![],
                    capabilities: vec!["chat".into()],
                    download_url: Some(format!("https://huggingface.co/{}", repo_id)),
                    license: "unknown".into(),
                    requires_auth: false,
                    model_id: Some(repo_id.clone()),
                    default_filename: Some(filename.clone()),
                    files: vec![],
                    total_size_bytes: 0,
                    embedding_dimensions: None,
                    embedding_compatibility: None,
                    format: crate::features::llm::engine::models::ModelFormat::Gguf,
                },
            };

            // Override model identity with the user-selected internal ID and file.
            resolved.id = model_id.to_string();
            resolved.model_id = Some(repo_id.clone());
            resolved.default_filename = Some(filename.clone());
            resolved.download_url = Some(format!("https://huggingface.co/{}", repo_id));
            resolved.files.clear();
            if resolved.total_size_bytes == 0 && resolved.size_gb > 0.0 {
                resolved.total_size_bytes = (resolved.size_gb * 1_000_000_000.0) as u64;
            }

            // Embedding models need a multi-file download (weights + tokenizer
            // + config). Dispatch on filename extension — safetensors is the
            // Candle path; .onnx is the legacy ORT path (will be removed once
            // all consumers have migrated).
            if resolved.category
                == crate::features::model_management::domain::ModelCategory::Embedding
            {
                // For gated repos, fetch the user's HF token so HEAD probes
                // (e.g., 1_Pooling/config.json) don't 401 silently and skip
                // a file we'd actually need.
                let auth_token: Option<String> = if resolved.requires_auth {
                    self.credentials
                        .get_api_key("huggingface_token")
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };

                // `pytorch_model.bin` joins the Candle branch: the catalog
                // adapter picks it when a repo ships no safetensors, and the
                // loader reads it through `candle_core::pickle`. Without this
                // it would fall through to a bare single-file download with
                // no tokenizer or config beside it.
                if filename.ends_with(".safetensors")
                    || filename == crate::features::embedding::candle_service::WEIGHTS_PYTORCH_BIN
                {
                    resolved.files = Self::build_safetensors_embedding_file_list(
                        &repo_id,
                        &filename,
                        auth_token.as_deref(),
                    )
                    .await;
                    resolved.default_filename = None;
                } else if filename.ends_with(".onnx") {
                    resolved.files =
                        Self::build_onnx_embedding_file_list(&repo_id, &filename).await;
                    resolved.default_filename = None;
                }
            }

            return Ok(resolved);
        }

        // 3) Compatibility fallback: search by internal ID in cached external catalog.
        let candidates = self.catalog.search_models("gguf", 300).await?;
        if let Some(found) = candidates
            .into_iter()
            .filter_map(|meta| meta.to_domain_model().ok())
            .find(|model| model.id == model_id)
        {
            return Ok(found);
        }

        Err(AppError::NotFound(format!(
            "Model '{}' not found in Hugging Face catalog",
            model_id
        )))
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
            == crate::features::model_management::domain::ModelCategory::Embedding
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

    /// Download a single-file model (legacy path for GGUF models)
    async fn download_single_file_model(
        &self,
        model_id: &str,
        curated: &crate::features::model_management::domain::ModelMetadata,
        model_path: &std::path::Path,
        paths: &ModelPaths,
    ) -> Result<DownloadModelResponseDto, AppError> {
        let download_url = curated.download_url.clone().ok_or_else(|| {
            AppError::InvalidInput(format!("No download URL for model: {}", model_id))
        })?;
        let default_filename = curated.default_filename.clone().ok_or_else(|| {
            AppError::InvalidInput(format!("No filename specified for model: {}", model_id))
        })?;
        let model_name = &curated.name;

        // For HuggingFace models, build the resolve URL using the actual filename
        let model_file_url = if download_url.contains("huggingface.co") {
            format!("{}/resolve/main/{}", download_url, default_filename)
        } else {
            download_url
        };

        let destination = paths.manifest_file_path(&default_filename)?;

        debug!(
            url = %model_file_url,
            destination = %destination.display(),
            "Initiating single-file model download"
        );

        // STEP 1: Create parent Model record FIRST (satisfies FK constraint)
        // Use explicit scope to ensure connections are released before download starts
        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_repo = uow.model_repository()?;

            let model = Model {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                name: model_name.clone(),
                description: Some(curated.description.clone()),
                base_path: model_path.to_string_lossy().to_string(),
                total_size_bytes: curated.total_size_bytes as i64,
                architecture: "GGUF".to_string(), // Default to GGUF format for LLM models
                model_type: match curated.category {
                    crate::features::model_management::domain::ModelCategory::LLM => {
                        "chat".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Embedding => {
                        "embedding".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::OCR => {
                        "ocr".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Transcription => {
                        // See ModelType::to_db_string: the models-table CHECK
                        // constraint has no `transcription` value yet.
                        "custom".to_string()
                    }
                },
                status: ModelStatus::Pending,
                files: vec![],
                is_active_for_chat: false,
                is_active_for_embedding: false,
                use_count: 0,
                last_used_at: None,
                metadata: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            };

            model_repo
                .create_model_with_files(&model, vec![])
                .await
                .map_err(|e| AppError::Database(format!("Failed to create model record: {}", e)))?;

            info!(
                model_id = %model_id,
                model_name = %model_name,
                "Created parent model record with Pending status"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model record transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model record: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_file_repo = uow.model_file_repository()?;

            let model_file = ModelFile {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                file_name: default_filename.clone(),
                file_path: destination.to_string_lossy().to_string(),
                relative_path: default_filename.clone(),
                size_bytes: curated.total_size_bytes as i64,
                downloaded_bytes: 0,
                checksum_sha256: curated.files.first().and_then(|f| f.checksum.clone()),
                download_url: model_file_url.clone(),
                status: FileStatus::Pending,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            };

            model_file_repo.create(&model_file).await.map_err(|e| {
                AppError::Database(format!("Failed to create model_file record: {}", e))
            })?;

            info!(
                model_id = %model_id,
                file_name = %default_filename,
                "Created model_file record with Pending status"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model_file transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model_file record: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        // Yield to allow connection pool to reclaim connections
        tokio::task::yield_now().await;

        let auth_token = if curated.requires_auth {
            match self.credentials.get_api_key("huggingface_token").await {
                Ok(Some(token)) => {
                    info!(model_id = %model_id, "Model requires authentication, using HuggingFace token");
                    Some(token)
                }
                Ok(None) => {
                    return Err(AppError::InvalidInput(format!(
                        "Model '{}' requires HuggingFace authentication. Please set your HuggingFace token in Settings → HuggingFace.",
                        model_name
                    )));
                }
                Err(e) => {
                    warn!(error = ?e, model_id = %model_id, "Failed to retrieve HuggingFace token from secure storage");
                    return Err(AppError::Other(format!(
                        "Failed to retrieve HuggingFace token: {}. Please check your keychain settings.",
                        e
                    )));
                }
            }
        } else {
            None
        };

        let download_request = DownloadRequest {
            url: model_file_url.clone(),
            destination: destination.clone(),
            checksum: None,
            auth_token,
            model_name: Some(model_name.clone()),
            model_id: Some(model_id.to_string()),
            model_file_name: Some(default_filename.clone()),
        };

        let download_id = match self.download_manager.start_download(download_request).await {
            Ok(id) => id,
            Err(e) => {
                // Detect network errors
                let error_msg = e.to_string();
                let mapped_error = if error_msg.contains("Network")
                    || error_msg.contains("connection")
                    || error_msg.contains("timeout")
                {
                    AppError::Network(format!("Network error: {}", error_msg))
                } else {
                    AppError::Network(format!("Failed to start download: {}", e))
                };

                // Mark model + file as failed before returning error
                let mut uow = self.uow_factory.create().await?;
                let update_result = {
                    let model_repo = uow.model_repository()?;
                    let model_file_repo = uow.model_file_repository()?;

                    model_repo.update_status_failed(model_id).await?;
                    model_file_repo
                        .update_file_status_by_model_and_name(
                            model_id,
                            &default_filename,
                            FileStatus::Failed,
                            None,
                        )
                        .await?;
                    Ok::<(), AppError>(())
                };

                match update_result {
                    Ok(()) => {
                        uow.commit().await?;
                    }
                    Err(err) => {
                        if let Err(rollback_err) = uow.rollback().await {
                            return Err(AppError::Database(format!(
                                "Failed to mark download failed: {}; rollback failed: {}",
                                err, rollback_err
                            )));
                        }
                        return Err(err);
                    }
                }

                return Err(mapped_error);
            }
        };

        info!(
            download_id = %download_id,
            model_id = %model_id,
            "Single-file download initiated, waiting for completion"
        );

        // Prefer the verified session size discovered by DownloadManager (HEAD/Range),
        // falling back to catalog metadata when unavailable.
        let total_size_bytes = self
            .download_manager
            .get_download_status(&download_id)
            .await
            .ok()
            .flatten()
            .and_then(|session| session.progress().total_bytes())
            .unwrap_or(curated.total_size_bytes);

        Ok(DownloadModelResponseDto {
            state: DownloadOperationState::DownloadStarted {
                total_size_bytes,
                files_to_download: 1,
            },
            path: Some(model_path.to_string_lossy().to_string()),
        })
    }

    /// Download a multi-file model (e.g., BGE-M3 with multiple ONNX files)
    async fn download_multi_file_model(
        &self,
        model_id: &str,
        curated: &crate::features::model_management::domain::ModelMetadata,
        model_path: &std::path::Path,
        paths: &ModelPaths,
    ) -> Result<DownloadModelResponseDto, AppError> {
        let model_name = &curated.name;
        let mut download_ids = Vec::new();

        // MODULE 3: Enhanced logging & verification at START
        info!(
            model_id = %model_id,
            file_count = curated.files.len(),
            files = ?curated.files.iter().map(|f| &f.filename).collect::<Vec<_>>(),
            total_size_gb = curated.total_size_bytes as f64 / 1_000_000_000.0,
            "Starting multi-file model download"
        );

        // STEP 1: Create parent Model record FIRST (satisfies FK constraint)
        // Use explicit scope to ensure connections are released before download starts
        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_repo = uow.model_repository()?;

            let model = Model {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.to_string(),
                name: model_name.clone(),
                description: Some(curated.description.clone()),
                base_path: model_path.to_string_lossy().to_string(),
                total_size_bytes: curated.total_size_bytes as i64,
                architecture: "GGUF".to_string(), // Default to GGUF format for LLM models
                model_type: match curated.category {
                    crate::features::model_management::domain::ModelCategory::LLM => {
                        "chat".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Embedding => {
                        "embedding".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::OCR => {
                        "ocr".to_string()
                    }
                    crate::features::model_management::domain::ModelCategory::Transcription => {
                        // See ModelType::to_db_string: the models-table CHECK
                        // constraint has no `transcription` value yet.
                        "custom".to_string()
                    }
                },
                status: ModelStatus::Pending,
                files: vec![],
                is_active_for_chat: false,
                is_active_for_embedding: false,
                use_count: 0,
                last_used_at: None,
                metadata: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            };

            model_repo
                .create_model_with_files(&model, vec![])
                .await
                .map_err(|e| AppError::Database(format!("Failed to create model record: {}", e)))?;

            info!(
                model_id = %model_id,
                model_name = %model_name,
                "Created parent model record with Pending status for multi-file model"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model record transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model record: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let model_file_repo = uow.model_file_repository()?;

            for file in &curated.files {
                let destination = paths.manifest_file_path(&file.filename)?;

                let model_file = ModelFile {
                    id: Uuid::new_v4().to_string(),
                    model_id: model_id.to_string(),
                    file_name: file.filename.clone(),
                    file_path: destination.to_string_lossy().to_string(),
                    relative_path: file.filename.clone(),
                    size_bytes: file.size_bytes as i64,
                    downloaded_bytes: 0,
                    checksum_sha256: file.checksum.clone(),
                    download_url: file.url.clone(),
                    status: FileStatus::Pending,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    downloaded_at: None,
                };

                model_file_repo.create(&model_file).await.map_err(|e| {
                    AppError::Database(format!("Failed to create model_file record: {}", e))
                })?;

                info!(
                    model_id = %model_id,
                    file_name = %file.filename,
                    "Created model_file record with Pending status"
                );
            }

            info!(
                model_id = %model_id,
                file_count = curated.files.len(),
                "Pre-created all model_file records before download"
            );
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await.map_err(|e| {
                    AppError::Database(format!("Failed to commit model_file transaction: {}", e))
                })?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Failed to create model_file records: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        // Yield to allow connection pool to reclaim connections
        tokio::task::yield_now().await;

        let auth_token = if curated.requires_auth {
            match self.credentials.get_api_key("huggingface_token").await {
                Ok(Some(token)) => {
                    info!(model_id = %model_id, "Model requires authentication, using HuggingFace token for all files");
                    Some(token)
                }
                Ok(None) => {
                    return Err(AppError::InvalidInput(format!(
                        "Model '{}' requires HuggingFace authentication. Please set your HuggingFace token in Settings → HuggingFace.",
                        model_name
                    )));
                }
                Err(e) => {
                    warn!(error = ?e, model_id = %model_id, "Failed to retrieve HuggingFace token from secure storage");
                    return Err(AppError::Other(format!(
                        "Failed to retrieve HuggingFace token: {}. Please check your keychain settings.",
                        e
                    )));
                }
            }
        } else {
            None
        };

        let mut batch_items = Vec::with_capacity(curated.files.len());
        for (index, file) in curated.files.iter().enumerate() {
            let destination = paths.manifest_file_path(&file.filename)?;

            // MODULE 4: Enhanced download status tracking
            info!(
                file = %file.filename,
                file_num = index + 1,
                total_files = curated.files.len(),
                size_mb = file.size_bytes as f64 / 1_000_000.0,
                url = %file.url,
                "Starting file download"
            );

            let checksum = if let Some(checksum_str) = &file.checksum {
                Some(
                    crate::domain::download::Checksum::new(
                        crate::domain::download::ChecksumAlgorithm::Sha256,
                        checksum_str.clone(),
                    )
                    .map_err(|e| {
                        AppError::InvalidInput(format!(
                            "Invalid checksum for {}: {}",
                            file.filename, e
                        ))
                    })?,
                )
            } else {
                None
            };

            let mut attempt_urls = vec![file.url.clone()];
            if let Some(fallback_url) =
                Self::fallback_url_for_onnx_metadata(&file.filename, &file.url)
            {
                if fallback_url != file.url {
                    attempt_urls.push(fallback_url);
                }
            }

            let requests = attempt_urls
                .into_iter()
                .map(|attempt_url| DownloadRequest {
                    url: attempt_url.clone(),
                    destination: destination.clone(),
                    checksum: checksum.clone(),
                    auth_token: auth_token.clone(),
                    model_name: Some(model_name.clone()),
                    model_id: Some(model_id.to_string()),
                    model_file_name: Some(file.filename.clone()),
                })
                .collect();
            batch_items.push(DownloadBatchItem { requests });
        }

        let ids = match self
            .download_manager
            .start_download_batch(batch_items)
            .await
        {
            Ok(ids) => ids,
            Err(error) => {
                let mut uow = self.uow_factory.create().await?;
                let update_result = {
                    let model_repo = uow.model_repository()?;
                    let model_file_repo = uow.model_file_repository()?;

                    model_repo.update_status_failed(model_id).await?;
                    for file in &curated.files {
                        model_file_repo
                            .update_file_status_by_model_and_name(
                                model_id,
                                &file.filename,
                                FileStatus::Failed,
                                Some(file.size_bytes as i64),
                            )
                            .await?;
                    }
                    Ok::<(), AppError>(())
                };

                match update_result {
                    Ok(()) => uow.commit().await?,
                    Err(err) => {
                        if let Err(rollback_err) = uow.rollback().await {
                            return Err(AppError::Database(format!(
                                "Failed to mark download failed: {}; rollback failed: {}",
                                err, rollback_err
                            )));
                        }
                        return Err(err);
                    }
                }

                return Err(AppError::Network(format!(
                    "Failed to prepare model download: {}",
                    error
                )));
            }
        };

        download_ids.extend(
            ids.into_iter()
                .zip(curated.files.iter().map(|file| file.filename.clone())),
        );

        info!(
            model_id = %model_id,
            download_count = download_ids.len(),
            "All downloads initiated, returning immediately"
        );

        // Sum concrete session sizes from DownloadManager to avoid surfacing stale/estimated
        // catalog sizes in UI while download starts.
        let mut detected_total_size_bytes = 0u64;
        for (download_id, _) in &download_ids {
            if let Ok(Some(session)) = self.download_manager.get_download_status(download_id).await
            {
                if let Some(total_bytes) = session.progress().total_bytes() {
                    detected_total_size_bytes =
                        detected_total_size_bytes.saturating_add(total_bytes);
                }
            }
        }
        let total_size_bytes = if detected_total_size_bytes > 0 {
            detected_total_size_bytes
        } else {
            curated.total_size_bytes
        };

        Ok(DownloadModelResponseDto {
            state: DownloadOperationState::DownloadStarted {
                total_size_bytes,
                files_to_download: curated.files.len(),
            },
            path: Some(model_path.to_string_lossy().to_string()),
        })
    }
}
