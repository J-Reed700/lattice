//! The indexing feature's implementation of the vault writer's corpus sink.
//!
//! Mirrored journal pages reach the corpus through [`NoteIndexSink`]
//! (defined in `application::ports` so the vault feature never depends on
//! indexing). This adapter is the port's one real implementation. It runs
//! the same single-file path the `index_file` command does — path
//! validation, embedding-model readiness, settings-driven chunking, the
//! shared progress state and the ENTRY/EXIT logs — and removes a note
//! through the same path a user-initiated delete takes, ownership-aware
//! [`DeleteDocumentUseCase`] included, so a mirrored file is never deleted
//! from disk.
//!
//! Every failure is returned, never swallowed: the vault writer keeps the
//! durable outbox entry deferred and retries on its next pass.

use std::sync::Arc;

use async_trait::async_trait;

use crate::features::indexing::commands::{
    chunk_tokens_for, delete_indexed_document, index_single_file, SingleFileIndexing,
    DEFAULT_CHUNK_TOKENS,
};
use crate::features::indexing::dto::{
    ChunkingStrategyDto, IndexFileRequestDto, IndexFileResponseDto,
};
use crate::features::indexing::engine::IndexingState;
use crate::features::indexing::use_cases::{DeleteDocumentUseCase, IndexFileUseCase};
use crate::infrastructure::security::FileAccessConfig;
use lattice_core::application::ports::{DocumentRepository, NoteIndexSink};
use lattice_core::shared::error::Result;

/// The embedding model must be loaded and ready before a file is indexed.
///
/// Deliberately a one-method port rather than a `Container` or an
/// `EmbeddingPort`: the index path only needs the readiness gate. The
/// command path implements it over the container; for this sink the
/// composition root implements it over the embedding runtime, which owns
/// single-flight loading and the retry cooldown. Both apply
/// `commands::require_embedding_ready`, so the check and its messages are
/// one.
#[async_trait]
pub trait EmbeddingReadiness: Send + Sync {
    /// Load the active model if needed and verify it is ready.
    async fn ensure_ready(&self) -> Result<()>;
}

/// The one settings value the sink reads: the user's chunk-size
/// preference, before the shared chunk-size policy is applied. Defined
/// here so indexing never names the settings feature; the composition root
/// implements it over the settings use case.
#[async_trait]
pub trait ChunkSizePreference: Send + Sync {
    /// The user's chunk-size setting, unclamped.
    async fn chunk_size(&self) -> Result<u32>;
}

/// The half of the indexing pipeline the single-file path drives: one file
/// in, document row and vectors out. Implemented by [`IndexFileUseCase`] so
/// the sink runs the exact pipeline a user-initiated index runs, and by the
/// recording fakes in the tests below.
#[async_trait]
pub trait NoteIndexPipeline: Send + Sync {
    /// Run the full index pipeline for one file.
    async fn submit(&self, request: IndexFileRequestDto) -> Result<IndexFileResponseDto>;
}

#[async_trait]
impl NoteIndexPipeline for IndexFileUseCase {
    async fn submit(&self, request: IndexFileRequestDto) -> Result<IndexFileResponseDto> {
        self.execute(request).await
    }
}

/// The ownership-aware half: drops a document row and its vectors, and
/// the file on disk only when Lattice owns it. Implemented by
/// [`DeleteDocumentUseCase`] so a mirrored note is removed exactly the
/// way a user-initiated delete removes anything.
#[async_trait]
pub trait NoteRemovalPipeline: Send + Sync {
    /// Delete the document with this id.
    async fn remove(&self, document_id: String) -> Result<()>;
}

#[async_trait]
impl NoteRemovalPipeline for DeleteDocumentUseCase {
    async fn remove(&self, document_id: String) -> Result<()> {
        self.execute(document_id).await.map(|_| ())
    }
}

/// Persistence mapping between mirrored vault note ids (`<note_id>.md`) and
/// their current indexed `documents.id`. Defined here so indexing never names
/// the vault feature; the composition root binds it over `vault::repository`.
#[async_trait]
pub trait VaultNoteDocumentRepo: Send + Sync {
    async fn find_document_id(&self, note_id: &str) -> Result<Option<String>>;
    async fn upsert_document_id(&self, note_id: &str, document_id: &str) -> Result<()>;
    async fn delete_mapping(&self, note_id: &str) -> Result<()>;
}

