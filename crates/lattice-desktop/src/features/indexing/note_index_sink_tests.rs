use super::*;
use crate::features::indexing::commands::require_embedding_ready;
use crate::features::indexing::engine::progress::IndexStatus;
use lattice_core::application::ports::{
    DocumentRepositoryPort, RepositoryPort, UnavailableEmbedding,
};
use lattice_core::domain::entities::Document;
use lattice_core::shared::error::{AppError, Result as AppResult};
// The fake vault must be absolute *on the host*: `/vault` has a root but
// no prefix, so Windows does not consider it absolute and
// `validate_path` resolves a note under it against the current drive,
// landing outside the configured root. `abs` spells the same fake vault
// the way the running platform spells absolute paths.
use lattice_core::shared::testing::paths::{abs, abs_str};

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
        require_embedding_ready(Ok(Arc::new(UnavailableEmbedding::not_installed()))).await
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
    async fn find_by_id(&self, _: &str) -> AppResult<Option<Document>> {
        Ok(None)
    }
    async fn find_all(&self) -> AppResult<Vec<Document>> {
        Ok(vec![])
    }
    async fn save(&self, _: &Document) -> AppResult<()> {
        Ok(())
    }
    async fn save_batch(&self, _: &[Document]) -> AppResult<()> {
        Ok(())
    }
    async fn delete(&self, _: &str) -> AppResult<()> {
        Ok(())
    }
    async fn delete_batch(&self, _: &[&str]) -> AppResult<()> {
        Ok(())
    }
    async fn count(&self) -> AppResult<usize> {
        Ok(0)
    }
    async fn exists(&self, _: &str) -> AppResult<bool> {
        Ok(false)
    }
}

#[async_trait]
impl DocumentRepositoryPort for PathLookupRepo {
    async fn find_file_path_by_id(&self, _: &str) -> AppResult<String> {
        Ok(self.path.clone().unwrap_or_default())
    }
    async fn document_exists(&self, _: &str) -> AppResult<bool> {
        Ok(false)
    }
    async fn find_id_by_path(&self, _: &str) -> AppResult<Option<String>> {
        Ok(self.id.clone())
    }
    async fn delete(&self, id: &str) -> AppResult<()> {
        RepositoryPort::delete(self, id).await
    }
    async fn delete_and_release_blob(&self, id: &str) -> AppResult<()> {
        <Self as DocumentRepositoryPort>::delete(self, id).await
    }
    async fn find_by_checksum(
        &self,
        _: &lattice_core::domain::value_objects::Checksum,
    ) -> AppResult<Option<Document>> {
        Ok(None)
    }
    async fn count_documents(&self) -> AppResult<i64> {
        Ok(0)
    }
    async fn count_chunks(&self) -> AppResult<i64> {
        Ok(0)
    }
    async fn find_all_paginated(
        &self,
        _: lattice_core::shared::persistence::Page,
    ) -> AppResult<Vec<Document>> {
        Ok(vec![])
    }
}

impl DocumentRepository for PathLookupRepo {}

#[derive(Default)]
struct InMemoryNoteDocumentRepo {
    map: parking_lot::Mutex<std::collections::HashMap<String, String>>,
}

#[async_trait]
impl VaultNoteDocumentRepo for InMemoryNoteDocumentRepo {
    async fn find_document_id(&self, note_id: &str) -> Result<Option<String>> {
        Ok(self.map.lock().get(note_id).cloned())
    }

    async fn upsert_document_id(&self, note_id: &str, document_id: &str) -> Result<()> {
        self.map
            .lock()
            .insert(note_id.to_string(), document_id.to_string());
        Ok(())
    }

