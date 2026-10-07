//! # Index Directory Use Case
//!
//! Recursively indexes all supported files in a directory.
//!
//! This use case:
//! 1. Discovers files in directory (optionally recursive)
//! 2. Filters by supported file extensions
//! 3. Indexes each file using IndexFileUseCase
//! 4. Collects statistics and errors
//! 5. Tracks progress and supports cancellation
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::indexing::index_directory::IndexDirectoryUseCase;
//! use lattice::application::dtos::indexing_dto::{IndexDirectoryRequestDto, ChunkingStrategyDto};
//!
//! # async fn example(use_case: IndexDirectoryUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = IndexDirectoryRequestDto {
//!     path: "/path/to/docs".to_string(),
//!     recursive: true,
//!     chunking_strategy: ChunkingStrategyDto::FixedSize { size: 512 },
//!     include_extensions: Some(vec!["txt".to_string(), "md".to_string()]),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Indexed {} files, {} failed", response.files_indexed, response.files_failed);
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::features::indexing::dto::{
    IndexDirectoryRequestDto, IndexDirectoryResponseDto, IndexFileRequestDto,
};
use crate::features::indexing::engine::IndexingState;
use crate::features::indexing::use_cases::index_file::IndexFileUseCase;
use crate::shared::error::{AppError, Result};

/// Index directory use case.
///
/// Batch indexes all supported files in a directory tree.
///
/// ## Dependencies
///
/// - `IndexFileUseCase`: Handles individual file indexing
/// - `IndexingState`: Tracks progress and cancellation
pub struct IndexDirectoryUseCase {
    index_file_use_case: Arc<IndexFileUseCase>,
    indexing_state: Arc<IndexingState>,
}

impl IndexDirectoryUseCase {
    /// Create a new index directory use case.
    ///
    /// # Arguments
    ///
    /// * `index_file_use_case` - Use case for indexing individual files
    /// * `indexing_state` - Shared state for progress tracking
    pub fn new(
        index_file_use_case: Arc<IndexFileUseCase>,
        indexing_state: Arc<IndexingState>,
    ) -> Self {
        Self {
            index_file_use_case,
            indexing_state,
        }
    }

    /// Execute directory indexing.
    ///
    /// # Arguments
    ///
    /// * `request` - Directory indexing request
    ///
    /// # Returns
    ///
    /// Response with statistics about indexed files
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Directory path is invalid
    /// - Directory cannot be read
    /// - Permissions are insufficient
    ///
    /// Note: Individual file failures are collected in the response,
    /// not returned as errors (allows partial success).
    pub async fn execute(
        &self,
        request: IndexDirectoryRequestDto,
    ) -> Result<IndexDirectoryResponseDto> {
        let directory_path = PathBuf::from(&request.path);

        // 0. Initialize state
        self.indexing_state.reset();
        self.indexing_state.start_scanning();

        // 1. Discover files. A big tree is a lot of blocking stat calls; keep
        // them off the async workers that chat and search share.
        let recursive = request.recursive;
        let include_extensions = request.include_extensions.clone();
        let files = tokio::task::spawn_blocking(move || {
            discover_files(&directory_path, recursive, include_extensions.as_ref())
        })
        .await
        .map_err(|error| AppError::Other(format!("Directory scan task failed: {error}")))??;

        self.indexing_state.set_total_files(files.len());

        // 2. Index each file
        let mut files_indexed = 0;
        let mut files_failed = 0;
        let mut total_chunks = 0;
        let mut document_ids = Vec::new();
        let mut errors = Vec::new();

        let mut cancelled = false;

        for file_path in files {
            if self.indexing_state.is_cancelled() {
                cancelled = true;
                break;
            }

            // Honour Pause. Without this the Pause button moves the actor queue
            // and leaves the run the user is watching untouched — a control that
            // changes nothing.
            self.indexing_state.wait_while_paused().await;
            if self.indexing_state.is_cancelled() {
                cancelled = true;
                break;
            }

            let path_str = file_path.to_string_lossy().to_string();
            self.indexing_state.set_current_file(path_str.clone());

            let file_request = IndexFileRequestDto {
                path: path_str.clone(),
                chunking_strategy: request.chunking_strategy.clone(),
                tags: None,
                metadata: None,
                space_id: None,
            };

            match self.index_file_use_case.execute(file_request).await {
                Ok(response) => {
                    files_indexed += 1;
                    total_chunks += response.chunks_created;
                    document_ids.push(response.document_id);
                    self.indexing_state.file_processed();
                }
                Err(e) => {
                    files_failed += 1;
                    errors.push(format!("{}: {}", file_path.display(), e));
                    // Keep the path and the reason, not just the count — the
                    // failure list in the UI is built from these.
                    self.indexing_state
                        .record_failure(&file_path, e.to_string());
                    self.indexing_state.file_failed();
                    // We don't abort on file error, just record it and continue.
                }
            }
        }

        // 3. Mark the terminal state.
        // `complete()` forces `status: Complete, percentage: 100`, so calling
        // it unconditionally reported a cancelled, partial index to the user
        // as "Complete — 100%". `IndexStatus::Cancelled` already exists; a
        // cancelled run must land there instead.
        if cancelled {
            tracing::info!(
                files_indexed,
                files_failed,
                "directory indexing cancelled by user; reporting partial progress"
            );
            self.indexing_state.cancel();
        } else {
            self.indexing_state.complete();
        }

        // 4. Build response
        Ok(IndexDirectoryResponseDto {
            files_indexed,
            files_failed,
            total_chunks,
            document_ids,
            errors,
        })
    }
}

