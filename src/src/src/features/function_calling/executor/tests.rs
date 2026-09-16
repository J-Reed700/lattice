//! Unit tests for [`super::FunctionExecutor`].

use super::*;
use crate::application::ports::DocumentRepositoryPort;
use crate::application::ports::{
    ChunkRepositoryPort, FavoritesRepositoryPort, FileMetadata, FileStoragePort,
    RecentDocumentsRepositoryPort,
};
use crate::domain::entities::Document;
use crate::domain::repositories::mocks::DddMockDocumentRepository as DddMockDocRepo;
use crate::domain::value_objects::Checksum;
use crate::features::embedding::mocks::MockEmbeddingService;
use crate::features::favorites::dto::FavoriteDto;
use crate::features::function_calling::dto::{
    DocumentResult, GetDocumentOutput, ListDocumentsOutput,
};
use crate::features::function_calling::mocks::MockFunctionRegistry;
use crate::features::function_calling::registry::FunctionRegistry;
use crate::features::recent::dto::RecentDocumentDto;
use crate::features::search::mocks::{MockBM25Search, MockHybridSearch, MockSearchService};
use crate::features::tags::mocks::MockTagService;
use crate::features::web::mocks::MockWebService;
use crate::infrastructure::persistence::repositories::mocks::MockChunkRepository;
use crate::RepositoryPort;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
        _filter: &dyn crate::application::ports::repository_port::Filter,
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

impl crate::application::ports::DocumentRepository for SingleDocumentRepository {}

#[tokio::test]
async fn test_execute_unknown_function() {
    let registry = Arc::new(MockFunctionRegistry::new());
    let embedding = Arc::new(MockEmbeddingService::default());
    let search = Arc::new(MockSearchService::new());
    let bm25 = Arc::new(MockBM25Search::new());
    let hybrid = Arc::new(MockHybridSearch::new());
    let mock = Arc::new(DddMockDocRepo::new());
    use crate::application::ports::DocumentRepository;
    let doc_repo = mock as Arc<dyn DocumentRepository>;
    let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
    let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
    let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
    let web = Arc::new(MockWebService::new());
    let favorites_repository = Arc::new(MockFavoritesRepository::new());
    let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());

    let executor = FunctionExecutor::new(
        registry as Arc<dyn FunctionRegistryTrait>,
        embedding as Arc<dyn EmbeddingServiceTrait>,
        search as Arc<dyn SearchServiceTrait>,
        bm25 as Arc<dyn BM25SearchTrait>,
        hybrid as Arc<dyn HybridSearchTrait>,
        doc_repo,
        chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web as Arc<dyn WebServiceTrait>,
    );

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
    let hybrid = Arc::new(MockHybridSearch::new());
    let mock = Arc::new(DddMockDocRepo::new());
    use crate::application::ports::DocumentRepository;
    let doc_repo = mock as Arc<dyn DocumentRepository>;
    let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
    let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
    let favorites_repository = Arc::new(MockFavoritesRepository::new());
    let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());
    let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
    let web = Arc::new(MockWebService::new());

    let executor = FunctionExecutor::new(
        registry as Arc<dyn FunctionRegistryTrait>,
        embedding as Arc<dyn EmbeddingServiceTrait>,
        search as Arc<dyn SearchServiceTrait>,
        bm25 as Arc<dyn BM25SearchTrait>,
        hybrid as Arc<dyn HybridSearchTrait>,
        doc_repo,
        chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web as Arc<dyn WebServiceTrait>,
    );

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
            crate::shared::domain_types::ValidatedFilePath::new(PathBuf::from(format!(
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
    let executor = FunctionExecutor::new(
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap()),
        Arc::new(MockEmbeddingService::default()),
        Arc::new(MockSearchService::new()),
        Arc::new(MockBM25Search::new()),
        Arc::new(MockHybridSearch::new()),
        documents,
        Arc::new(MockChunkRepository::new()),
        Arc::new(MockTagService::new()),
        favorites.clone(),
        recent.clone(),
        Arc::new(MockFileStoragePort),
        Arc::new(MockWebService::new()),
    );
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
async fn test_get_document_supports_internal_pagination() {
    use crate::application::ports::DocumentRepository;
    use crate::features::function_calling::domain::ToolDefinition;
    use crate::shared::domain_types::ValidatedFilePath;
    use serde_json::json;

    let registry = Arc::new(FunctionRegistry::new());
    registry
        .register(
            ToolDefinition::new(
                "get_document",
                "Get document",
                json!({
                    "type": "object",
                    "properties": {
                        "document_id": {"type": "string"},
                        "include_metadata": {"type": "boolean"},
                        "max_content_length": {
                            "type": "integer",
                            "minimum": 1000,
                            "maximum": 100000
                        },
                        "page": {
                            "type": "integer",
                            "minimum": 1
                        }
                    },
                    "required": ["document_id"]
                }),
            )
            .expect("tool schema should be valid"),
        )
        .expect("tool should register");

    let embedding = Arc::new(MockEmbeddingService::default());
    let search = Arc::new(MockSearchService::new());
    let bm25 = Arc::new(MockBM25Search::new());
    let hybrid = Arc::new(MockHybridSearch::new());
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

    let executor = FunctionExecutor::new(
        registry as Arc<dyn FunctionRegistryTrait>,
        embedding as Arc<dyn EmbeddingServiceTrait>,
        search as Arc<dyn SearchServiceTrait>,
        bm25 as Arc<dyn BM25SearchTrait>,
        hybrid as Arc<dyn HybridSearchTrait>,
        doc_repo,
        chunk_repo,
        tag_service,
        favorites_repository,
        recent_documents_repository,
        file_storage,
        web,
    );

    let call = FunctionCall::new(
        "call_get_doc",
        "get_document",
        json!({
            "document_id": document_id,
            "include_metadata": false,
            "max_content_length": 1000,
            "page": 2
        }),
    );

    let result = executor
        .execute(call)
        .await
        .expect("execution should succeed");
    assert!(result.success, "tool should succeed");

    let output: GetDocumentOutput = serde_json::from_value(result.data.expect("data expected"))
        .expect("output should deserialize");

    assert_eq!(output.page, 2);
    assert_eq!(output.total_pages, 3);
    assert_eq!(output.total_chars, 2500);
    assert_eq!(output.content.len(), 1000);
    assert!(output.has_previous_page);
    assert!(output.has_next_page);
    assert_eq!(output.previous_page, Some(1));
    assert_eq!(output.next_page, Some(3));
    assert!(output.content_truncated);
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
