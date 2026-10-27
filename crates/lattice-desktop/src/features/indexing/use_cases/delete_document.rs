//! # Delete Document Use Case
//!
//! Deletes a document and everything that belongs to it: rows, vectors, and —
//! only when Lattice owns it — the file on disk.
//!
//! ## Ownership rule
//!
//! `documents.file_path` can point at three quite different things, and only
//! two of them are ours to delete:
//!
//! - a blob in the content-addressed library: released by hash through
//!   [`LibraryGc`], which refuses while another document shares the content or
//!   an import is mid-flight on it;
//! - a web archive article: deleted through the archive service;
//! - anything else — a vault note, a file indexed where the user keeps it —
//!   is logged and left alone.

use std::path::Path;
use std::sync::Arc;

use crate::features::indexing::LibraryGc;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::application::ports::{
    ContentAddressedStoragePort, DocumentIndexObserver, DocumentRepositoryPort,
    SearchResultCachePort, VectorSearchPort,
};
use lattice_core::domain::ports::article_archive::ArticleArchive;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Request to delete a document.
///
/// Contains the document ID to be deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteDocumentRequestDto {
    /// ID of the document to delete
    pub document_id: String,
}

/// Response from deleting a document.
///
/// Contains the deletion status and confirmation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteDocumentResponseDto {
    /// Deletion status (e.g., "deleted", "not_found")
    pub status: String,

    /// Confirmation or error message
    pub message: String,
}

/// Delete document use case.
///
/// Coordinates deletion of a document and all its associated data, including:
/// - Text chunks and every child row that cascades from the document
/// - Vector embeddings from the search index
/// - The file on disk, when Lattice owns it
///
/// ## Business Rules
///
/// - Document must exist to be deleted
/// - Chunks and the document row go in one transaction; foreign keys cascade
///   embeddings, images, tags, favorites, memberships and web assets
/// - Vector keys and the artifact on disk are removed only after that commit
/// - A file Lattice does not own is never deleted
pub struct DeleteDocumentUseCase {
    document_repo: Arc<dyn DocumentRepositoryPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    library: Arc<dyn ContentAddressedStoragePort>,
    library_gc: Arc<LibraryGc>,
    web_archive: Arc<dyn ArticleArchive>,
    /// Search results cached against the corpus, dropped once a document
    /// goes so a repeated query stops finding it.
    query_cache: Option<Arc<dyn SearchResultCachePort>>,
    /// Told before a document's rows go; the summary tier drops its vectors.
    index_observer: Option<Arc<dyn DocumentIndexObserver>>,
}

impl DeleteDocumentUseCase {
    /// Create a new delete document use case.
    pub fn new(
        document_repo: Arc<dyn DocumentRepositoryPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
        library: Arc<dyn ContentAddressedStoragePort>,
        library_gc: Arc<LibraryGc>,
        web_archive: Arc<dyn ArticleArchive>,
    ) -> Self {
        Self {
            document_repo,
            vector_search,
            uow_factory,
            library,
            library_gc,
            web_archive,
            query_cache: None,
            index_observer: None,
        }
    }

    /// Invalidate `cache` whenever a document is deleted.
    #[must_use]
    pub fn with_query_cache(mut self, cache: Arc<dyn SearchResultCachePort>) -> Self {
        self.query_cache = Some(cache);
        self
    }

    /// Tell `observer` about every document this use case deletes.
    #[must_use]
    pub fn with_index_observer(mut self, observer: Arc<dyn DocumentIndexObserver>) -> Self {
        self.index_observer = Some(observer);
        self
    }

