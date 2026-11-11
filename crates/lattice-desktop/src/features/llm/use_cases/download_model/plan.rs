//! What a model download fetches, resolved once from the catalog entry.

use lattice_core::domain::models::selection::ModelMetadata;
use lattice_core::shared::error::AppError;

/// One file of a download as the file row, the transfer and verification see it.
pub(super) struct PlannedFile {
    pub filename: String,
    pub url: String,
    /// What the file row records.
    pub size_bytes: u64,
    /// Checked on verification only when the catalog knows the size exactly.
    pub expected_size: Option<u64>,
    pub checksum: Option<String>,
}

/// The files to fetch: the catalog's list, or the one default file.
pub(super) fn planned_files(
    model_id: &str,
    meta: &ModelMetadata,
) -> Result<Vec<PlannedFile>, AppError> {
    if !meta.files.is_empty() {
        return Ok(meta
            .files
            .iter()
            .map(|file| PlannedFile {
                filename: file.filename.clone(),
                url: file.url.clone(),
                size_bytes: file.size_bytes,
                expected_size: (file.size_bytes > 0).then_some(file.size_bytes),
                checksum: file.checksum.clone(),
            })
            .collect());
    }

    let (filename, download_url) = match (&meta.default_filename, &meta.download_url) {
        (Some(filename), Some(url)) => (filename, url),
        (None, None) => {
            return Err(AppError::InvalidInput(
                "No files or default filename specified for model".into(),
            ))
        }
        (None, Some(_)) => {
            return Err(AppError::InvalidInput(format!(
                "No filename specified for model: {}",
                model_id
            )))
        }
        (Some(_), None) => {
            return Err(AppError::InvalidInput(format!(
                "No download URL for model: {}",
                model_id
            )))
        }
    };

    let url = if download_url.contains("huggingface.co") {
        format!("{}/resolve/main/{}", download_url, filename)
    } else {
        download_url.clone()
    };

    Ok(vec![PlannedFile {
        filename: filename.clone(),
        url,
        size_bytes: meta.total_size_bytes,
        expected_size: None,
        checksum: None,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::domain::models::metadata::ModelFileMetadata;
    use lattice_core::domain::models::selection::{ModelCategory, ModelFormat, PerformanceTier};

    fn meta(
        files: Vec<ModelFileMetadata>,
        filename: Option<&str>,
        url: Option<&str>,
    ) -> ModelMetadata {
        ModelMetadata {
            id: "plan-test".into(),
            name: "Plan Test".into(),
            category: ModelCategory::LLM,
            description: "a synthetic model".into(),
            size_gb: 0.0,
            context_length: 512,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec![],
            capabilities: vec![],
            download_url: url.map(str::to_string),
            license: "unknown".into(),
            requires_auth: false,
            model_id: None,
            default_filename: filename.map(str::to_string),
            files,
            total_size_bytes: 42,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        }
    }

    #[test]
    fn a_files_list_maps_one_to_one_and_a_zero_size_means_no_expected_size() {
        let mut known = ModelFileMetadata::new("a.bin".into(), "https://x/a".into(), 8);
        known.checksum = Some("abc".into());
        let unknown = ModelFileMetadata::new("b.json".into(), "https://x/b".into(), 0);
        let plan = planned_files("m", &meta(vec![known, unknown], Some("ignored"), None)).unwrap();

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].filename, "a.bin");
        assert_eq!(plan[0].url, "https://x/a");
        assert_eq!(plan[0].size_bytes, 8);
        assert_eq!(plan[0].expected_size, Some(8));
        assert_eq!(plan[0].checksum.as_deref(), Some("abc"));
        assert_eq!(plan[1].size_bytes, 0);
        assert_eq!(plan[1].expected_size, None);
        assert_eq!(plan[1].checksum, None);
    }

    #[test]
    fn a_single_file_on_hugging_face_gets_the_resolve_url() {
        let plan = planned_files(
            "m",
            &meta(
                vec![],
                Some("w.gguf"),
                Some("https://huggingface.co/org/repo"),
            ),
        )
        .unwrap();

        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan[0].url,
            "https://huggingface.co/org/repo/resolve/main/w.gguf"
        );
        assert_eq!(plan[0].size_bytes, 42);
        assert_eq!(plan[0].expected_size, None);
    }

    #[test]
    fn a_single_file_on_another_host_keeps_its_url() {
        let plan = planned_files(
            "m",
            &meta(vec![], Some("w.gguf"), Some("https://example.com/w.gguf")),
        )
        .unwrap();

        assert_eq!(plan[0].url, "https://example.com/w.gguf");
    }

    #[test]
    fn a_missing_filename_is_named() {
        let error = planned_files("m", &meta(vec![], None, Some("https://example.com/w")))
            .err()
            .unwrap();

        assert!(error
            .to_string()
            .contains("No filename specified for model: m"));
    }

    #[test]
    fn a_missing_url_is_named() {
        let error = planned_files("m", &meta(vec![], Some("w.gguf"), None))
            .err()
            .unwrap();

        assert!(error.to_string().contains("No download URL for model: m"));
    }

    #[test]
    fn neither_files_nor_a_filename_is_named() {
        let error = planned_files("m", &meta(vec![], None, None)).err().unwrap();

        assert!(error
            .to_string()
            .contains("No files or default filename specified for model"));
    }
}
