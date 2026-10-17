//! Unit tests for [`super::FunctionExecutor`].

use super::*;
use crate::features::embedding::mocks::MockEmbeddingService;
use crate::features::favorites::dto::FavoriteDto;
use crate::features::function_calling::dto::{
    DocumentResult, GetDocumentOutput, ListDocumentsOutput,
};
use crate::features::function_calling::mocks::MockFunctionRegistry;
use crate::features::recent::dto::RecentDocumentDto;
use crate::features::search::mocks::{empty_hybrid_search, MockBM25Search, MockSearchService};
use crate::features::tags::mocks::MockTagService;
use crate::features::web::mocks::MockWebService;
use crate::infrastructure::persistence::repositories::mocks::MockChunkRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::application::ports::RepositoryPort;
use lattice_core::application::ports::{
    ChunkRepositoryPort, FavoritesRepositoryPort, FileMetadata, FileStoragePort,
    RecentDocumentsRepositoryPort,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::repositories::mocks::DddMockDocumentRepository as DddMockDocRepo;
use lattice_core::domain::value_objects::Checksum;
use std::path::{Path, PathBuf};

struct MockFileStoragePort;

struct MockFavoritesRepository;

impl MockFavoritesRepository {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl FavoritesRepositoryPort for MockFavoritesRepository {
    async fn add_favorite(&self, document_id: &str) -> Result<FavoriteDto> {
        Ok(FavoriteDto {
            id: "fav-1".to_string(),
            document_id: document_id.to_string(),
            document_name: "Test Document".to_string(),
            document_path: "/path/to/doc".to_string(),
            file_type: None,
            added_at: "2024-01-01T00:00:00Z".to_string(),
        })
    }

    async fn remove_favorite(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
        Ok(Vec::new())
    }

    async fn is_favorite(&self, _document_id: &str) -> Result<bool> {
        Ok(false)
    }
}

struct MockRecentDocumentsRepository;

impl MockRecentDocumentsRepository {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RecentDocumentsRepositoryPort for MockRecentDocumentsRepository {
    async fn track_access(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn get_recent_documents(&self, _limit: usize) -> Result<Vec<RecentDocumentDto>> {
        Ok(Vec::new())
    }

    async fn clear_recent_history(&self, _before_date: Option<&str>) -> Result<usize> {
        Ok(0)
    }
}

#[async_trait]
impl FileStoragePort for MockFileStoragePort {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok(String::new())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(Vec::new())
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
        Ok("".to_string())
    }

    async fn exists(&self, _path: &Path) -> bool {
        false
    }

    async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
        Ok(FileMetadata {
            size: 0,
            modified_at: 0,
            is_file: true,
            is_directory: false,
        })
    }
}

struct LongContentFileStorage {
    content: String,
}

#[async_trait]
impl FileStoragePort for LongContentFileStorage {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok(self.content.clone())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(self.content.as_bytes().to_vec())
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
        Ok("".to_string())
    }

    async fn exists(&self, _path: &Path) -> bool {
        true
    }

    async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
        Ok(FileMetadata {
            size: self.content.len() as u64,
            modified_at: 0,
            is_file: true,
            is_directory: false,
        })
    }
}

struct SingleDocumentRepository {
    document: Document,
}

impl SingleDocumentRepository {
    fn new(document: Document) -> Self {
        Self { document }
    }
}

#[async_trait]
impl RepositoryPort<Document> for SingleDocumentRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
        if self.document.id().as_str() == id {
            Ok(Some(self.document.clone()))
        } else {
            Ok(None)
        }
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn lattice_core::application::ports::repository_port::Filter,
    ) -> Result<Vec<Document>> {
        Ok(vec![self.document.clone()])
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![self.document.clone()])
    }

    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
    }

    async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(1)
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.document.id().as_str() == id)
    }
}

