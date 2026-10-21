//! The files of a model directory that more than one feature names.
//!
//! Downloading records which file a directory's model lives in, the catalog
//! picks it out of a repository listing, and the runtimes load it, so the
//! names live here rather than with any one of them.

use std::path::Path;

/// The weights file of a safetensors checkpoint directory (the reranker's).
pub const WEIGHTS_SAFETENSORS: &str = "model.safetensors";

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
    "pytorch_model.bin",
    "config.json",
    "tokenizer_config.json",
    "sentence_bert_config.json",
    "special_tokens_map.json",
    "vocab.txt",
    "1_Pooling",
    "sparse_linear.pt",
    "onnx",
];

/// Curated catalog ids whose model directories received safetensors-era
/// embedding downloads: today's Qwen3 entry, and all-MiniLM-L6-v2, which left
/// the catalog but may still have a folder on disk.
pub const SAFETENSORS_ERA_EMBEDDING_DIRS: &[&str] = &["qwen3-embedding-0.6b", "all-minilm-l6-v2"];
