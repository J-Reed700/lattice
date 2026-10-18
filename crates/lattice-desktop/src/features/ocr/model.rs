//! Where the downloaded OCR model lives.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;

use lattice_core::application::ports::{ModelStoragePort, OcrError};
use lattice_core::domain::models::curated::{
    CURATED_OCR_MODELS, OCR_MODEL_FILE, OCR_PROJECTOR_FILE,
};

/// The two files one OCR server is started with, and the instruction that
/// model expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrModel {
    pub id: String,
    /// The language model, passed as `-m`.
    pub weights: PathBuf,
    /// The vision projector, passed as `--mmproj`.
    pub projector: PathBuf,
    pub prompt: String,
}

impl OcrModel {
    /// The model laid out the way the catalog downloads it: both files, under
    /// their fixed local names, in `dir`.
    pub fn in_dir(id: &str, dir: &Path, prompt: &str) -> Self {
        Self {
            id: id.to_string(),
            weights: dir.join(OCR_MODEL_FILE),
            projector: dir.join(OCR_PROJECTOR_FILE),
            prompt: prompt.to_string(),
        }
    }
}

/// Answers "which OCR model, if any".
#[async_trait]
pub trait OcrModelSource: Send + Sync {
    /// The model to use, or `None` when no OCR model is downloaded.
    async fn locate(&self) -> Result<Option<OcrModel>, OcrError>;
}

/// The first catalog OCR entry that is downloaded and complete on disk.
///
/// Asked on every page rather than once at startup, so downloading the model
/// turns OCR on for the next scan without a restart. The query is one indexed
/// row read; a page of OCR costs seconds.
pub struct DownloadedOcrModel {
    storage: Arc<dyn ModelStoragePort>,
}

impl DownloadedOcrModel {
    pub fn new(storage: Arc<dyn ModelStoragePort>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl OcrModelSource for DownloadedOcrModel {
    async fn locate(&self) -> Result<Option<OcrModel>, OcrError> {
        for entry in CURATED_OCR_MODELS {
            let downloaded = self
                .storage
                .is_model_downloaded(entry.id)
                .await
                .map_err(|error| OcrError::Unavailable {
                    reason: format!("could not read the model list: {error}"),
                })?;
            if !downloaded {
                continue;
            }
            // The record points at whichever `.gguf` the download saga chose
            // as primary; both files sit in its directory either way.
            let recorded = match self.storage.get_model_path(entry.id).await {
                Ok(path) => path,
                Err(error) => {
                    warn!(model_id = entry.id, %error, "OCR model is recorded as downloaded but cannot be found");
                    continue;
                }
            };
            // repository-barrier-allow: distinguishing a file location from a directory location of the same record.
            let dir = if recorded.is_dir() {
                recorded.as_path()
            } else if let Some(parent) = recorded.parent() {
                parent
            } else {
                continue;
            };
            let model = OcrModel::in_dir(entry.id, dir, entry.prompt);
            // repository-barrier-allow: the projector is a sibling file the model record does not list; its presence is what makes the entry usable.
            if model.weights.is_file() && model.projector.is_file() {
                return Ok(Some(model));
            }
            warn!(
                model_id = entry.id,
                dir = %dir.display(),
                "OCR model is missing its weights or vision projector; download it again"
            );
        }
        Ok(None)
    }
}

/// A model at a known location, or none. For the live smoke example and tests.
pub struct FixedOcrModel(pub Option<OcrModel>);

#[async_trait]
impl OcrModelSource for FixedOcrModel {
    async fn locate(&self) -> Result<Option<OcrModel>, OcrError> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lattice_core::application::ports::model_storage::{DownloadedModel, MockModelStoragePort};

    fn storage_with(path: PathBuf) -> Arc<dyn ModelStoragePort> {
        let storage = MockModelStoragePort::new();
        storage.add_model(DownloadedModel {
            model_id: CURATED_OCR_MODELS[0].id.to_string(),
            path,
            size_bytes: 1,
            downloaded_at: chrono::Utc::now(),
        });
        Arc::new(storage)
    }

    #[tokio::test]
    async fn nothing_downloaded_means_no_model() {
        let source = DownloadedOcrModel::new(Arc::new(MockModelStoragePort::new()));
        assert_eq!(source.locate().await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_complete_download_yields_both_files_and_the_card_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(OCR_MODEL_FILE), b"gguf").unwrap();
        std::fs::write(dir.path().join(OCR_PROJECTOR_FILE), b"gguf").unwrap();
        let source = DownloadedOcrModel::new(storage_with(dir.path().join(OCR_MODEL_FILE)));

        let model = source.locate().await.unwrap().expect("model found");

        assert_eq!(model.weights, dir.path().join(OCR_MODEL_FILE));
        assert_eq!(model.projector, dir.path().join(OCR_PROJECTOR_FILE));
        assert_eq!(model.prompt, "OCR:");
    }

    /// A record without its projector would start a text-only server that
    /// rejects every image; better to report "not configured".
    #[tokio::test]
    async fn a_download_missing_its_projector_is_not_offered() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(OCR_MODEL_FILE), b"gguf").unwrap();
        let source = DownloadedOcrModel::new(storage_with(dir.path().join(OCR_MODEL_FILE)));

        assert_eq!(source.locate().await.unwrap(), None);
    }
}