#[async_trait]
impl DocumentRepositoryPort for SingleDocumentRepository {
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
        if self.document.id().as_str() == document_id {
            Ok(self.document.file_path().display().to_string())
        } else {
            Err(AppError::NotFound(format!(
                "Document not found: {}",
                document_id
            )))
        }
    }

    async fn document_exists(&self, document_id: &str) -> Result<bool> {
        Ok(self.document.id().as_str() == document_id)
    }

    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
        let matches = self.document.file_path().display().to_string() == file_path;
        Ok(matches.then(|| self.document.id().as_str().to_string()))
    }

    async fn delete(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn find_by_checksum(&self, checksum: &Checksum) -> Result<Option<Document>> {
        Ok((self.document.checksum() == checksum).then(|| self.document.clone()))
    }

    async fn count_documents(&self) -> Result<i64> {
        Ok(1)
    }

    async fn count_chunks(&self) -> Result<i64> {
        Ok(self.document.chunks().len() as i64)
    }

    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<Document>> {
        Ok(vec![self.document.clone()]
            .into_iter()
            .take(limit)
            .collect())
    }
}

impl lattice_core::application::ports::DocumentRepository for SingleDocumentRepository {}

#[tokio::test]
async fn test_execute_unknown_function() {
    let registry = Arc::new(MockFunctionRegistry::new());
    let embedding = Arc::new(MockEmbeddingService::default());
    let search = Arc::new(MockSearchService::new());
    let bm25 = Arc::new(MockBM25Search::new());
    let hybrid = empty_hybrid_search();
    let mock = Arc::new(DddMockDocRepo::new());
    use lattice_core::application::ports::DocumentRepository;
    let doc_repo = mock as Arc<dyn DocumentRepository>;
    let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
    let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
    let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
    let web = Arc::new(MockWebService::new());
    let favorites_repository = Arc::new(MockFavoritesRepository::new());
    let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());

    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: registry as Arc<dyn FunctionRegistryTrait>,
        embedding_service: embedding as Arc<dyn EmbeddingServiceTrait>,
        search_service: search as Arc<dyn SearchServiceTrait>,
        bm25_service: bm25 as Arc<dyn BM25SearchTrait>,
        hybrid_search: hybrid,
        document_repository: doc_repo,
        chunk_repository: chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web_service: web as Arc<dyn WebServiceTrait>,
    });

    let call = FunctionCall::new("call_123", "unknown_function", serde_json::json!({}));

    let result = executor.execute(call).await.unwrap();
    assert!(!result.success);
    assert_eq!(result.error_code.as_ref().unwrap(), "FUNCTION_NOT_FOUND");
}

#[tokio::test]
async fn test_validate_arguments() {
    use crate::features::function_calling::domain::ToolDefinition;
    use serde_json::json;

    let registry = Arc::new(MockFunctionRegistry::new());
    let embedding = Arc::new(MockEmbeddingService::default());
    let tool = ToolDefinition::new(
        "test_function",
        "Test function",
        json!({
            "type": "object",
            "properties": {
                "required_param": {"type": "string"}
            },
            "required": ["required_param"]
        }),
    )
    .unwrap();
    registry.register(tool).unwrap();

    let search = Arc::new(MockSearchService::new());
    let bm25 = Arc::new(MockBM25Search::new());
    let hybrid = empty_hybrid_search();
    let mock = Arc::new(DddMockDocRepo::new());
    use lattice_core::application::ports::DocumentRepository;
    let doc_repo = mock as Arc<dyn DocumentRepository>;
    let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
    let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
    let favorites_repository = Arc::new(MockFavoritesRepository::new());
    let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());
    let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
    let web = Arc::new(MockWebService::new());

    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: registry as Arc<dyn FunctionRegistryTrait>,
        embedding_service: embedding as Arc<dyn EmbeddingServiceTrait>,
        search_service: search as Arc<dyn SearchServiceTrait>,
        bm25_service: bm25 as Arc<dyn BM25SearchTrait>,
        hybrid_search: hybrid,
        document_repository: doc_repo,
        chunk_repository: chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web_service: web as Arc<dyn WebServiceTrait>,
    });

    // Valid arguments
    let valid = json!({"required_param": "value"});
    assert!(executor.validate_arguments("test_function", &valid).is_ok());

    // Missing required field
    let invalid = json!({});
    assert!(executor
        .validate_arguments("test_function", &invalid)
        .is_err());
}