/// Folders an import never descends into: hidden folders (`.git`, `.Trash`,
/// editor state) and dependency or cache trees that hold thousands of files
/// nobody means to put in their library.
const SKIPPED_FOLDERS: &[&str] = &["node_modules", "__pycache__"];

fn is_skipped_name(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_FOLDERS.contains(&name)
}

/// Discover the files to index under `path`.
///
/// Symlinks are not followed, so a link back to a parent folder cannot loop
/// the walk. A folder that cannot be read is logged and skipped rather than
/// aborting the import before anything was indexed.
fn discover_files(
    path: &Path,
    recursive: bool,
    include_extensions: Option<&Vec<String>>,
) -> Result<Vec<PathBuf>> {
    // repository-barrier-allow: indexing walks the user-provided directory resource.
    if !path.is_dir() {
        return Err(AppError::NotFound(format!(
            "Directory not found: {}",
            path.display()
        )));
    }

    let walker = walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_depth(if recursive { usize::MAX } else { 1 })
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || !entry.file_name().to_str().is_some_and(is_skipped_name)
        });

    let mut files = Vec::new();
    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(
                    path = ?error.path(),
                    %error,
                    "Skipping a folder the import cannot read"
                );
                continue;
            }
        };
        // repository-barrier-allow: enumerating user-selected import resources, not persisted state.
        if !entry.file_type().is_file() {
            continue;
        }
        let keep = match include_extensions {
            Some(extensions) => entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| extensions.iter().any(|allowed| allowed == ext)),
            None => true,
        };
        if keep {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn names(files: &[PathBuf], root: &Path) -> Vec<PathBuf> {
        let mut names: Vec<_> = files
            .iter()
            .map(|f| f.strip_prefix(root).unwrap().to_path_buf())
            .collect();
        names.sort();
        names
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_subfolder_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("keep.txt"), "a").unwrap();
        let locked = root.join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("hidden.txt"), "b").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = discover_files(root, true, None);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            names(&result.unwrap(), root),
            vec![PathBuf::from("keep.txt")]
        );
    }

    #[test]
    fn a_symlink_to_a_parent_folder_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let sub = root.join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("note.md"), "a").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root, sub.join("loop")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(root, sub.join("loop")).unwrap();

        let files = discover_files(root, true, None).unwrap();

        assert_eq!(
            names(&files, root),
            vec![PathBuf::from("sub").join("note.md")]
        );
    }

    #[test]
    fn hidden_and_dependency_folders_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for folder in [".git", "node_modules", "docs"] {
            std::fs::create_dir(root.join(folder)).unwrap();
            std::fs::write(root.join(folder).join("file.md"), "a").unwrap();
        }
        std::fs::write(root.join(".DS_Store"), "x").unwrap();

        let files = discover_files(root, true, None).unwrap();

        assert_eq!(
            names(&files, root),
            vec![PathBuf::from("docs").join("file.md")]
        );
    }
}
