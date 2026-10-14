//! The indexing feature's implementation of the vault writer's corpus sink.
//!
//! Mirrored journal pages reach the corpus through [`NoteIndexSink`]
//! (defined in `application::ports` so the vault feature never depends on
//! indexing). This adapter is the port's one real implementation. It runs
//! the same single-file path the `index_file` command does — path
//! validation, embedding-model readiness, settings-driven chunking, the
//! shared progress state, the `FileIndexed` audit event and the ENTRY/EXIT
//! logs — and removes a note through the same path a user-initiated delete
//! takes, ownership-aware [`DeleteDocumentUseCase`] and `FileDeleted` audit
//! event included, so a mirrored file is never deleted from disk.
//!
//! Every failure is returned, never swallowed: the vault writer keeps the
//! durable outbox entry deferred and retries on its next pass.

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{DocumentRepository, NoteIndexSink};
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
use crate::shared::error::Result;

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

/// One sink per process: the vault worker calls it inline, so a note is
/// never indexed twice concurrently and no queue sits between the mirror
/// write and the corpus.
pub struct NoteIndexSinkAdapter {
    index_file: Arc<dyn NoteIndexPipeline>,
    delete_document: Arc<dyn NoteRemovalPipeline>,
    document_repo: Arc<dyn DocumentRepository>,
    embedding: Arc<dyn EmbeddingReadiness>,
    chunk_size: Arc<dyn ChunkSizePreference>,
    file_access: Arc<FileAccessConfig>,
    indexing_state: Arc<IndexingState>,
}

impl NoteIndexSinkAdapter {
    pub fn new(
        index_file: Arc<dyn NoteIndexPipeline>,
        delete_document: Arc<dyn NoteRemovalPipeline>,
        document_repo: Arc<dyn DocumentRepository>,
        embedding: Arc<dyn EmbeddingReadiness>,
        chunk_size: Arc<dyn ChunkSizePreference>,
        file_access: Arc<FileAccessConfig>,
        indexing_state: Arc<IndexingState>,
    ) -> Self {
        Self {
            index_file,
            delete_document,
            document_repo,
            embedding,
            chunk_size,
            file_access,
            indexing_state,
        }
    }
}

#[async_trait]
impl NoteIndexSink for NoteIndexSinkAdapter {
    async fn index(&self, path: &str) -> Result<()> {
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
        index_single_file(&indexing, None, request)
            .await
            .map(|_| ())
            .map_err(|failure| failure.error)
    }

    async fn remove(&self, path: &str) -> Result<()> {
        // Idempotent by contract: a note that was never indexed is already
        // gone, and the outbox entry can settle.
        let Some(document_id) = self.document_repo.find_id_by_path(path).await? else {
            tracing::debug!(target = %path, "vault note removal: not indexed — nothing to do");
            return Ok(());
        };

        delete_indexed_document(self.delete_document.as_ref(), document_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{DocumentRepositoryPort, MockEmbeddingPort, RepositoryPort};
    use crate::domain::entities::Document;
    use crate::features::indexing::commands::require_embedding_ready;
    use crate::features::indexing::engine::progress::IndexStatus;
    use crate::shared::error::{AppError, Result as AppResult};
    // The fake vault must be absolute *on the host*: `/vault` has a root but
    // no prefix, so Windows does not consider it absolute and
    // `validate_path` resolves a note under it against the current drive,
    // landing outside the configured root. `abs` spells the same fake vault
    // the way the running platform spells absolute paths.
    use crate::shared::testing::paths::{abs, abs_str};

    struct ReadyModel;

    #[async_trait]
    impl EmbeddingReadiness for ReadyModel {
        async fn ensure_ready(&self) -> Result<()> {
            Ok(())
        }
    }

    /// No active model: the loader falls back to the degraded mock, which
    /// loads fine but never reports ready. The shared gate must refuse it.
    struct NoActiveModel;

    #[async_trait]
    impl EmbeddingReadiness for NoActiveModel {
        async fn ensure_ready(&self) -> Result<()> {
            require_embedding_ready(Ok(Arc::new(MockEmbeddingPort::new_degraded()))).await
        }
    }

    /// The settings stand-in: a fixed chunk-size preference.
    struct FixedChunkSize(u32);

    #[async_trait]
    impl ChunkSizePreference for FixedChunkSize {
        async fn chunk_size(&self) -> Result<u32> {
            Ok(self.0)
        }
    }

    struct UnreadableSettings;

    #[async_trait]
    impl ChunkSizePreference for UnreadableSettings {
        async fn chunk_size(&self) -> Result<u32> {
            Err(AppError::Database("settings table is locked".to_string()))
        }
    }

    /// Records the last request handed to the index use case, and fails
    /// when told to.
    #[derive(Default)]
    struct RecordingIndexUseCase {
        last_request: parking_lot::Mutex<Option<IndexFileRequestDto>>,
        fail: bool,
    }

    impl RecordingIndexUseCase {
        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::default()
            }
        }

        fn last_path(&self) -> Option<String> {
            self.last_request.lock().as_ref().map(|r| r.path.clone())
        }

        fn last_max_tokens(&self) -> Option<usize> {
            self.last_request
                .lock()
                .as_ref()
                .map(|request| match request.chunking_strategy {
                    ChunkingStrategyDto::Semantic { max_tokens } => max_tokens,
                    ref other => panic!("vault notes are chunked semantically, got {other:?}"),
                })
        }
    }