    /// Execute document deletion.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Document not found (AppError::NotFound)
    /// - Chunk retrieval fails (AppError::Database)
    /// - Vector embedding removal fails (AppError::SearchFailed)
    /// - Chunk deletion fails (AppError::Database)
    /// - Document deletion fails (AppError::Database)
    pub async fn execute(&self, document_id: String) -> Result<DeleteDocumentResponseDto> {
        // 1. Read what the artifact is before the row that describes it goes.
        //    Both lookups are metadata-only: a document that failed before it
        //    was ever chunked must still be deletable.
        let file_path = self
            .document_repo
            .find_file_path_by_id(&document_id)
            .await?;
        let checksum = self.document_repo.find_checksum_by_id(&document_id).await?;

        // Drop summary vectors before the rows go so the summary index never
        // keeps a dead entry (rows themselves cascade on delete).
        if let Some(observer) = &self.index_observer {
            observer.document_deleted(&document_id).await;
        }

        // 2. Chunks and the document row in one transaction.
        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let chunk_repo = uow.chunk_repository()?;
            let document_repo = uow.document_repository()?;

            let exists = document_repo.document_exists(&document_id).await?;
            if !exists {
                return Err(AppError::NotFound(format!(
                    "Document not found: {}",
                    document_id
                )));
            }

            let chunks = chunk_repo.find_by_document(&document_id).await?;

            chunk_repo.delete_by_document(&document_id).await?;
            DocumentRepositoryPort::delete(&*document_repo, &document_id).await?;
            Ok::<_, AppError>(chunks)
        };

        let chunks = match db_result {
            Ok(chunks) => {
                uow.commit().await?;
                chunks
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Delete failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        };

        let chunks_count = chunks.len();

        // 3. Remove vector embeddings AFTER DB commit.
        // If this fails, surface error to user (DB state is already committed).
        // Key comes from the one canonical scheme (`encoding::vector_key`),
        // shared with the writers and the startup rebuild.
        let embedding_keys: Vec<String> = chunks
            .iter()
            .map(|chunk| {
                lattice_core::shared::persistence::embedding_encoding::vector_key(
                    &chunk.id().to_string(),
                )
            })
            .collect();
        let vector_search = Arc::clone(&self.vector_search);
        tokio::task::spawn_blocking(move || vector_search.remove_embeddings(&embedding_keys))
            .await
            .map_err(|e| AppError::Other(format!("Vector deletion task failed: {}", e)))??;

        // 4. Dispose of the artifact on disk, if it is ours to dispose of.
        self.dispose_artifact(&document_id, &file_path, &checksum)
            .await;

        // The corpus changed, so cached search results are wrong: without this a
        // deleted document keeps appearing for up to the cache TTL.
        if let Some(cache) = &self.query_cache {
            cache.invalidate_corpus();
        }

        Ok(DeleteDocumentResponseDto {
            status: "deleted".to_string(),
            message: format!(
                "Document {} and {} associated chunks deleted successfully",
                document_id, chunks_count
            ),
        })
    }

