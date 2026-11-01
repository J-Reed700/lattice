//! The cross-encoder search orders its shortlist with.

use std::path::{Path, PathBuf};

use crate::shared::constants::RERANKER_MODEL_DIR;

/// A reranker GGUF, served by the bundled `llama-server --reranking`.
///
/// The file is pinned to a commit, with its size and SHA-256 from the Hugging
/// Face tree API (`size` and `lfs.oid`), so a truncated or substituted
/// download fails verification instead of failing at load.
#[derive(Debug, Clone, Copy)]
pub struct CuratedReranker {
    pub name: &'static str,
    /// Hugging Face repository holding the file.
    pub repo: &'static str,
    /// The commit the file is downloaded from.
    pub revision: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

impl CuratedReranker {
    /// Where the file is downloaded from.
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    /// Where the file lives under the models folder `models_dir`.
    pub fn path_in(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(RERANKER_MODEL_DIR).join(self.file)
    }
}

/// Qwen3-Reranker-0.6B at Q8_0, the GGUF llama.cpp's maintainers converted.
/// With `--reranking` it scores a passage that answers a query near 1 and an
/// unrelated one near 0.
pub const CURATED_RERANKER: CuratedReranker = CuratedReranker {
    name: "Qwen/Qwen3-Reranker-0.6B",
    repo: "ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF",
    revision: "a02f48bb4f057028298c21fa033da2b30d7742d5",
    file: "qwen3-reranker-0.6b-q8_0.gguf",
    bytes: 639_153_184,
    sha256: "22c9979ce4fbcdc5acdc310c6641c32797eff1aa980b8f7a2db8a8ea23429a48",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reranker_is_pinned_to_a_commit() {
        assert_eq!(
            CURATED_RERANKER.url(),
            "https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF/resolve/\
             a02f48bb4f057028298c21fa033da2b30d7742d5/qwen3-reranker-0.6b-q8_0.gguf"
        );
        assert_eq!(CURATED_RERANKER.sha256.len(), 64);
        assert_eq!(
            CURATED_RERANKER.path_in(Path::new("/models")),
            Path::new("/models")
                .join(RERANKER_MODEL_DIR)
                .join("qwen3-reranker-0.6b-q8_0.gguf")
        );
    }
}