    #[async_trait]
    impl NoteIndexPipeline for RecordingIndexUseCase {
        async fn submit(&self, request: IndexFileRequestDto) -> Result<IndexFileResponseDto> {
            let path = request.path.clone();
            *self.last_request.lock() = Some(request);
            if self.fail {
                return Err(AppError::EmbeddingFailed {
                    reason: "simulated".to_string(),
                });
            }
            Ok(IndexFileResponseDto {
                document_id: "doc-1".to_string(),
                chunks_created: 1,
                status: "indexed".to_string(),
                error: None,
                file_path: path,
            })
        }
    }

    struct RecordingDeleteUseCase {
        last_document: parking_lot::Mutex<Option<String>>,
    }

    impl RecordingDeleteUseCase {
        fn new() -> Self {
            Self {
                last_document: parking_lot::Mutex::new(None),
            }
        }

        fn last_document(&self) -> Option<String> {
            self.last_document.lock().clone()
        }
    }

    #[async_trait]
    impl NoteRemovalPipeline for RecordingDeleteUseCase {
        async fn remove(&self, document_id: String) -> Result<()> {
            *self.last_document.lock() = Some(document_id);
            Ok(())
        }
    }

    struct PathLookupRepo {
        path: Option<String>,
        id: Option<String>,
    }

    #[async_trait]
    impl RepositoryPort<Document> for PathLookupRepo {
        async fn find_by_id(&self, _id: &str) -> AppResult<Option<Document>> {
            Ok(None)
        }

        async fn find_by_filter(
            &self,
            _filter: &dyn crate::application::ports::Filter,
        ) -> AppResult<Vec<Document>> {
            Ok(vec![])
        }

        async fn find_all(&self) -> AppResult<Vec<Document>> {
            Ok(vec![])
        }

        async fn save(&self, _document: &Document) -> AppResult<()> {
            Ok(())
        }

        async fn save_batch(&self, _documents: &[Document]) -> AppResult<()> {
            Ok(())
        }

        async fn delete(&self, _document_id: &str) -> AppResult<()> {
            Ok(())
        }

        async fn delete_batch(&self, _ids: &[&str]) -> AppResult<()> {
            Ok(())
        }

        async fn count(&self) -> AppResult<usize> {
            Ok(0)
        }

        async fn exists(&self, _id: &str) -> AppResult<bool> {
            Ok(false)
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for PathLookupRepo {
        async fn find_file_path_by_id(&self, _document_id: &str) -> AppResult<String> {
            Ok(self.path.clone().unwrap_or_default())
        }

        async fn document_exists(&self, _document_id: &str) -> AppResult<bool> {
            Ok(false)
        }

        async fn find_id_by_path(&self, _file_path: &str) -> AppResult<Option<String>> {
            Ok(self.id.clone())
        }

        async fn delete(&self, document_id: &str) -> AppResult<()> {
            RepositoryPort::delete(self, document_id).await
        }

        async fn find_by_checksum(
            &self,
            _checksum: &crate::domain::value_objects::Checksum,
        ) -> AppResult<Option<Document>> {
            Ok(None)
        }

        async fn count_documents(&self) -> AppResult<i64> {
            Ok(0)
        }

        async fn count_chunks(&self) -> AppResult<i64> {
            Ok(0)
        }

        async fn find_all_paginated(&self, _limit: usize) -> AppResult<Vec<Document>> {
            Ok(vec![])
        }
    }

    impl DocumentRepository for PathLookupRepo {}

    struct Fixture {
        index: Arc<RecordingIndexUseCase>,
        delete: Arc<RecordingDeleteUseCase>,
        state: Arc<IndexingState>,
        sink: NoteIndexSinkAdapter,
    }

    fn fixture(
        index: RecordingIndexUseCase,
        repo: PathLookupRepo,
        embedding: Arc<dyn EmbeddingReadiness>,
        chunk_size: Arc<dyn ChunkSizePreference>,
    ) -> Fixture {
        let index = Arc::new(index);
        let delete = Arc::new(RecordingDeleteUseCase::new());
        let state = Arc::new(IndexingState::new());
        let sink = NoteIndexSinkAdapter::new(
            Arc::clone(&index) as Arc<dyn NoteIndexPipeline>,
            Arc::clone(&delete) as Arc<dyn NoteRemovalPipeline>,
            Arc::new(repo),
            embedding,
            chunk_size,
            Arc::new(FileAccessConfig::new(vec![abs("vault")])),
            Arc::clone(&state),
        );
        Fixture {
            index,
            delete,
            state,
            sink,
        }
    }

    /// A note inside the fake vault, as the vault writer would pass it.
    fn vault_note() -> String {
        abs_str("vault/notes/note.md")
    }

    fn unindexed() -> PathLookupRepo {
        PathLookupRepo {
            path: None,
            id: None,
        }
    }

    #[tokio::test]
    async fn index_defers_when_the_embedding_model_is_not_ready() {
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(NoActiveModel),
            Arc::new(FixedChunkSize(800)),
        );

        let error = f
            .sink
            .index(&vault_note())
            .await
            .expect_err("must fail so the outbox entry stays deferred")
            .to_string();

        assert_eq!(
            error,
            "Service not available: AI embedding model is not ready yet. Activate an embedding model in Settings -> Models and try again."
        );
        assert_eq!(f.index.last_path(), None, "use case must not run");
    }