#[test]
fn test_normalize_tool_arguments_date_only_filters() {
    use chrono::Timelike;
    use serde_json::json;

    let args = json!({
        "query": "bigtime handbook",
        "date_from": "2000-01-01",
        "date_to": "2000-01-01"
    });

    let normalized = FunctionExecutor::normalize_tool_arguments("semantic_search", &args);

    let from = normalized
        .get("date_from")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let to = normalized
        .get("date_to")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let parsed_from = DateTime::parse_from_rfc3339(from).ok();
    let parsed_to = DateTime::parse_from_rfc3339(to).ok();

    assert!(parsed_from.is_some());
    assert!(parsed_to.is_some());

    if let Some(dt) = parsed_from {
        let utc = dt.with_timezone(&Utc);
        assert_eq!(utc.hour(), 0);
        assert_eq!(utc.minute(), 0);
        assert_eq!(utc.second(), 0);
    }

    if let Some(dt) = parsed_to {
        let utc = dt.with_timezone(&Utc);
        assert_eq!(utc.hour(), 23);
        assert_eq!(utc.minute(), 59);
        assert_eq!(utc.second(), 59);
    }
}

#[test]
fn test_normalize_tool_arguments_removes_empty_datetime_filters() {
    use serde_json::json;

    let args = json!({
        "query": "arugula bitterness",
        "date_from": "   ",
        "date_to": ""
    });

    let normalized = FunctionExecutor::normalize_tool_arguments("semantic_search", &args);

    assert!(normalized.get("date_from").is_none());
    assert!(normalized.get("date_to").is_none());
}

#[tokio::test]
async fn list_documents_uses_migrated_schema_for_inventory_favorites_and_recent() {
    use crate::features::favorites::repository::FavoritesRepository;
    use crate::features::recent::repository::RecentDocumentsRepository;
    use crate::infrastructure::persistence::repositories::DocumentRepository as SqliteDocuments;
    use serde_json::json;

    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let documents = Arc::new(SqliteDocuments::new(pool.clone()));
    for name in ["mpep-0100.pdf", "mpep-0200.pdf"] {
        let document = Document::new(
            lattice_core::shared::types::ValidatedFilePath::new(PathBuf::from(format!(
                "/tmp/{name}"
            )))
            .unwrap(),
            name.to_string(),
            "application/pdf".to_string(),
            1024,
            Checksum::new("a".repeat(64)).unwrap(),
        );
        documents.save(&document).await.unwrap();
    }
    let favorites = Arc::new(FavoritesRepository::new(pool.clone()));
    let recent = Arc::new(RecentDocumentsRepository::new(pool));
    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: Arc::new(
            crate::features::function_calling::registry::init_function_registry().unwrap(),
        ),
        embedding_service: Arc::new(MockEmbeddingService::default()),
        search_service: Arc::new(MockSearchService::new()),
        bm25_service: Arc::new(MockBM25Search::new()),
        hybrid_search: empty_hybrid_search(),
        document_repository: documents,
        chunk_repository: Arc::new(MockChunkRepository::new()),
        tag_service: Arc::new(MockTagService::new()),
        favorites_repository: favorites.clone(),
        recent_documents_repository: recent.clone(),
        file_storage: Arc::new(MockFileStoragePort),
        web_service: Arc::new(MockWebService::new()),
    });
    let list = |args| {
        let executor = &executor;
        async move {
            let result = executor
                .execute(FunctionCall::new("inventory", "list_documents", args))
                .await
                .unwrap();
            assert!(result.success, "{result:?}");
            serde_json::from_value::<ListDocumentsOutput>(result.data.unwrap()).unwrap()
        }
    };
    // Even an empty favorites table must not break the full inventory.
    let first = list(json!({"limit": 1, "sort_by": "name", "sort_order": "asc"})).await;
    assert_eq!(first.total, 2);
    assert!(first.has_more);
    assert_eq!(first.documents[0].filename, "mpep-0100.pdf");
    let id = &first.documents[0].document_id;
    favorites.add_favorite(id).await.unwrap();
    recent.track_access(id).await.unwrap();
    for mode in ["favorites", "recent"] {
        let filtered = list(json!({"filter_mode": mode})).await;
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.documents[0].document_id, *id);
        assert!(filtered.documents[0].is_favorite);
    }
    let second =
        list(json!({"limit": 1, "offset": 1, "sort_by": "name", "sort_order": "asc"})).await;
    assert_eq!(second.total, 2);
    assert!(!second.has_more);
    assert_eq!(second.documents[0].filename, "mpep-0200.pdf");
}

