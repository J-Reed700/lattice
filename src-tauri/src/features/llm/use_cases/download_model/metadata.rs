//! Resolving what to download: curated or Hugging Face metadata and embedding file lists.

use lattice_core::application::ports::model_catalog::ExternalModelMetadata;
use lattice_core::domain::models::curated::get_all_curated_models;
use lattice_core::shared::error::AppError;

use super::DownloadModelUseCase;

impl DownloadModelUseCase {
    pub(super) fn fallback_url_for_onnx_metadata(file_name: &str, url: &str) -> Option<String> {
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
    ) -> Vec<lattice_core::domain::models::metadata::ModelFileMetadata> {
        use lattice_core::domain::models::metadata::ModelFileMetadata;

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

        let sparse_head = lattice_core::domain::models::checkpoint::SPARSE_HEAD_PT;
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
    ) -> Vec<lattice_core::domain::models::metadata::ModelFileMetadata> {
        use lattice_core::domain::models::metadata::ModelFileMetadata;

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

    pub(super) async fn resolve_model_metadata(
        &self,
        model_id: &str,
    ) -> Result<lattice_core::domain::models::selection::ModelMetadata, AppError> {
        // 1) Curated model IDs are still supported for backward compatibility.
        if let Some(curated) = get_all_curated_models()
            .into_iter()
            .find(|m| m.id == model_id)
        {
            return Ok(curated);
        }

        // 2) New Hugging Face internal ID path.
        if let Some((repo_id, filename)) = ExternalModelMetadata::decode_hf_download_id(model_id) {
            // Resolve this exact artifact, not the repository's default Q4 file.
            // Otherwise choosing Q8 can inherit Q4's size and memory metadata.
            let metadata = if filename.to_ascii_lowercase().ends_with(".gguf") {
                Some(self.catalog.get_model_variants(&repo_id).await?
                    .into_iter()
                    .find(|meta| meta.preferred_filename.as_deref() == Some(filename.as_str()))
                    .ok_or_else(|| AppError::NotFound(format!(
                        "Standalone model file '{}' is no longer available in {}. Refresh the catalog versions.", filename, repo_id
                    )))?)
            } else {
                self.catalog.get_model_by_id(&repo_id).await?
            };
            let mut resolved = match metadata {
                Some(meta) => meta.to_domain_model()?,
                None => lattice_core::domain::models::selection::ModelMetadata {
                    id: model_id.to_string(),
                    name: Self::infer_model_name_from_repo(&repo_id),
                    category: lattice_core::domain::models::selection::ModelCategory::LLM,
                    description: format!("Hugging Face model: {}", repo_id),
                    size_gb: 0.0,
                    minimum_ram_gb: 4.0,
                    recommended_ram_gb: 8.0,
                    context_length: 4096,
                    performance_tier:
                        lattice_core::domain::models::selection::PerformanceTier::Balanced,
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
                == lattice_core::domain::models::selection::ModelCategory::Embedding
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
                    || filename == lattice_core::domain::models::checkpoint::WEIGHTS_PYTORCH_BIN
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
}
