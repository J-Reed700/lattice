//! Resolving what to download: curated or Hugging Face metadata and embedding file lists.

use lattice_core::application::ports::model_catalog::ExternalModelMetadata;
use lattice_core::domain::models::curated::get_all_curated_models;
use lattice_core::shared::error::AppError;

use super::DownloadModelUseCase;

impl DownloadModelUseCase {
    /// The files of an embedding model picked from a Hugging Face search: the
    /// chosen GGUF, plus the repository's `tokenizer.json` when it has one.
    ///
    /// The embedding runtime cuts passages with the model's own tokenizer, so
    /// a GGUF without one beside it downloads but cannot be activated for
    /// search; activation says so. Sizes are left unknown (0): the transfer
    /// engine takes them from the server rather than from a catalog estimate.
    async fn build_gguf_embedding_file_list(
        repo_id: &str,
        gguf_filename: &str,
        auth_token: Option<&str>,
    ) -> Vec<lattice_core::domain::models::metadata::ModelFileMetadata> {
        use lattice_core::domain::models::checkpoint::TOKENIZER_JSON;
        use lattice_core::domain::models::metadata::ModelFileMetadata;

        let base_url = format!("https://huggingface.co/{}/resolve/main", repo_id);
        let mut files = vec![ModelFileMetadata::new(
            gguf_filename.to_string(),
            format!("{}/{}", base_url, gguf_filename),
            0,
        )];

        let tokenizer_url = format!("{}/{}", base_url, TOKENIZER_JSON);
        if Self::remote_file_exists(&tokenizer_url, auth_token).await {
            files.push(ModelFileMetadata::new(
                TOKENIZER_JSON.to_string(),
                tokenizer_url,
                0,
            ));
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

            // An embedding GGUF downloads with the tokenizer its passages are
            // cut with, so it takes the multi-file path.
            if resolved.category
                == lattice_core::domain::models::selection::ModelCategory::Embedding
                && filename.to_ascii_lowercase().ends_with(".gguf")
            {
                // A gated repo answers the tokenizer probe only with the
                // user's token.
                let auth_token: Option<String> = if resolved.requires_auth {
                    self.credentials
                        .get_api_key("huggingface_token")
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };

                resolved.files = Self::build_gguf_embedding_file_list(
                    &repo_id,
                    &filename,
                    auth_token.as_deref(),
                )
                .await;
                resolved.default_filename = None;
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
