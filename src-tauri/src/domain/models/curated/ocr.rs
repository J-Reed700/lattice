//! The curated OCR model and its projector file.

use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::ModelFormat;
use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};

/// Local name of an OCR entry's language-model weights. Every OCR entry is
/// downloaded under these two fixed names, whatever the remote files are
/// called, so the OCR engine loads one layout:
///
///     ~/.cache/lattice/models/<id>/{model.gguf, mmproj.gguf}
pub const OCR_MODEL_FILE: &str = "model.gguf";

/// Local name of an OCR entry's vision projector, passed as `--mmproj`.
pub const OCR_PROJECTOR_FILE: &str = "mmproj.gguf";

/// A downloadable OCR model: a GGUF language model plus its vision projector,
/// served by a dedicated `llama-server`.
///
/// Sizes and SHA-256 digests are the Hugging Face tree API's `size` and
/// `lfs.oid` for the pinned files, so a truncated or substituted download
/// fails verification instead of failing at load.
#[derive(Debug, Clone, Copy)]
pub struct CuratedOcrModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Hugging Face repository holding both files.
    pub repo: &'static str,
    pub model_remote_file: &'static str,
    pub model_bytes: u64,
    pub model_sha256: &'static str,
    pub projector_remote_file: &'static str,
    pub projector_bytes: u64,
    pub projector_sha256: &'static str,
    /// The instruction the model card prescribes for plain text recognition.
    pub prompt: &'static str,
    pub license: &'static str,
    pub minimum_ram_gb: f64,
    pub recommended_ram_gb: f64,
}

/// The OCR catalog, in order of preference when several are downloaded.
///
/// PaddleOCR-VL-1.6 (0.9B, 2026-06) scores 96.33 on OmniDocBench v1.6. The
/// pinned llama.cpp lists its `paddleocr` architecture, and the official
/// PaddlePaddle GGUF loads in the bundled binary in under a second and reads a
/// dense page in about five on Apple Silicon (verified 2026-10-13). The card's
/// prompt for text is `OCR:`; it also documents `Table Recognition:` and
/// `Formula Recognition:` for cropped elements, which a whole page does not need.
pub const CURATED_OCR_MODELS: &[CuratedOcrModel] = &[CuratedOcrModel {
    id: "paddleocr-vl-1.6",
    name: "PaddleOCR-VL 1.6",
    description: "Reads scanned PDF pages so they can be searched and cited. \
                  A 0.9B document model run only while scans are being indexed.",
    repo: "PaddlePaddle/PaddleOCR-VL-1.6-GGUF",
    model_remote_file: "PaddleOCR-VL-1.6-GGUF.gguf",
    model_bytes: 935_769_056,
    model_sha256: "f3ae46ec885050acf4b3d31944431e1fd90d50664fb09126af4a3c050ba14ee8",
    projector_remote_file: "PaddleOCR-VL-1.6-GGUF-mmproj.gguf",
    projector_bytes: 881_770_560,
    projector_sha256: "204d757d7610d9b3faab10d506d69e5b244e32bf765e2bab2d0167e65e0a058a",
    prompt: "OCR:",
    license: "Apache-2.0",
    minimum_ram_gb: 4.0,
    recommended_ram_gb: 8.0,
}];

impl CuratedOcrModel {
    fn metadata(&self) -> ModelMetadata {
        let total_size_bytes = self.model_bytes + self.projector_bytes;
        let file = |local: &str, remote: &str, bytes: u64, sha256: &str| {
            ModelFileMetadata::with_checksum(
                local.into(),
                format!("https://huggingface.co/{}/resolve/main/{remote}", self.repo),
                bytes,
                sha256.into(),
            )
        };
        ModelMetadata {
            id: self.id.into(),
            name: self.name.into(),
            category: ModelCategory::OCR,
            description: self.description.into(),
            size_gb: total_size_bytes as f64 / 1_000_000_000.0,
            minimum_ram_gb: self.minimum_ram_gb,
            recommended_ram_gb: self.recommended_ram_gb,
            context_length: 4096,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["BF16".into()],
            capabilities: vec![
                "ocr".into(),
                "vision".into(),
                "document-understanding".into(),
            ],
            download_url: Some(format!("https://huggingface.co/{}", self.repo)),
            license: self.license.into(),
            requires_auth: false,
            model_id: Some(self.repo.into()),
            default_filename: None, // two-file bundle
            // The weights first: the download saga records the first `.gguf`
            // as the model's location, and the engine reads its directory.
            files: vec![
                file(
                    OCR_MODEL_FILE,
                    self.model_remote_file,
                    self.model_bytes,
                    self.model_sha256,
                ),
                file(
                    OCR_PROJECTOR_FILE,
                    self.projector_remote_file,
                    self.projector_bytes,
                    self.projector_sha256,
                ),
            ],
            total_size_bytes,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        }
    }
}

/// Curated OCR models, as catalog entries.
pub fn get_curated_ocr_models() -> Vec<ModelMetadata> {
    CURATED_OCR_MODELS
        .iter()
        .map(CuratedOcrModel::metadata)
        .collect()
}