    async fn delete_mapping(&self, note_id: &str) -> Result<()> {
        self.map.lock().remove(note_id);
        Ok(())
    }
}

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
        Arc::new(InMemoryNoteDocumentRepo::default()),
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
        "Service not available: AI embedding model is not ready yet. Activate an embedding model in Settings → Downloaded and try again."
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
async fn the_readiness_gate_gives_the_error_of_a_port_that_failed_to_load() {
    let failure = AppError::ModelLoadFailed("weights are corrupt".to_string());
    let port = Arc::new(UnavailableEmbedding::failed_to_load(failure.clone()));

    let error = require_embedding_ready(Ok(port))
        .await
        .expect_err("a model that failed to load is not ready");

    assert_eq!(error.to_string(), failure.to_string());
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
    let f = fixture(
        RecordingIndexUseCase::default(),
        unindexed(),
        Arc::new(ReadyModel),
        Arc::new(FixedChunkSize(1_000_000)),
    );
    f.sink.index(&vault_note()).await.unwrap();
    assert_eq!(f.index.last_max_tokens(), Some(1_000_000));
    assert_eq!(chunk_tokens_for(1_000_000), 1_000_000);
    assert_eq!(chunk_tokens_for(0), 1);

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

#[tokio::test]
async fn reindexing_note_supersedes_previous_draft_and_remove_deletes_it_from_real_db_and_cas() {
    use crate::features::embedding::repository::EmbeddingRepository;
    use crate::features::indexing::content_extraction_adapter::ContentExtractionAdapter;
    use crate::features::indexing::engine::extraction::{ContentExtractor, ExtractionWorkers};
    use crate::features::indexing::LibraryGc;
    use crate::infrastructure::file_system::SecureFileStorage;
    use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
    use crate::infrastructure::storage::content_addressed_storage::ContentAddressedStorage;
    use crate::infrastructure::storage::FileIntents;
    use lattice_core::application::contracts::search::SearchResultRecord;
    use lattice_core::application::ports::{
        ContentAddressedStoragePort, ContentExtractionPort, EmbeddingPort, FileStoragePort,
        VectorSearchPort,
    };
    use lattice_core::domain::ports::article_archive::{ArticleArchive, SavedArticle};
    use lattice_core::shared::persistence::Database;
    use std::collections::HashSet;
    use std::path::Path;

    struct SqliteTestNoteDocRepo(Database);

    #[async_trait]
    impl VaultNoteDocumentRepo for SqliteTestNoteDocRepo {
        async fn find_document_id(&self, note_id: &str) -> Result<Option<String>> {
            let mut read = self.0.begin_read().await?;
            let id = sqlx::query_scalar::<_, String>(
                "SELECT document_id FROM vault_note_documents WHERE note_id = ?",
            )
            .bind(note_id)
            .fetch_optional(&mut *read)
            .await?;
            read.commit().await?;
            Ok(id)
        }

        async fn upsert_document_id(&self, note_id: &str, document_id: &str) -> Result<()> {
            let mut tx = self.0.begin_write().await?;
            sqlx::query(
                "INSERT INTO vault_note_documents (note_id, document_id) VALUES (?, ?) \
                 ON CONFLICT(note_id) DO UPDATE SET document_id = excluded.document_id",
            )
            .bind(note_id)
            .bind(document_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(())
        }

        async fn delete_mapping(&self, note_id: &str) -> Result<()> {
            let mut tx = self.0.begin_write().await?;
            sqlx::query("DELETE FROM vault_note_documents WHERE note_id = ?")
                .bind(note_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        }
    }

    struct DeterministicEmbedder;
    #[async_trait]
    impl EmbeddingPort for DeterministicEmbedder {
        async fn embed_single(&self, _: &str) -> AppResult<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3, 0.4])
        }
        async fn embed_batch(&self, texts: &[String]) -> AppResult<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3, 0.4]).collect())
        }
        fn dimension(&self) -> usize {
            4
        }
        async fn is_ready(&self) -> AppResult<bool> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct TestVectorSearch(parking_lot::Mutex<HashSet<String>>);
    impl VectorSearchPort for TestVectorSearch {
        fn search(&self, _: &[f32], _: usize, _: f32) -> AppResult<Vec<SearchResultRecord>> {
            Ok(Vec::new())
        }
        fn search_scoped(
            &self,
            _: &[f32],
            _: usize,
            _: f32,
            _: Option<&HashSet<String>>,
        ) -> AppResult<Vec<SearchResultRecord>> {
            Ok(Vec::new())
        }
        fn add_embedding(&self, id: String, _: Vec<f32>) -> AppResult<()> {
            self.0.lock().insert(id);
            Ok(())
        }
        fn remove_embedding(&self, id: &str) -> AppResult<()> {
            self.0.lock().remove(id);
            Ok(())
        }
        fn remove_embeddings(&self, ids: &[String]) -> AppResult<()> {
            let mut g = self.0.lock();
            for id in ids {
                g.remove(id);
            }
            Ok(())
        }
        fn clear(&self) -> AppResult<()> {
            self.0.lock().clear();
            Ok(())
        }
        fn count(&self) -> usize {
            self.0.lock().len()
        }
        fn dimension(&self) -> usize {
            4
        }
    }

    struct NonWebArchive;
    #[async_trait]
    impl ArticleArchive for NonWebArchive {
        async fn delete_article(&self, _: &Path) -> AppResult<()> {
            Ok(())
        }
        fn owns(&self, _: &Path) -> bool {
            false
        }
        fn is_saved_article(&self, _: &Path) -> bool {
            false
        }
        async fn saved_article(&self, _: &Path) -> Option<SavedArticle> {
            None
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let vault_root = temp.path().join("vault");
    let notes_dir = vault_root.join("notes");
    let library_root = temp.path().join("files");
    std::fs::create_dir_all(&notes_dir).unwrap();
    std::fs::create_dir_all(&library_root).unwrap();

    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let database = Database::new(pool.clone());

    let cas = Arc::new(ContentAddressedStorage::with_root(library_root.clone()));
    let document_repo = Arc::new(DocumentRepositoryImpl::new(database.clone()));
    let sweep_gate = Arc::new(lattice_core::shared::fs::SweepGate::default());
    sweep_gate.open();
    let library_gc = Arc::new(
        LibraryGc::new(
            Arc::clone(&cas) as Arc<dyn ContentAddressedStoragePort>,
            Arc::clone(&document_repo) as Arc<dyn DocumentRepositoryPort>,
            sweep_gate,
        )
        .with_intents(FileIntents::new(database.clone())),
    );
    let vector_search = Arc::new(TestVectorSearch::default());
    let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;
    let content_extractor = Arc::new(
        ContentExtractionAdapter::new()
            .with_extractor(ContentExtractor::new().with_workers(ExtractionWorkers::new())),
    ) as Arc<dyn ContentExtractionPort>;

    let index_uc = Arc::new(
        IndexFileUseCase::new(
            Arc::clone(&cas) as Arc<dyn ContentAddressedStoragePort>,
            file_storage,
            content_extractor,
            Arc::new(DeterministicEmbedder),
            Arc::clone(&document_repo) as Arc<dyn DocumentRepositoryPort>,
            Arc::new(EmbeddingRepository::new(database.clone())),
            database.clone(),
        )
        .with_vector_search(Arc::clone(&vector_search) as Arc<dyn VectorSearchPort>)
        .with_library_gc(Arc::clone(&library_gc)),
    );
    let delete_uc = Arc::new(DeleteDocumentUseCase::new(
        Arc::clone(&vector_search) as Arc<dyn VectorSearchPort>,
        database.clone(),
        Arc::clone(&cas) as Arc<dyn ContentAddressedStoragePort>,
        library_gc,
        Arc::new(NonWebArchive),
    ));

    let sink = NoteIndexSinkAdapter::new(
        index_uc,
        delete_uc,
        Arc::clone(&document_repo) as Arc<dyn DocumentRepository>,
        Arc::new(SqliteTestNoteDocRepo(database.clone())),
        Arc::new(ReadyModel),
        Arc::new(FixedChunkSize(800)),
        Arc::new(FileAccessConfig::new(vec![vault_root.clone()])),
        Arc::new(IndexingState::new()),
    );

    let note_path = notes_dir.join("note-sb01.md");
    let note_path_str = note_path.to_string_lossy().to_string();

    // Index v1
    std::fs::write(
        &note_path,
        "# Draft 1\n\nFirst draft content for retrieval.",
    )
    .unwrap();
    sink.index(&note_path_str).await.unwrap();
    let doc_count_v1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(doc_count_v1, 1);

    // Index v2: must supersede v1 so ONLY 1 documents row (and 1 CAS blob) remains!
    std::fs::write(
        &note_path,
        "# Draft 2\n\nSecond draft supersedes the first.",
    )
    .unwrap();
    sink.index(&note_path_str).await.unwrap();

    let doc_count_v2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(doc_count_v2, 1, "re-indexing v2 must not duplicate rows");

    let mapped_id: Option<String> = sqlx::query_scalar(
        "SELECT document_id FROM vault_note_documents WHERE note_id = 'note-sb01'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    let stored_id: String = sqlx::query_scalar("SELECT id FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mapped_id.as_deref(), Some(stored_id.as_str()));

    // Remove must delete the mapped document completely (0 rows left), even
    // though documents.file_path points into CAS (~/.lattice/files/<sha256>/...).
    sink.remove(&note_path_str).await.unwrap();
    let doc_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(doc_count_after, 0, "remove must delete the CAS-backed row");

    let mapping_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vault_note_documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mapping_count_after, 0);
    // Vault source note on disk must NEVER be deleted by CAS removal.
    assert!(note_path.exists());
}

