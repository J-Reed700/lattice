//! Tests for reindexing: vector eviction, empty extractions and strategy changes.

use super::*;
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use async_trait::async_trait;
use lattice_core::application::ports::FileMetadata;
use lattice_core::domain::entities::Document;
use lattice_core::shared::types::ValidatedFilePath;
use sqlx::SqlitePool;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;

/// Records what reindex asks the vector index to do, so tests can assert
/// the index is kept in step with the database rather than silently
/// diverging from it.
#[derive(Default)]
struct MockVectorSearch {
    added: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    batches: Mutex<(usize, usize)>,
    published: Mutex<Vec<(String, String)>>,
}

impl VectorSearchPort for MockVectorSearch {
    fn search(
        &self,
        _query_embedding: &[f32],
        _top_k: usize,
        _threshold: f32,
    ) -> Result<Vec<lattice_core::application::contracts::search::SearchResultRecord>> {
        Ok(Vec::new())
    }

    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<Vec<lattice_core::application::contracts::search::SearchResultRecord>> {
        let mut results = self.search(query_embedding, top_k, threshold)?;
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
            results.truncate(top_k);
        }
        Ok(results)
    }

    fn add_embedding(&self, id: String, _embedding: Vec<f32>) -> Result<()> {
        self.added.lock().unwrap().push(id);
        Ok(())
    }

    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.removed.lock().unwrap().push(id.to_string());
        Ok(())
    }

    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        self.batches.lock().unwrap().0 += 1;
        self.removed.lock().unwrap().extend_from_slice(ids);
        Ok(())
    }

    fn publish_embeddings(
        &self,
        entries: Vec<lattice_core::application::ports::vector_search_port::VectorIndexEntry>,
    ) -> Result<()> {
        self.batches.lock().unwrap().1 += 1;
        for entry in entries {
            self.added.lock().unwrap().push(entry.id);
            self.published
                .lock()
                .unwrap()
                .push((entry.document_id, entry.content));
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        Ok(())
    }

    fn count(&self) -> usize {
        self.added.lock().unwrap().len()
    }

    fn dimension(&self) -> usize {
        384
    }
}

struct MockFileStorage {
    content: &'static str,
}

impl MockFileStorage {
    fn updated() -> Self {
        Self {
            content: "Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy. \
            We need to ensure this content is long enough to create at least one chunk when using the default FixedSize strategy with size 512. \
            This text is being extended to reach that minimum length requirement so that the reindexing tests can verify that chunks are created successfully. \
            Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
            Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
        }
    }
}

#[async_trait]
impl FileStoragePort for MockFileStorage {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok(self.content.to_string())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(b"Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy.".to_vec())
    }

    async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    async fn compute_hash(&self, _path: &Path) -> Result<String> {
        Ok("mock_hash".to_string())
    }

    async fn exists(&self, _path: &Path) -> bool {
        true
    }

    async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
        Ok(FileMetadata {
            size: 1000,
            modified_at: 1640000000,
            is_file: true,
            is_directory: false,
        })
    }
}

struct MockEmbedder;

#[async_trait]
impl EmbeddingPort for MockEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2]).collect())
    }

    fn dimension(&self) -> usize {
        2
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

async fn pool_with(document: Option<&Document>) -> (SqlitePool, Database) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let db = Database::new(pool.clone());
    if let Some(document) = document {
        let mut tx = db.begin_write().await.unwrap();
        document::ops::save(&mut tx, document).await.unwrap();
        tx.commit().await.unwrap();
    }
    (pool, db)
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

struct MockDocumentRepo {
    document: Mutex<Option<Document>>,
}

impl MockDocumentRepo {
    fn new() -> Self {
        Self {
            document: Mutex::new(None),
        }
    }

    fn set_document(&self, doc: Document) {
        *self.document.lock().unwrap() = Some(doc);
    }
}

#[async_trait]
impl RepositoryPort<Document> for MockDocumentRepo {
    async fn save(&self, entity: &Document) -> Result<()> {
        *self.document.lock().unwrap() = Some(entity.clone());
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(self.document.lock().unwrap().clone())
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(self.document.lock().unwrap().iter().cloned().collect())
    }

    async fn save_batch(&self, entities: &[Document]) -> Result<()> {
        if let Some(entity) = entities.first() {
            *self.document.lock().unwrap() = Some(entity.clone());
        }
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        *self.document.lock().unwrap() = None;
        Ok(())
    }

    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        *self.document.lock().unwrap() = None;
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(if self.document.lock().unwrap().is_some() {
            1
        } else {
            0
        })
    }

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(self.document.lock().unwrap().is_some())
    }
}

fn create_test_aggregate() -> (Document, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Original content").unwrap();

    let validated_path = ValidatedFilePath::new(file_path).unwrap();
    let content = "Original content".to_string();
    let strategy = ChunkingStrategy::FixedSize { size: 10 };
    let metadata = lattice_core::domain::value_objects::FileMetadata::new(
        "test.txt".to_string(),
        "text/plain".to_string(),
        content.len() as i64,
        chrono::Utc::now(),
    )
    .unwrap();
    let checksum = lattice_core::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

    let document =
        Document::from_file(validated_path, metadata, checksum, content, strategy).unwrap();
    (document, temp_dir)
}

#[tokio::test]

async fn test_reindex_document() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();
    let _original_chunks = aggregate.chunks().len();

    let (pool, db) = pool_with(Some(&aggregate)).await;
    repo.set_document(aggregate);

    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo.clone(),
        db,
        Arc::new(MockVectorSearch::default()),
    );

    let response = use_case.execute(doc_id.clone()).await.unwrap();

    assert_eq!(response.document_id, doc_id);
    assert_eq!(response.status, "reindexed");
    assert!(response.error.is_none());
    // Chunks may differ due to different content
    assert!(response.chunks_created > 0);
    assert_eq!(
        count(&pool, "text_embeddings").await,
        response.chunks_created as i64
    );
}