#[tokio::test]
async fn get_document_reads_any_part_the_shipped_schema_is_asked_for() {
    use lattice_core::application::ports::DocumentRepository;
    use lattice_core::shared::types::ValidatedFilePath;
    use serde_json::json;

    // The registry the app ships, so its schema is what validates the call:
    // the tool loop sizes a part to the result's room, which can be far more
    // than 100,000 characters on a large window or a few hundred on a full one.
    let registry = Arc::new(
        crate::features::function_calling::registry::init_function_registry()
            .expect("the shipped registry builds"),
    );

    let embedding = Arc::new(MockEmbeddingService::default());
    let search = Arc::new(MockSearchService::new());
    let bm25 = Arc::new(MockBM25Search::new());
    let hybrid = empty_hybrid_search();
    let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
    let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
    let favorites_repository = Arc::new(MockFavoritesRepository::new());
    let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());
    let web = Arc::new(MockWebService::new()) as Arc<dyn WebServiceTrait>;

    let file_path = ValidatedFilePath::new(PathBuf::from("/tmp/pagination-test.txt"))
        .expect("file path should validate");
    let checksum = Checksum::new("a".repeat(64)).expect("checksum should validate");
    let document = Document::new(
        file_path,
        "pagination-test.txt".to_string(),
        "text/plain".to_string(),
        0,
        checksum,
    );
    let document_id = document.id().as_str().to_string();
    let doc_repo = Arc::new(SingleDocumentRepository::new(document)) as Arc<dyn DocumentRepository>;

    let file_storage = Arc::new(LongContentFileStorage {
        content: "A".repeat(2500),
    }) as Arc<dyn FileStoragePort>;

    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: registry as Arc<dyn FunctionRegistryTrait>,
        embedding_service: embedding as Arc<dyn EmbeddingServiceTrait>,
        search_service: search as Arc<dyn SearchServiceTrait>,
        bm25_service: bm25 as Arc<dyn BM25SearchTrait>,
        hybrid_search: hybrid,
        document_repository: doc_repo,
        chunk_repository: chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web_service: web,
    });

    let read = |arguments: serde_json::Value| {
        let executor = &executor;
        async move {
            let result = executor
                .execute(FunctionCall::new("call_get_doc", "get_document", arguments))
                .await
                .expect("execution should succeed");
            assert!(
                result.success,
                "tool should succeed: {:?}",
                result.error_message
            );
            serde_json::from_value::<GetDocumentOutput>(result.data.expect("data expected"))
                .expect("output should deserialize")
        }
    };

    let middle = read(json!({
        "document_id": document_id,
        "include_metadata": false,
        "max_content_length": 1000,
        "offset": 1000
    }))
    .await;
    assert_eq!(middle.offset, 1000);
    assert_eq!(middle.total_chars, 2500);
    assert_eq!(middle.content.len(), 1000);
    assert_eq!(middle.next_offset, Some(2000));
    assert!(middle.content_truncated);

    // Past the old schema's 100,000-character maximum: the whole document.
    let large_room = read(json!({
        "document_id": document_id,
        "include_metadata": false,
        "max_content_length": 150_000
    }))
    .await;
    assert_eq!(large_room.content.len(), 2500);
    assert_eq!(large_room.next_offset, None);
    assert!(!large_room.content_truncated);

    // Below the old 1,000-character minimum: a short part, and where to go on.
    let small_room = read(json!({
        "document_id": document_id,
        "include_metadata": false,
        "max_content_length": 300,
        "offset": 2400
    }))
    .await;
    assert_eq!(small_room.content.len(), 100);
    assert_eq!(small_room.offset, 2400);
    assert_eq!(small_room.next_offset, None);
}

