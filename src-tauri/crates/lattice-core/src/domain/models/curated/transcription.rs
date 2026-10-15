//! Curated transcription models.

use crate::domain::models::metadata::ModelFileMetadata;
use crate::domain::models::selection::ModelFormat;
use crate::domain::models::selection::{ModelCategory, ModelMetadata, PerformanceTier};

/// Curated on-device transcription models.
///
/// Both entries are quantized in candle's own GGUF layout — the only whisper
/// weights `quantized_model::Whisper::load` can read. Every URL here has been
/// verified to resolve and every model has been verified to transcribe; whisper
/// base and small do **not** exist in this format anywhere public, which is why
/// tiny and the distilled medium are the two rungs on offer.
///
/// Each file is downloaded under a **fixed local name** even though the remote
/// name may be size-qualified, so the engine loads one stable layout:
///
///     ~/.cache/lattice/models/<id>/{model.gguf, config.json, tokenizer.json}
///
/// `melfilters.bytes` is deliberately **not** downloaded: the 80-bin mel
/// filterbank is a constant of the architecture, identical for every 80-mel
/// model, and is bundled with the engine
/// (`features/transcription/engine/melfilters.bytes`).
///
/// `model.gguf` is the primary file per `features/download/saga.rs::select_primary_model_file`,
/// so the recorded `ModelLocation` is `LocalFile{…/model.gguf}` and
/// `DownloadedModelRepository::is_downloaded` reduces to `path.is_file()`.
/// The engine takes `location.enclosing_dir()` to find the siblings.
pub fn get_curated_transcription_models() -> Vec<ModelMetadata> {
    vec![
        ModelMetadata {
            id: "whisper-tiny-q8".into(),
            name: "Whisper Tiny".into(),
            category: ModelCategory::Transcription,
            description: "Fast on-device transcription for voice memos and meetings. \
                          Good on clear English speech; misses names and accents."
                .into(),
            size_gb: 0.042,
            minimum_ram_gb: 1.0,
            recommended_ram_gb: 2.0,
            context_length: 448,
            performance_tier: PerformanceTier::Fast,
            supported_quantizations: vec!["Q8_0".into()],
            capabilities: vec!["transcription".into(), "multilingual".into()],
            download_url: Some("https://huggingface.co/lmz/candle-whisper".into()),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("lmz/candle-whisper".into()),
            default_filename: None, // multi-file bundle
            files: vec![
                ModelFileMetadata::new(
                    "model.gguf".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/model-tiny-q80.gguf"
                        .into(),
                    0,
                ),
                ModelFileMetadata::new(
                    "config.json".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/config-tiny.json"
                        .into(),
                    0,
                ),
                ModelFileMetadata::new(
                    "tokenizer.json".into(),
                    "https://huggingface.co/lmz/candle-whisper/resolve/main/tokenizer-tiny.json"
                        .into(),
                    0,
                ),
            ],
            total_size_bytes: 41_841_632,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
        ModelMetadata {
            id: "whisper-medium-distil-q8".into(),
            name: "Whisper Medium (distilled)".into(),
            category: ModelCategory::Transcription,
            description: "Higher-accuracy on-device transcription. Much better with names, \
                          accents and crosstalk; needs about 430 MB of disk."
                .into(),
            size_gb: 0.43,
            minimum_ram_gb: 2.0,
            recommended_ram_gb: 4.0,
            context_length: 448,
            performance_tier: PerformanceTier::Balanced,
            supported_quantizations: vec!["Q8_0".into()],
            capabilities: vec!["transcription".into(), "multilingual".into()],
            download_url: Some(
                "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil".into(),
            ),
            license: "MIT".into(),
            requires_auth: false,
            model_id: Some("Demonthos/candle-quantized-whisper-medium-distil".into()),
            default_filename: None,
            files: vec![
                ModelFileMetadata::new(
                    "model.gguf".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/model.gguf"
                        .into(),
                    0,
                ),
                ModelFileMetadata::new(
                    "config.json".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/config.json"
                        .into(),
                    0,
                ),
                ModelFileMetadata::new(
                    "tokenizer.json".into(),
                    "https://huggingface.co/Demonthos/candle-quantized-whisper-medium-distil/resolve/main/tokenizer.json"
                        .into(),
                    0,
                ),
            ],
            total_size_bytes: 430_005_504,
            embedding_dimensions: None,
            embedding_compatibility: None,
            format: ModelFormat::Gguf,
        },
    ]
}
