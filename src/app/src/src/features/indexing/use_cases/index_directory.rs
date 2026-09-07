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

use std::path::PathBuf;
use std::sync::Arc;

use crate::features::indexing::dto::{
    IndexDirectoryRequestDto, IndexDirectoryResponseDto, IndexFileRequestDto,
};
use crate::features::indexing::use_cases::index_file::IndexFileUseCase;
use crate::infrastructure::indexing::IndexingState;
use crate::shared::error::Result;

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

        // 1. Discover files
        let files = self.discover_files(
            &directory_path,
            request.recursive,
            request.include_extensions.as_ref(),
        )?;

        // Update progress with total count
        self.indexing_state.set_total_files(files.len());

        // 2. Index each file
        let mut files_indexed = 0;
        let mut files_failed = 0;
        let mut total_chunks = 0;
        let mut document_ids = Vec::new();
        let mut errors = Vec::new();

        let mut cancelled = false;

        for file_path in files {
            // Check for cancellation
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
                    self.indexing_state.record_failure(&file_path, e.to_string());
                    self.indexing_state.file_failed();
                    // We don't abort on file error, just record it and continue.
                }
            }
        }

        // 3. Mark the terminal state.
        //
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

    /// Discover files in directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory to search
    /// * `recursive` - Whether to search subdirectories
    /// * `include_extensions` - Optional filter for file extensions
    ///
    /// # Returns
    ///
    /// List of file paths to index
    fn discover_files(
        &self,
        path: &PathBuf,
        recursive: bool,
        include_extensions: Option<&Vec<String>>,
    ) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // repository-barrier-allow: indexing walks the user-provided directory resource.
        if !path.is_dir() {
            return Err(crate::error::AppError::NotFound(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        self.scan_directory(path, recursive, include_extensions, &mut files)?;

        Ok(files)
    }

    /// Recursively scan directory for files.
    #[allow(clippy::only_used_in_recursion)]
    fn scan_directory(
        &self,
        path: &PathBuf,
        recursive: bool,
        include_extensions: Option<&Vec<String>>,
        files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        // repository-barrier-allow: walking user-provided ingestion directory to discover files for indexing.
        let entries = std::fs::read_dir(path)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // repository-barrier-allow: classify entries in the user-provided ingestion tree.
            if path.is_file() {
                // Check if file extension matches filter
                if let Some(extensions) = include_extensions {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if extensions.contains(&ext.to_string()) {
                            files.push(path);
                        }
                    }
                } else {
                    // No filter, include all files
                    files.push(path);
                }
            // repository-barrier-allow: recurse only into directories in that ingestion tree.
            } else if path.is_dir() && recursive {
                self.scan_directory(&path, recursive, include_extensions, files)?;
            }
        }

        Ok(())
    }
}
