//! Computing and establishing an embedding model's artifact identity.
//!
//! The identity is a SHA-256 over the model's preprocessing and weights. It is
//! computed when a model is activated for embedding and stored on its row;
//! launch and model load read the stored value and never hash model files.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::domain::downloaded_model::DownloadedModel;
use crate::domain::value_objects::ArtifactIdentity;
use crate::features::embedding::candle_service::{WEIGHTS_PYTORCH_BIN, WEIGHTS_SAFETENSORS};
use crate::shared::error::{AppError, Result};

/// Stream the artifacts under `dir` through SHA-256. The input is the
/// content-addressed model plus its preprocessing, not merely its output
/// dimension. Blocking; call through `compute_in_background` from async code.
pub fn compute(dir: &Path) -> Result<ArtifactIdentity> {
    let mut hash = Sha256::new();
    hash.update(b"lattice-embedding-input-v2");
    for name in [
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "sentence_bert_config.json",
        "1_Pooling/config.json",
        WEIGHTS_SAFETENSORS,
    ] {
        hash.update(name.as_bytes());
        let path = dir.join(name);
        if !path.exists() {
            hash.update(b"absent");
            continue;
        }
        fold_file_contents(&mut hash, &path)?;
    }
    // The pickle is folded in only when it is actually on disk. Appending it
    // to the list above would have hashed an `absent` marker for every
    // safetensors model and changed every identity already written into a
    // vault's index filenames; contributing nothing when the file is missing
    // keeps those byte-for-byte while still covering the weights of a
    // checkpoint that ships only `pytorch_model.bin`.
    let pickle = dir.join(WEIGHTS_PYTORCH_BIN);
    if pickle.exists() {
        hash.update(WEIGHTS_PYTORCH_BIN.as_bytes());
        fold_file_contents(&mut hash, &pickle)?;
    }
    Ok(ArtifactIdentity::from_digest(&hash.finalize().into()))
}

/// `spawn_blocking` wrapper around `compute`.
pub async fn compute_in_background(dir: PathBuf) -> Result<ArtifactIdentity> {
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let identity = compute(&dir)?;
        tracing::info!(
            dir = %dir.display(),
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
/// Remote models carry none. Local models reuse a stored identity or compute
/// one now, off the runtime thread.
pub async fn establish(model: &DownloadedModel) -> Result<Option<ArtifactIdentity>> {
    let Some(dir) = model.location().enclosing_dir() else {
        return Ok(None);
    };
    if let Some(identity) = model.embedding_artifact_identity() {
        return Ok(Some(identity.clone()));
    }
    compute_in_background(dir).await.map(Some)
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
    fn compute_for_safetensors_models_is_unchanged() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.json"), b"{}").unwrap();
        std::fs::write(dir.path().join(WEIGHTS_SAFETENSORS), b"weights").unwrap();
        assert_eq!(
            compute(dir.path()).unwrap().as_str(),
            "sha256:ca19a491bee769f6c8e7a8a9bc8d1e3a6846116a4e1915e5e999a999cdc7d3e3"
        );
    }

    #[test]
    fn compute_covers_the_pickle_when_it_is_the_only_weights_file() {
        let safetensors_dir = TempDir::new().unwrap();
        std::fs::write(safetensors_dir.path().join("config.json"), b"{}").unwrap();
        std::fs::write(safetensors_dir.path().join(WEIGHTS_SAFETENSORS), b"weights").unwrap();

        let pickle_dir = TempDir::new().unwrap();
        std::fs::write(pickle_dir.path().join("config.json"), b"{}").unwrap();
        std::fs::write(pickle_dir.path().join(WEIGHTS_PYTORCH_BIN), b"weights").unwrap();

        let pickle_identity = compute(pickle_dir.path()).unwrap();
        assert_ne!(
            pickle_identity,
            compute(safetensors_dir.path()).unwrap(),
            "a pickle checkpoint is a different artifact set than a safetensors one"
        );

        // And the pickle's bytes are actually hashed, not just its name.
        std::fs::write(pickle_dir.path().join(WEIGHTS_PYTORCH_BIN), b"other").unwrap();
        assert_ne!(pickle_identity, compute(pickle_dir.path()).unwrap());
    }
}
