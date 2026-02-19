//! File System Event Handlers
//!
//! Handlers for file system watcher events (create, modify, delete).

use crate::application::dtos::indexing_dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::application::ports::DocumentRepositoryPort;
use crate::application::use_cases::indexing::{
    DeleteDocumentUseCase, IndexFileUseCase, ReindexDocumentUseCase,
};
use crate::shared::error::Result;
use notify::{Event, EventKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn};

const DEFAULT_WATCHER_CHUNK_TOKENS: usize = 800;

/// File system event handler
pub struct FileSystemEventHandler {
    index_file_use_case: Arc<IndexFileUseCase>,
    reindex_use_case: Arc<ReindexDocumentUseCase>,
    document_repository: Arc<dyn DocumentRepositoryPort>,
    delete_use_case: Arc<DeleteDocumentUseCase>,
    watched_directories: Vec<PathBuf>,
}

impl FileSystemEventHandler {
    pub fn new(
        index_file_use_case: Arc<IndexFileUseCase>,
        reindex_use_case: Arc<ReindexDocumentUseCase>,
        document_repository: Arc<dyn DocumentRepositoryPort>,
        delete_use_case: Arc<DeleteDocumentUseCase>,
        watched_directories: Vec<PathBuf>,
    ) -> Self {
        Self {
            index_file_use_case,
            reindex_use_case,
            document_repository,
            delete_use_case,
            watched_directories,
        }
    }

    /// Handle a file system event
    pub async fn handle_event(&self, event: Event) -> Result<()> {
        match event.kind {
            EventKind::Create(_) => {
                for path in &event.paths {
                    if self.should_index(path) {
                        info!("New file detected: {:?}", path);
                        self.handle_create(path).await?;
                    }
                }
            }
            EventKind::Modify(_) => {
                for path in &event.paths {
                    if self.should_index(path) {
                        info!("File modified: {:?}", path);
                        self.handle_modify(path).await?;
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    info!("File removed: {:?}", path);
                    self.handle_remove(path).await?;
                }
            }
            _ => {
                // Ignore other events (access, etc.)
            }
        }

        Ok(())
    }

    /// Handle file creation
    async fn handle_create(&self, path: &PathBuf) -> Result<()> {
        let request = IndexFileRequestDto {
            path: path.to_string_lossy().to_string(),
            chunking_strategy: ChunkingStrategyDto::Semantic {
                max_tokens: DEFAULT_WATCHER_CHUNK_TOKENS,
            },
            tags: None,
            metadata: None,
            space_id: None,
        };

        match self.index_file_use_case.execute(request).await {
            Ok(response) => {
                info!(
                    "Indexed new file: {} ({})",
                    path.display(),
                    response.document_id
                );
                Ok(())
            }
            Err(e) => {
                error!("Failed to index new file {:?}: {}", path, e);
                Err(e)
            }
        }
    }

    /// Handle file modification
    async fn handle_modify(&self, path: &PathBuf) -> Result<()> {
        // Check if document exists, if so reindex, otherwise index as new
        let path_str = path.to_string_lossy().to_string();

        match self.document_repository.find_id_by_path(&path_str).await? {
            Some(document_id) => {
                info!("Reindexing modified file: {:?} (id={})", path, document_id);
                match self.reindex_use_case.execute(document_id).await {
                    Ok(response) => {
                        info!(
                            "Reindexed file: {} ({})",
                            path.display(),
                            response.document_id
                        );
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to reindex file {:?}: {}", path, e);
                        Err(e)
                    }
                }
            }
            None => {
                let request = IndexFileRequestDto {
                    path: path_str,
                    chunking_strategy: ChunkingStrategyDto::Semantic {
                        max_tokens: DEFAULT_WATCHER_CHUNK_TOKENS,
                    },
                    tags: None,
                    metadata: None,
                    space_id: None,
                };

                match self.index_file_use_case.execute(request).await {
                    Ok(response) => {
                        info!(
                            "Indexed modified file as new: {} ({})",
                            path.display(),
                            response.document_id
                        );
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to index modified file {:?}: {}", path, e);
                        Err(e)
                    }
                }
            }
        }
    }

    /// Handle file removal
    async fn handle_remove(&self, path: &PathBuf) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();

        match self.document_repository.find_id_by_path(&path_str).await? {
            Some(document_id) => {
                info!("Deleting removed file: {:?} (id={})", path, document_id);
                match self.delete_use_case.execute(document_id).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        error!("Failed to delete removed file {:?}: {}", path, e);
                        Err(e)
                    }
                }
            }
            None => {
                warn!("File removed but no document found: {:?}", path);
                Ok(())
            }
        }
    }

    /// Check if file should be indexed
    fn should_index(&self, path: &Path) -> bool {
        // Check if path is in watched directories
        if !self
            .watched_directories
            .iter()
            .any(|dir| path.starts_with(dir))
        {
            return false;
        }

        // Check file extension
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            matches!(
                ext_str.as_str(),
                "txt" | "md" | "pdf" | "docx" | "doc" | "html" | "json" | "xml" | "csv"
            )
        } else {
            false
        }
    }
}

/// Start file system watcher
pub async fn start_file_watcher(handler: Arc<FileSystemEventHandler>) -> Result<()> {
    use notify::{recommended_watcher, RecursiveMode, Watcher};
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel(100);

    // Create watcher
    let mut watcher = recommended_watcher(move |res: Result<Event, notify::Error>| match res {
        Ok(event) => {
            let _ = tx.blocking_send(event);
        }
        Err(e) => {
            error!("File watcher error: {}", e);
        }
    })
    .map_err(|e| crate::shared::error::AppError::InternalError(e.to_string()))?;

    // Watch directories
    for dir in &handler.watched_directories {
        info!("Watching directory: {:?}", dir);
        watcher
            .watch(dir, RecursiveMode::Recursive)
            .map_err(|e| crate::shared::error::AppError::InternalError(e.to_string()))?;
    }

    // Event processing loop
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = handler.handle_event(event).await {
                error!("Failed to handle file system event: {}", e);
            }
        }
    });

    // Keep watcher alive
    std::mem::forget(watcher);

    Ok(())
}