    /// Delete the file this document described, if Lattice owns it.
    ///
    /// Deliberately infallible: the rows are already gone, and a file that
    /// could not be removed is an orphan the startup sweep collects, not a
    /// reason to tell the user the delete failed.
    async fn dispose_artifact(&self, document_id: &str, file_path: &str, checksum: &str) {
        let path = Path::new(file_path);

        if self.library.owns(path) {
            // By hash, never by path: another document may share this content.
            match self.library_gc.release(checksum).await {
                Ok(removal) => tracing::info!(
                    %document_id,
                    %checksum,
                    ?removal,
                    "Released the library blob behind a deleted document"
                ),
                Err(error) => tracing::warn!(
                    %document_id,
                    %checksum,
                    %error,
                    "Could not release the library blob; the next startup sweep will retry"
                ),
            }
            return;
        }

        if self.web_archive.owns(path) {
            match self.web_archive.delete_article(path).await {
                Ok(()) => tracing::info!(
                    %document_id,
                    file_path,
                    "Deleted the archived article behind a deleted document"
                ),
                Err(error) => tracing::warn!(
                    %document_id,
                    file_path,
                    %error,
                    "Could not delete the archived article (non-fatal)"
                ),
            }
            return;
        }

        tracing::info!(
            %document_id,
            file_path,
            "Document removed; its file is not ours and was left in place"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::web::mocks::MockWebArchiveService;
    use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
    use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
    use crate::infrastructure::storage::ContentAddressedStorage;
    use lattice_core::application::contracts::search::SearchResultRecord;
    use lattice_core::application::contracts::web::CleanArticle;
    use parking_lot::Mutex;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Records the keys deletion asks the vector index to drop.
    #[derive(Default)]
    struct RecordingVectorSearch {
        removed: Mutex<Vec<String>>,
    }

    impl VectorSearchPort for RecordingVectorSearch {
        fn search(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            _threshold: f32,
        ) -> Result<Vec<SearchResultRecord>> {
            Ok(Vec::new())
        }

        fn search_scoped(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            _threshold: f32,
            _allowed_document_ids: Option<&HashSet<String>>,
        ) -> Result<Vec<SearchResultRecord>> {
            Ok(Vec::new())
        }

        fn add_embedding(&self, _id: String, _embedding: Vec<f32>) -> Result<()> {
            Ok(())
        }

        fn remove_embedding(&self, id: &str) -> Result<()> {
            self.removed.lock().push(id.to_string());
            Ok(())
        }

        fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
            self.removed.lock().extend_from_slice(ids);
            Ok(())
        }

        fn clear(&self) -> Result<()> {
            Ok(())
        }

        fn count(&self) -> usize {
            0
        }

        fn dimension(&self) -> usize {
            384
        }
    }

    /// One in-memory database, one library, one archive, wired the way the
    /// container wires them. The repository is the real SQLite one so the
    /// reference count the collector reads reflects the delete that just
    /// committed.
    struct Fixture {
        temp: TempDir,
        library_root: PathBuf,
        library: Arc<ContentAddressedStorage>,
        archive: Arc<MockWebArchiveService>,
        documents: Arc<DocumentRepositoryImpl>,
        vectors: Arc<RecordingVectorSearch>,
        pool: SqlitePool,
    }

    impl Fixture {
        async fn new() -> Self {
            let temp = TempDir::new().unwrap();
            let library_root = temp.path().join("files");
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&pool).await.unwrap();

            Self {
                library: Arc::new(ContentAddressedStorage::with_root(library_root.clone())),
                archive: Arc::new(MockWebArchiveService::with_base_dir(
                    temp.path().join("web-archive"),
                )),
                documents: Arc::new(DocumentRepositoryImpl::new(pool.clone())),
                vectors: Arc::new(RecordingVectorSearch::default()),
                pool,
                library_root,
                temp,
            }
        }

        fn use_case(&self) -> DeleteDocumentUseCase {
            let documents = Arc::clone(&self.documents) as Arc<dyn DocumentRepositoryPort>;
            let library = Arc::clone(&self.library) as Arc<dyn ContentAddressedStoragePort>;
            DeleteDocumentUseCase::new(
                Arc::clone(&documents),
                Arc::clone(&self.vectors) as Arc<dyn VectorSearchPort>,
                Arc::new(SqliteUnitOfWorkFactory::new(self.pool.clone())),
                Arc::clone(&library),
                Arc::new(LibraryGc::new(library, documents)),
                Arc::clone(&self.archive) as Arc<dyn ArticleArchive>,
            )
        }

        /// Insert the row directly. Deletion needs a document row and nothing
        /// else, and a document that failed before it was ever chunked has to
        /// be deletable too.
        async fn register(&self, id: &str, path: &Path, checksum: &str) -> String {
            sqlx::query(
                "INSERT INTO documents \
                 (id, file_path, file_name, file_type, mime_type, size_bytes, \
                  modified_at, checksum, status) \
                 VALUES (?1, ?2, ?3, 'txt', 'text/plain', 12, CURRENT_TIMESTAMP, ?4, 'indexed')",
            )
            .bind(id)
            .bind(path.to_string_lossy().to_string())
            .bind(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file.txt"),
            )
            .bind(checksum)
            .execute(&self.pool)
            .await
            .unwrap();
            id.to_string()
        }

        /// Copy a file into the library and hand back where it landed.
        async fn import(&self, name: &str, content: &str) -> (String, PathBuf) {
            let source = self.temp.path().join(name);
            tokio::fs::write(&source, content).await.unwrap();
            let blob = self.library.import_file(&source).await.unwrap();
            // The import is over; from here only documents keep the blob alive.
            (blob.hash.clone(), blob.path.clone())
        }
    }