/// Reindex keeps the live vector index in step with the database: no
/// vectors for deleted chunks, and the edited content searchable.
#[tokio::test]
async fn reindex_evicts_stale_vectors_and_adds_new_ones() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();

    let stale_keys: Vec<String> = aggregate
        .chunks()
        .iter()
        .map(|c| {
            lattice_core::shared::persistence::embedding_encoding::vector_key(&c.id().to_string())
        })
        .collect();
    assert!(!stale_keys.is_empty(), "fixture must start with chunks");

    let (_, db) = pool_with(Some(&aggregate)).await;
    repo.set_document(aggregate);

    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo.clone(),
        db,
        vector_search.clone(),
    );

    let response = use_case.execute(doc_id).await.unwrap();

    let removed = vector_search.removed.lock().unwrap().clone();
    for key in &stale_keys {
        assert!(
            removed.contains(key),
            "stale vector {} was not evicted from the index",
            key
        );
    }

    assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
    assert!(vector_search
        .published
        .lock()
        .unwrap()
        .iter()
        .all(|(id, text)| !id.is_empty() && !text.is_empty()));
    let added = vector_search.added.lock().unwrap().clone();
    assert_eq!(
        added.len(),
        response.chunks_created,
        "every new chunk must be added to the vector index"
    );
    assert!(
        added.iter().all(|k| k.starts_with("emb_")),
        "added keys must use the canonical scheme: {:?}",
        added
    );
}

#[tokio::test]
async fn a_file_that_now_extracts_to_whitespace_fails_and_keeps_its_vectors() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();
    let (_, db) = pool_with(Some(&aggregate)).await;
    repo.set_document(aggregate);
    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage {
            content: " \n\t\n ",
        }),
        Arc::new(MockEmbedder),
        repo,
        db,
        vector_search.clone(),
    );

    let error = use_case.execute(doc_id).await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(m) if m.contains("no extractable text")),
        "{error:?}"
    );
    assert!(vector_search.removed.lock().unwrap().is_empty());
    assert!(vector_search.added.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_reindex_document_not_found() {
    let repo = Arc::new(MockDocumentRepo::new());
    // Don't set any document

    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo,
        pool_with(None).await.1,
        Arc::new(MockVectorSearch::default()),
    );

    let result = use_case.execute("nonexistent-id".to_string()).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]

async fn test_reindex_with_different_strategy() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();

    let (_, db) = pool_with(Some(&aggregate)).await;
    repo.set_document(aggregate);

    let vector_search = Arc::new(MockVectorSearch::default());
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo,
        db,
        vector_search.clone(),
    );

    // Reindex with different chunk size
    let new_strategy = ChunkingStrategy::FixedSize { size: 5 };
    let response = use_case
        .execute_with_strategy(doc_id, new_strategy)
        .await
        .unwrap();

    assert!(response.chunks_created > 0);
    assert_eq!(response.status, "reindexed");
    assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
    assert_eq!(
        vector_search.published.lock().unwrap().len(),
        response.chunks_created
    );
}

#[tokio::test]
async fn reindex_refreshes_stale_octet_stream_mime_type_and_marks_yaml_chunks_as_code() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("config.yaml");
    let yaml_content = "server:\n  host: 127.0.0.1\n  port: 8080\n";
    fs::write(&file_path, yaml_content).unwrap();

    // Simulate a legacy row imported with "application/octet-stream"
    let metadata = lattice_core::domain::value_objects::FileMetadata::new(
        "config.yaml".to_string(),
        "application/octet-stream".to_string(),
        yaml_content.len() as i64,
        chrono::Utc::now(),
    )
    .unwrap();
    let checksum = lattice_core::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();
    let document = Document::from_file(
        ValidatedFilePath::new(file_path).unwrap(),
        metadata,
        checksum,
        yaml_content.to_string(),
        ChunkingStrategy::FixedSize { size: 512 },
    )
    .unwrap();
    let doc_id = document.id().to_string();

    let repo = Arc::new(MockDocumentRepo::new());
    let (_, db) = pool_with(Some(&document)).await;
    repo.set_document(document);

    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage {
            content: yaml_content,
        }),
        Arc::new(MockEmbedder),
        repo,
        db.clone(),
        Arc::new(MockVectorSearch::default()),
    );

    use_case.execute(doc_id.clone()).await.unwrap();

    let persisted = DocumentRepositoryImpl::new(db)
        .find_by_id(&doc_id)
        .await
        .unwrap()
        .expect("the reindexed document is stored");
    assert_eq!(persisted.mime_type(), "text/x-yaml");
    assert!(persisted.chunks().iter().all(|c| c.has_code()));
}

#[tokio::test]
async fn reindex_does_not_resurrect_a_document_deleted_meanwhile() {
    let repo = Arc::new(MockDocumentRepo::new());
    let (aggregate, _temp_dir) = create_test_aggregate();
    let doc_id = aggregate.id().to_string();
    let (pool, db) = pool_with(None).await;
    repo.set_document(aggregate);
    let use_case = ReindexDocumentUseCase::new(
        Arc::new(MockFileStorage::updated()),
        Arc::new(MockEmbedder),
        repo,
        db,
        Arc::new(MockVectorSearch::default()),
    );

    let error = use_case.execute(doc_id).await.unwrap_err();

    assert!(matches!(error, AppError::NotFound(_)), "{error:?}");
    assert_eq!(count(&pool, "documents").await, 0);
    assert_eq!(count(&pool, "text_embeddings").await, 0);
}