#[tokio::test]
async fn retry_cleans_up_superseded_document_after_transient_delete_failure() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct SequenceIndex(parking_lot::Mutex<Vec<String>>);
    #[async_trait]
    impl NoteIndexPipeline for SequenceIndex {
        async fn submit(&self, request: IndexFileRequestDto) -> Result<IndexFileResponseDto> {
            let doc_id = self.0.lock().remove(0);
            Ok(IndexFileResponseDto {
                document_id: doc_id,
                chunks_created: 1,
                status: "indexed".to_string(),
                error: None,
                file_path: request.path,
            })
        }
    }
    struct FailOnceDelete {
        fail_next: AtomicBool,
        deleted: parking_lot::Mutex<Vec<String>>,
    }
    #[async_trait]
    impl NoteRemovalPipeline for FailOnceDelete {
        async fn remove(&self, document_id: String) -> Result<()> {
            if self.fail_next.swap(false, Ordering::AcqRel) {
                return Err(AppError::Database("DatabaseBusy".to_string()));
            }
            self.deleted.lock().push(document_id);
            Ok(())
        }
    }
    let note_docs = Arc::new(InMemoryNoteDocumentRepo::default());
    let delete = Arc::new(FailOnceDelete {
        fail_next: AtomicBool::new(true),
        deleted: parking_lot::Mutex::new(Vec::new()),
    });
    let sink = NoteIndexSinkAdapter::new(
        Arc::new(SequenceIndex(parking_lot::Mutex::new(vec![
            "doc-v1".into(),
            "doc-v2".into(),
            "doc-v2".into(),
        ]))),
        Arc::clone(&delete) as Arc<dyn NoteRemovalPipeline>,
        Arc::new(unindexed()),
        Arc::clone(&note_docs) as Arc<dyn VaultNoteDocumentRepo>,
        Arc::new(ReadyModel),
        Arc::new(FixedChunkSize(800)),
        Arc::new(FileAccessConfig::new(vec![abs("vault")])),
        Arc::new(IndexingState::new()),
    );
    let path = vault_note();
    sink.index(&path).await.unwrap();
    assert_eq!(
        note_docs.find_document_id("note").await.unwrap().as_deref(),
        Some("doc-v1")
    );
    assert!(sink.index(&path).await.is_err());
    assert_eq!(
        note_docs.find_document_id("note").await.unwrap().as_deref(),
        Some("doc-v1")
    );
    sink.index(&path).await.unwrap();
    assert_eq!(delete.deleted.lock().as_slice(), &["doc-v1".to_string()]);
    assert_eq!(
        note_docs.find_document_id("note").await.unwrap().as_deref(),
        Some("doc-v2")
    );
}