/// The tool executor's hybrid search is chat's: the library search
/// orchestrator, each hit named after its document.
#[tokio::test]
async fn hybrid_semantic_search_runs_the_library_orchestrator_and_names_hits_by_document() {
    use crate::features::function_calling::dto::SemanticSearchOutput;
    use crate::features::search::engine::text_search::SqliteTextSearch;
    use crate::features::search::engine::vector_search::USearchVectorIndex;
    use crate::features::search::use_cases::HybridSearchUseCase;
    use crate::infrastructure::persistence::repositories::ChunkRepository;
    use crate::infrastructure::persistence::repositories::DocumentRepository as SqliteDocuments;

    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let documents = Arc::new(SqliteDocuments::new(pool.clone()));
    let document = Document::new(
        lattice_core::shared::types::ValidatedFilePath::new(PathBuf::from("/tmp/quotas.md"))
            .unwrap(),
        "quotas.md".to_string(),
        "text/markdown".to_string(),
        1024,
        Checksum::new("b".repeat(64)).unwrap(),
    );
    documents.save(&document).await.unwrap();
    sqlx::query(
        "INSERT INTO text_chunks (id, document_id, content, chunk_index) \
         VALUES ('quota-chunk', ?, 'the upload quota resets every month', 0)",
    )
    .bind(document.id().as_str())
    .execute(&pool)
    .await
    .unwrap();
    // No embedding model: the keyword branch answers alone, as it would in chat.
    let search = Arc::new(
        HybridSearchUseCase::new(
            Arc::new(lattice_core::application::ports::MockEmbeddingPort::new_degraded()),
            Arc::new(USearchVectorIndex::new(4, None).unwrap()),
            Arc::new(SqliteTextSearch::new(pool.clone())),
        )
        .with_chunk_repository(Arc::new(ChunkRepository::new(pool.clone()))),
    );
    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: Arc::new(
            crate::features::function_calling::registry::init_function_registry().unwrap(),
        ),
        embedding_service: Arc::new(MockEmbeddingService::default()),
        search_service: Arc::new(MockSearchService::new()),
        bm25_service: Arc::new(MockBM25Search::new()),
        hybrid_search: search,
        document_repository: documents,
        chunk_repository: Arc::new(MockChunkRepository::new()),
        tag_service: Arc::new(MockTagService::new()),
        favorites_repository: Arc::new(MockFavoritesRepository::new()),
        recent_documents_repository: Arc::new(MockRecentDocumentsRepository::new()),
        file_storage: Arc::new(MockFileStoragePort),
        web_service: Arc::new(MockWebService::new()),
    });

    let result = executor
        .execute(FunctionCall::new(
            "search",
            "semantic_search",
            serde_json::json!({"query": "upload quota", "search_mode": "hybrid"}),
        ))
        .await
        .unwrap();

    assert!(result.success, "{result:?}");
    let output: SemanticSearchOutput = serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(output.results.len(), 1);
    assert_eq!(output.results[0].document_id, document.id().as_str());
    assert_eq!(output.results[0].filename, "quotas.md");
    assert_eq!(
        output.results[0].snippet,
        "the upload quota resets every month"
    );
}

