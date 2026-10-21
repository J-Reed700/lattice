//! Files a safetensors-era (Candle) embedding download left in its folder.
//!
//! The llama.cpp runtime reads a GGUF and its `tokenizer.json`; the folders
//! the Candle runtime filled still hold its weights and configs (about 1.2 GB
//! for Qwen3). Nothing removes them on its own: Settings lists them with their
//! size and removes exactly them when the owner asks.
//!
//! Only the names in `SAFETENSORS_ERA_EMBEDDING_FILES`, only inside the
//! curated embedding folders, and never a path a `models` row points at or
//! sits under.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lattice_core::domain::models::checkpoint::{
    SAFETENSORS_ERA_EMBEDDING_DIRS, SAFETENSORS_ERA_EMBEDDING_FILES,
};
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::{AppError, Result};

use crate::infrastructure::adapters::fs::model_paths::ModelPaths;

/// Leftover files and folders, and their size on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeftoverEmbeddingFiles {
    pub paths: Vec<PathBuf>,
    pub total_bytes: u64,
}

pub struct LeftoverEmbeddingFilesUseCase {
    repository: Arc<dyn DownloadedModelRepository>,
}

impl LeftoverEmbeddingFilesUseCase {
    pub fn new(repository: Arc<dyn DownloadedModelRepository>) -> Self {
        Self { repository }
    }

    /// The leftovers on disk now.
    pub async fn list(&self) -> Result<LeftoverEmbeddingFiles> {
        let (dirs, protected) = self.scope().await?;
        blocking(move || find_leftovers(&dirs, &protected)).await
    }

    /// Remove the leftovers found now, never a list a caller supplies, and
    /// return what was removed.
    pub async fn remove(&self) -> Result<LeftoverEmbeddingFiles> {
        let (dirs, protected) = self.scope().await?;
        blocking(move || {
            let found = find_leftovers(&dirs, &protected)?;
            for path in &found.paths {
                remove_path(path)?;
            }
            Ok(found)
        })
        .await
    }

    /// The curated embedding folders, and every path a `models` row names.
    async fn scope(&self) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
        let dirs = SAFETENSORS_ERA_EMBEDDING_DIRS
            .iter()
            .map(|id| ModelPaths::new(id).map(|paths| paths.unified_path().to_path_buf()))
            .collect::<Result<Vec<_>>>()?;
        let protected = self
            .repository
            .list()
            .await?
            .iter()
            .filter_map(|model| model.loadable_path().map(Path::to_path_buf))
            .collect();
        Ok((dirs, protected))
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| AppError::FileSystem(format!("Leftover file task failed: {e}")))?
}

fn find_leftovers(dirs: &[PathBuf], protected: &[PathBuf]) -> Result<LeftoverEmbeddingFiles> {
    let mut found = LeftoverEmbeddingFiles::default();
    for dir in dirs {
        for name in SAFETENSORS_ERA_EMBEDDING_FILES {
            let path = dir.join(name);
            // Not followed: a link is listed and removed as itself.
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if protected
                .iter()
                .any(|row| row.starts_with(&path) || path.starts_with(row))
            {
                continue;
            }
            found.total_bytes += size_on_disk(&path, &metadata);
            found.paths.push(path);
        }
    }
    Ok(found)
}

fn size_on_disk(path: &Path, metadata: &std::fs::Metadata) -> u64 {
    if !metadata.is_dir() {
        return metadata.len();
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let child = entry.path();
            std::fs::symlink_metadata(&child)
                .ok()
                .map(|metadata| size_on_disk(&child, &metadata))
        })
        .sum()
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(AppError::FileSystem(format!(
                "Failed to read {}: {e}",
                path.display()
            )))
        }
    };
    let removed = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    removed.map_err(|e| AppError::FileSystem(format!("Failed to remove {}: {e}", path.display())))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;

    /// A Qwen3 folder as the upgrade leaves it: the safetensors-era files, the
    /// new GGUF and tokenizer, and a stray file of the user's.
    fn upgraded_folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create model directory");
        for (name, contents) in [
            ("model.safetensors", &b"weights"[..]),
            ("config.json", &b"{}"[..]),
            ("tokenizer.json", &b"{}"[..]),
            ("Qwen3-Embedding-0.6B-f16.gguf", &b"gguf"[..]),
            ("notes.txt", &b"mine"[..]),
        ] {
            std::fs::write(dir.path().join(name), contents).expect("write file");
        }
        std::fs::create_dir(dir.path().join("1_Pooling")).expect("create pooling dir");
        std::fs::write(dir.path().join("1_Pooling/config.json"), b"{}").expect("write pooling");
        dir
    }

    #[test]
    fn lists_only_safetensors_era_files_with_their_size() {
        let dir = upgraded_folder();
        let found = find_leftovers(&[dir.path().to_path_buf()], &[]).unwrap();

        let mut names: Vec<String> = found
            .paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["1_Pooling", "config.json", "model.safetensors"]);
        assert_eq!(found.total_bytes, 7 + 2 + 2);
    }

    #[test]
    fn a_folder_a_models_row_points_at_is_left_alone() {
        let dir = upgraded_folder();
        let found =
            find_leftovers(&[dir.path().to_path_buf()], &[dir.path().to_path_buf()]).unwrap();
        assert_eq!(found, LeftoverEmbeddingFiles::default());

        let found = find_leftovers(
            &[dir.path().to_path_buf()],
            &[dir.path().join("model.safetensors")],
        )
        .unwrap();
        assert!(!found.paths.contains(&dir.path().join("model.safetensors")));
    }

    #[test]
    fn removing_keeps_the_gguf_the_tokenizer_and_the_users_files() {
        let dir = upgraded_folder();
        let found = find_leftovers(&[dir.path().to_path_buf()], &[]).unwrap();
        for path in &found.paths {
            remove_path(path).unwrap();
        }

        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "Qwen3-Embedding-0.6B-f16.gguf",
                "notes.txt",
                "tokenizer.json"
            ]
        );
        assert_eq!(
            find_leftovers(&[dir.path().to_path_buf()], &[]).unwrap(),
            LeftoverEmbeddingFiles::default()
        );
    }

    #[test]
    fn a_missing_folder_has_no_leftovers() {
        let dir = tempfile::tempdir().expect("create root");
        let found = find_leftovers(&[dir.path().join("absent")], &[]).unwrap();
        assert_eq!(found, LeftoverEmbeddingFiles::default());
    }
}
