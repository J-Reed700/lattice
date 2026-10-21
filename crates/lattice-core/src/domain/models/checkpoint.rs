//! The weights files of a Hugging Face checkpoint directory.
//!
//! Downloading records which file a directory's model lives in, the catalog
//! picks it out of a repository listing, and the Candle runtime loads it, so
//! the names live here rather than with any one of them.

use std::path::{Path, PathBuf};

/// The weights file Candle prefers: a memory-mappable safetensors archive.
pub const WEIGHTS_SAFETENSORS: &str = "model.safetensors";

/// The PyTorch pickle accepted when a repository never published a safetensors
/// conversion. `BAAI/bge-m3` is that case in the curated catalog — its only
/// root-level weights file is `pytorch_model.bin`, so requiring safetensors
/// made the model impossible to download or load at all.
pub const WEIGHTS_PYTORCH_BIN: &str = "pytorch_model.bin";

/// The PyTorch pickle of the sparse lexical head the BGE-M3 repository
/// publishes. Older downloads fetched it beside the weights.
pub const SPARSE_HEAD_PT: &str = "sparse_linear.pt";

/// The tokenizer a GGUF embedding model cuts passages with. It sits beside
/// the GGUF in the model's directory; the GGUF alone cannot be chunked.
pub const TOKENIZER_JSON: &str = "tokenizer.json";

/// Whether `model_dir` holds the tokenizer an embedding GGUF in it is
/// chunked with.
pub fn has_tokenizer(model_dir: &Path) -> bool {
    model_dir.join(TOKENIZER_JSON).is_file()
}

/// What a safetensors-era (Candle) embedding download left in its model
/// directory, files and folders alike. The llama.cpp runtime reads none of
/// them; Settings offers to remove them and never does so on its own.
/// `tokenizer.json` is deliberately absent: the GGUF runtime chunks with it.
pub const SAFETENSORS_ERA_EMBEDDING_FILES: &[&str] = &[
    WEIGHTS_SAFETENSORS,
    WEIGHTS_PYTORCH_BIN,
    "config.json",
    "tokenizer_config.json",
    "sentence_bert_config.json",
    "special_tokens_map.json",
    "vocab.txt",
    "1_Pooling",
    SPARSE_HEAD_PT,
    "onnx",
];

/// Curated catalog ids whose model directories received safetensors-era
/// embedding downloads: today's Qwen3 entry, and all-MiniLM-L6-v2, which left
/// the catalog but may still have a folder on disk.
pub const SAFETENSORS_ERA_EMBEDDING_DIRS: &[&str] = &["qwen3-embedding-0.6b", "all-minilm-l6-v2"];

/// The weights file to load out of `model_dir`, or `None` when it holds
/// neither candidate.
///
/// Safetensors wins when both are present: it mmaps instead of materializing
/// every tensor, and it is the format every existing model identity was
/// computed over.
pub fn weights_path(model_dir: &Path) -> Option<PathBuf> {
    [WEIGHTS_SAFETENSORS, WEIGHTS_PYTORCH_BIN]
        .into_iter()
        .map(|name| model_dir.join(name))
        .find(|path| path.is_file())
}

/// Whether `model_dir` holds weights the Candle runtime can load.
pub fn has_loadable_weights(model_dir: &Path) -> bool {
    weights_path(model_dir).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn weights_path_prefers_safetensors_and_accepts_the_pickle() {
        let dir = TempDir::new().unwrap();
        assert!(weights_path(dir.path()).is_none());
        assert!(!has_loadable_weights(dir.path()));

        std::fs::write(dir.path().join(WEIGHTS_PYTORCH_BIN), b"pickle").unwrap();
        assert_eq!(
            weights_path(dir.path()),
            Some(dir.path().join(WEIGHTS_PYTORCH_BIN))
        );
        assert!(has_loadable_weights(dir.path()));

        std::fs::write(dir.path().join(WEIGHTS_SAFETENSORS), b"tensors").unwrap();
        assert_eq!(
            weights_path(dir.path()),
            Some(dir.path().join(WEIGHTS_SAFETENSORS)),
            "safetensors wins when both are present"
        );
    }
}