    #[tokio::test]
    async fn the_readiness_gate_names_a_model_that_failed_to_load() {
        let error = require_embedding_ready(Err(AppError::ModelLoadFailed(
            "weights are corrupt".to_string(),
        )))
        .await
        .expect_err("a model that failed to load is not ready")
        .to_string();

        assert_eq!(
            error,
            "Service not available: Could not load the active embedding model: weights are corrupt"
        );
    }

    #[tokio::test]
    async fn index_defers_a_path_outside_the_allowed_roots() {
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(800)),
        );

        let error = f
            .sink
            .index(&abs_str("elsewhere/note.md"))
            .await
            .expect_err("a path the security layer rejects is deferred")
            .to_string();

        assert!(
            error.starts_with("Path validation failed: "),
            "got: {error}"
        );
        assert_eq!(f.index.last_path(), None, "use case must not run");
    }

    #[tokio::test]
    async fn a_saved_note_drives_the_shared_progress_state_to_complete() {
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(800)),
        );

        f.sink
            .index(&vault_note())
            .await
            .expect("a ready model indexes the note");

        assert_eq!(f.index.last_path().as_deref(), Some(vault_note().as_str()));
        let progress = f.state.get_snapshot();
        assert_eq!(progress.status, IndexStatus::Complete);
        assert_eq!(progress.total_files, 1);
        assert_eq!(progress.processed, 1);
        assert_eq!(progress.failed, 0);
    }

    #[tokio::test]
    async fn a_failed_note_index_is_recorded_in_the_shared_progress_state() {
        let f = fixture(
            RecordingIndexUseCase::failing(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(800)),
        );

        let error = f
            .sink
            .index(&vault_note())
            .await
            .expect_err("a pipeline failure keeps the outbox entry deferred");

        assert!(
            matches!(error, AppError::EmbeddingFailed { .. }),
            "the use case's typed error reaches the writer, got: {error:?}"
        );
        let progress = f.state.get_snapshot();
        assert_eq!(progress.status, IndexStatus::Error);
        assert_eq!(progress.failed, 1);
    }

    #[tokio::test]
    async fn remove_is_idempotent_for_a_path_that_was_never_indexed() {
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(800)),
        );

        f.sink
            .remove(&abs_str("vault/notes/gone.md"))
            .await
            .expect("a never-indexed note is already gone");
        assert_eq!(f.delete.last_document(), None);
    }

    #[tokio::test]
    async fn remove_deletes_the_document_that_describes_the_path() {
        let f = fixture(
            RecordingIndexUseCase::default(),
            PathLookupRepo {
                path: Some(vault_note()),
                id: Some("doc-1".to_string()),
            },
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(800)),
        );

        f.sink
            .remove(&vault_note())
            .await
            .expect("delete must succeed");
        assert_eq!(f.delete.last_document().as_deref(), Some("doc-1"));
    }

    #[tokio::test]
    async fn the_note_chunk_size_follows_the_index_file_command_policy() {
        // An out-of-range setting is clamped exactly as the command clamps it.
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(FixedChunkSize(1_000_000)),
        );
        f.sink.index(&vault_note()).await.unwrap();
        assert_eq!(f.index.last_max_tokens(), Some(chunk_tokens_for(1_000_000)));

        // Unreadable settings fall back to the command's default.
        let f = fixture(
            RecordingIndexUseCase::default(),
            unindexed(),
            Arc::new(ReadyModel),
            Arc::new(UnreadableSettings),
        );
        f.sink.index(&vault_note()).await.unwrap();
        assert_eq!(f.index.last_max_tokens(), Some(DEFAULT_CHUNK_TOKENS));
    }
}
