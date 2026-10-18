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
/// publishes. Downloading fetches it when the repository has one, and the
/// Candle runtime loads it.
pub const SPARSE_HEAD_PT: &str = "sparse_linear.pt";

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
