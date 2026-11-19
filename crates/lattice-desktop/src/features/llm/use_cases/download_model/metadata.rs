//! Resolving what to download: curated or Hugging Face metadata and embedding file lists.

use lattice_core::application::ports::model_catalog::ExternalModelMetadata;
use lattice_core::domain::models::curated::get_all_curated_models;
use lattice_core::shared::error::AppError;

use super::DownloadModelUseCase;

impl DownloadModelUseCase {
    /// The files of an embedding model picked from a Hugging Face search: the
    /// chosen GGUF alone, which is the whole model. Its size is left unknown
    /// (0): the transfer engine takes it from the server rather than from a
    /// catalog estimate.
    fn build_gguf_embedding_file_list(
        repo_id: &str,
        gguf_filename: &str,
    ) -> Vec<lattice_core::domain::models::metadata::ModelFileMetadata> {
        use lattice_core::domain::models::metadata::ModelFileMetadata;

        vec![ModelFileMetadata::new(
            gguf_filename.to_string(),
            format!(
                "https://huggingface.co/{}/resolve/main/{}",
                repo_id, gguf_filename
            ),
            0,
        )]
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

            // An embedding GGUF downloads as its one file on the file-list path.
            if resolved.category
                == lattice_core::domain::models::selection::ModelCategory::Embedding
                && filename.to_ascii_lowercase().ends_with(".gguf")
            {
                resolved.files = Self::build_gguf_embedding_file_list(&repo_id, &filename);
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