    #[tokio::test]
    async fn a_library_blob_is_released_when_its_last_document_goes() {
        let fixture = Fixture::new().await;
        let (hash, blob_path) = fixture.import("paper.txt", "library blob").await;
        fixture.register("doc-1", &blob_path, &hash).await;

        let response = fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .unwrap();

        assert_eq!(response.status, "deleted");
        assert!(!blob_path.exists(), "the blob goes with its last document");
        assert!(!fixture.library_root.join(&hash).exists());
    }

    #[tokio::test]
    async fn a_shared_library_blob_survives_until_the_last_reference() {
        let fixture = Fixture::new().await;
        let (hash, blob_path) = fixture.import("shared.txt", "shared content").await;
        fixture.register("doc-1", &blob_path, &hash).await;

        // A second row carrying the same checksum: same content, its own name
        // inside the one blob directory. `documents.file_path` is unique, so
        // sharing a blob always looks like this.
        let alias = fixture.library_root.join(&hash).join("alias.txt");
        tokio::fs::write(&alias, "shared content").await.unwrap();
        fixture.register("doc-2", &alias, &hash).await;

        fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .unwrap();

        assert!(
            blob_path.exists(),
            "the second document still references this content"
        );
        assert!(alias.exists());

        fixture
            .use_case()
            .execute("doc-2".to_string())
            .await
            .unwrap();

        assert!(!blob_path.exists(), "the last reference took it with it");
        assert!(!fixture.library_root.join(&hash).exists());
    }

    #[tokio::test]
    async fn a_file_lattice_does_not_own_is_left_alone() {
        let fixture = Fixture::new().await;
        let vault = fixture.temp.path().join("vault");
        tokio::fs::create_dir_all(&vault).await.unwrap();
        let note = vault.join("note.md");
        tokio::fs::write(&note, "a note the user owns")
            .await
            .unwrap();

        fixture.register("doc-1", &note, &"a".repeat(64)).await;
        fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .unwrap();

        assert!(note.exists(), "a user's own file is never deleted");
    }

    #[tokio::test]
    async fn a_web_article_is_deleted_through_the_archive() {
        let fixture = Fixture::new().await;
        let article = fixture
            .archive
            .base_dir()
            .join("example.com")
            .join("post.md");
        fixture.archive.set_article(
            article.clone(),
            CleanArticle {
                title: "Post".to_string(),
                author: None,
                content: "<p>Post</p>".to_string(),
                text_content: "Post".to_string(),
                word_count: 1,
                reading_time_minutes: 1,
                published_date: None,
                excerpt: None,
            },
        );

        fixture.register("doc-1", &article, &"b".repeat(64)).await;
        fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .unwrap();

        assert!(
            !fixture.archive.contains(&article),
            "the archive was asked to delete the article"
        );
    }

    #[tokio::test]
    async fn deleting_a_document_that_is_not_there_is_an_error() {
        let fixture = Fixture::new().await;

        let result = fixture.use_case().execute("missing-doc".to_string()).await;

        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_checksum_that_is_not_a_blob_hash_never_reaches_the_filesystem() {
        let fixture = Fixture::new().await;
        // A library path whose checksum column is garbage: the collector must
        // refuse the hash rather than guess, and the delete must still succeed.
        let (_hash, blob_path) = fixture.import("odd.txt", "odd content").await;
        fixture.register("doc-1", &blob_path, "not-a-hash").await;

        let response = fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .unwrap();

        assert_eq!(response.status, "deleted");
        assert!(
            blob_path.exists(),
            "an unusable checksum leaves the blob for the startup sweep"
        );
    }

    #[tokio::test]
    async fn checksum_lookup_does_not_need_the_aggregate() {
        // A document that never produced a chunk cannot be loaded as an
        // aggregate; it must still be deletable.
        let fixture = Fixture::new().await;
        let (hash, blob_path) = fixture.import("pending.txt", "never chunked").await;
        fixture.register("doc-1", &blob_path, &hash).await;

        assert_eq!(
            fixture
                .documents
                .find_checksum_by_id("doc-1")
                .await
                .unwrap(),
            hash
        );
        assert!(fixture
            .use_case()
            .execute("doc-1".to_string())
            .await
            .is_ok());
    }
}