/// One sink per process: the vault worker calls it inline, so a note is
/// never indexed twice concurrently and no queue sits between the mirror
/// write and the corpus.
pub struct NoteIndexSinkAdapter {
    index_file: Arc<dyn NoteIndexPipeline>,
    delete_document: Arc<dyn NoteRemovalPipeline>,
    document_repo: Arc<dyn DocumentRepository>,
    note_documents: Arc<dyn VaultNoteDocumentRepo>,
    embedding: Arc<dyn EmbeddingReadiness>,
    chunk_size: Arc<dyn ChunkSizePreference>,
    file_access: Arc<FileAccessConfig>,
    indexing_state: Arc<IndexingState>,
}

impl NoteIndexSinkAdapter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index_file: Arc<dyn NoteIndexPipeline>,
        delete_document: Arc<dyn NoteRemovalPipeline>,
        document_repo: Arc<dyn DocumentRepository>,
        note_documents: Arc<dyn VaultNoteDocumentRepo>,
        embedding: Arc<dyn EmbeddingReadiness>,
        chunk_size: Arc<dyn ChunkSizePreference>,
        file_access: Arc<FileAccessConfig>,
        indexing_state: Arc<IndexingState>,
    ) -> Self {
        Self {
            index_file,
            delete_document,
            document_repo,
            note_documents,
            embedding,
            chunk_size,
            file_access,
            indexing_state,
        }
    }
}

fn note_id_from_path(path: &str) -> Option<&str> {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
}

#[async_trait]
impl NoteIndexSink for NoteIndexSinkAdapter {
    async fn index(&self, path: &str) -> Result<()> {
        let note_id = note_id_from_path(path);
        let old_document_id = if let Some(note_id) = note_id {
            self.note_documents.find_document_id(note_id).await?
        } else {
            None
        };

        let chunk_tokens = match self.chunk_size.chunk_size().await {
            Ok(chunk_size) => chunk_tokens_for(chunk_size),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "vault note indexing: could not read settings; using default chunk size"
                );
                DEFAULT_CHUNK_TOKENS
            }
        };

        let request = IndexFileRequestDto {
            path: path.to_string(),
            chunking_strategy: ChunkingStrategyDto::Semantic {
                max_tokens: chunk_tokens,
            },
            tags: None,
            metadata: None,
            space_id: None,
        };
        let indexing = SingleFileIndexing {
            file_access: &self.file_access,
            embedding: self.embedding.as_ref(),
            state: &self.indexing_state,
            pipeline: self.index_file.as_ref(),
        };
        // The vault root is an allowed root once settings are readable; if
        // it is not (mid-boot, stale config), path validation fails and the
        // entry is deferred instead of indexing a path the security layer
        // would reject anyway.
        let response = index_single_file(&indexing, None, request)
            .await
            .map_err(|failure| failure.error)?;

        if let Some(old_id) = old_document_id {
            if old_id != response.document_id {
                delete_indexed_document(self.delete_document.as_ref(), old_id).await?;
            }
        }

        if let Some(note_id) = note_id {
            if !response.document_id.is_empty() {
                self.note_documents
                    .upsert_document_id(note_id, &response.document_id)
                    .await?;
            }
        }

        Ok(())
    }

    async fn remove(&self, path: &str) -> Result<()> {
        let note_id = note_id_from_path(path);
        let mapped_id = if let Some(note_id) = note_id {
            self.note_documents.find_document_id(note_id).await?
        } else {
            None
        };
        // Idempotent by contract: a note that was never indexed is already
        // gone, and the outbox entry can settle.
        let document_id = match mapped_id {
            Some(id) => Some(id),
            None => self.document_repo.find_id_by_path(path).await?,
        };
        let Some(document_id) = document_id else {
            tracing::debug!(target = %path, "vault note removal: not indexed — nothing to do");
            return Ok(());
        };

        delete_indexed_document(self.delete_document.as_ref(), document_id).await?;
        if let Some(note_id) = note_id {
            self.note_documents.delete_mapping(note_id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "note_index_sink_tests.rs"]
mod tests;
