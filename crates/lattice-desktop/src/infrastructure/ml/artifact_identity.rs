//! Computing and establishing an embedding model's artifact identity.
//!
//! The identity is the SHA-256 of the model's one GGUF file: the weights, the
//! quantisation and the pooling mode are all bytes of it, and llama-server's
//! normalisation is pinned. `tokenizer.json` is left out on purpose: it
//! decides where chunks are cut, not what a stored chunk's vector is. It is
//! computed when a model is activated for embedding and stored on its row;
//! launch and model load read the stored value and never hash model files.
//! It lives with the downloaded-model records because it describes the files
//! on disk, and the download saga establishes it when it auto-activates one.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

/// Stream the GGUF at `file` through SHA-256. The result is the same digest
/// `shasum -a 256` prints and Hugging Face shows for the file. Blocking; call
/// through `compute_in_background` from async code.
pub fn compute(file: &Path) -> Result<ArtifactIdentity> {
    let mut hash = Sha256::new();
    fold_file_contents(&mut hash, file)?;
    Ok(ArtifactIdentity::from_digest(&hash.finalize().into()))
}

/// `spawn_blocking` wrapper around `compute`.
pub async fn compute_in_background(file: PathBuf) -> Result<ArtifactIdentity> {
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let identity = compute(&file)?;
        tracing::info!(
            file = %file.display(),
            identity = %identity,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Computed embedding artifact identity"
        );
        Ok(identity)
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Artifact identity task failed: {e}")))?
}

/// The identity a model must carry to be activated for embedding.
/// Remote models carry none. Local models reuse a stored identity or hash
/// their GGUF now, off the runtime thread. A model stored as a directory is
/// not a GGUF and cannot be served.
pub async fn establish(model: &DownloadedModel) -> Result<Option<ArtifactIdentity>> {
    let file = match model.location() {
        ModelLocation::LocalFile { path } => path.clone(),
        ModelLocation::LocalDirectory { path } => {
            return Err(AppError::InvalidConfig(format!(
                "Embedding model {} is a directory ({}); embeddings are served from one GGUF file",
                model.model_id(),
                path.display()
            )))
        }
        ModelLocation::RemoteOllama => return Ok(None),
    };
    if let Some(identity) = model.embedding_artifact_identity() {
        return Ok(Some(identity.clone()));
    }
    compute_in_background(file).await.map(Some)
}

/// Stream a file's bytes into `hash` so a multi-gigabyte weights file never
/// lands in memory whole.
fn fold_file_contents(hash: &mut Sha256, path: &Path) -> Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| AppError::InvalidConfig(e.to_string()))?;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(
            buffer
                .get(..n)
                .ok_or_else(|| AppError::InvalidState("Invalid model read length".into()))?,
        );
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn the_identity_is_the_files_own_sha256() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("model-f16.gguf");
        std::fs::write(&file, b"weights").unwrap();
        // `printf weights | shasum -a 256`
        assert_eq!(
            compute(&file).unwrap().as_str(),
            "sha256:9a129038d9a00aed0cf6a7ea059ca50a813449061ab87848cf1a13eafdf33b2c"
        );
    }

    #[test]
    fn a_sibling_tokenizer_is_not_part_of_the_identity() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("model-f16.gguf");
        std::fs::write(&file, b"weights").unwrap();
        let before = compute(&file).unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();
        assert_eq!(before, compute(&file).unwrap());
        std::fs::write(&file, b"other weights").unwrap();
        assert_ne!(before, compute(&file).unwrap());
    }
}