#[test]
fn test_build_document_evidence_groups_by_document() {
    let now = Utc::now();
    let results = vec![
        DocumentResult {
            document_id: "doc-a".to_string(),
            filename: "a.pdf".to_string(),
            file_path: "/tmp/a.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            score: 0.91,
            snippet: "alpha excerpt".to_string(),
            chunk_index: Some(2),
            modified_at: now,
            size_bytes: 100,
        },
        DocumentResult {
            document_id: "doc-a".to_string(),
            filename: "a.pdf".to_string(),
            file_path: "/tmp/a.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            score: 0.77,
            snippet: "beta excerpt".to_string(),
            chunk_index: Some(5),
            modified_at: now,
            size_bytes: 100,
        },
        DocumentResult {
            document_id: "doc-b".to_string(),
            filename: "b.pdf".to_string(),
            file_path: "/tmp/b.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            score: 0.86,
            snippet: "gamma excerpt".to_string(),
            chunk_index: Some(1),
            modified_at: now,
            size_bytes: 100,
        },
    ];

    let grouped = FunctionExecutor::build_document_evidence(&results);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].document_id, "doc-a");
    assert_eq!(grouped[0].match_count, 2);
    assert!((grouped[0].max_score - 0.91).abs() < f32::EPSILON);
    assert_eq!(grouped[1].document_id, "doc-b");
    assert_eq!(grouped[1].match_count, 1);
}

#[tokio::test]
async fn test_executor_records_calls_and_syncs_custom_tools() {
    use lattice_core::application::ports::DocumentRepository;

    let registry: Arc<dyn FunctionRegistryTrait> = Arc::new(
        crate::features::function_calling::registry::init_function_registry()
            .expect("the shipped registry builds"),
    );
    let executor = FunctionExecutor::new(FunctionExecutorServices {
        registry: Arc::clone(&registry),
        embedding_service: Arc::new(MockEmbeddingService::default()),
        search_service: Arc::new(MockSearchService::new()),
        bm25_service: Arc::new(MockBM25Search::new()),
        hybrid_search: empty_hybrid_search(),
        document_repository: Arc::new(DddMockDocRepo::new()) as Arc<dyn DocumentRepository>,
        chunk_repository: Arc::new(MockChunkRepository::new()),
        tag_service: Arc::new(MockTagService::new()),
        favorites_repository: Arc::new(MockFavoritesRepository::new()),
        recent_documents_repository: Arc::new(MockRecentDocumentsRepository::new()),
        file_storage: Arc::new(MockFileStoragePort),
        web_service: Arc::new(MockWebService::new()),
    });

    let call = FunctionCall::new(
        "call_stats",
        "list_documents",
        serde_json::json!({"filter_mode": "all", "limit": 5}),
    );
    let result = executor
        .execute(call)
        .await
        .expect("execution should succeed");
    assert!(result.success);
    assert_eq!(registry.stats().call_counts.get("list_documents"), Some(&1));

    let custom_tool = CustomToolSettingsDto {
        enabled: true,
        name: "runtime_custom_search".to_string(),
        description: "Runtime custom search tool".to_string(),
        endpoint: "https://example.org/search".to_string(),
        query_param: "q".to_string(),
        max_results_param: Some("limit".to_string()),
        default_max_results: 5,
    };
    let mut map = HashMap::new();
    map.insert(custom_tool.name.clone(), custom_tool);
    executor.set_custom_tools(map);

    assert!(registry.get_tool("runtime_custom_search").is_some());

    // Removing the custom tool at runtime unregisters it from the registry while preserving built-ins.
    executor.set_custom_tools(HashMap::new());
    assert!(registry.get_tool("runtime_custom_search").is_none());
    assert!(registry.get_tool("semantic_search").is_some());
}
