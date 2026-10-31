//! Unit tests for [`super::FunctionExecutor`].

use super::*;
use crate::features::embedding::mocks::MockEmbeddingService;
use crate::features::favorites::dto::FavoriteDto;
use crate::features::function_calling::mocks::MockFunctionRegistry;
use crate::features::recent::dto::RecentDocumentDto;
use crate::features::search::mocks::{empty_hybrid_search, MockBM25Search, MockSearchService};
use crate::features::tags::mocks::MockTagService;
use crate::features::web::mocks::MockWebService;
use crate::infrastructure::persistence::repositories::mocks::MockChunkRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::contracts::tools::{
    DocumentResult, GetDocumentOutput, ListDocumentsOutput,
};
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::DocumentRepositoryPort;
use lattice_core::application::ports::RepositoryPort;
use lattice_core::application::ports::{
    FavoritesRepositoryPort, FileMetadata, FileStoragePort, RecentDocumentsRepositoryPort,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::repositories::mocks::DddMockDocumentRepository as DddMockDocRepo;
use lattice_core::domain::value_objects::Checksum;
use std::path::{Path, PathBuf};

struct MockFavoritesRepository;
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

#[derive(Default)]
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
        Ok(String::new())
    }
    async fn exists(&self, _path: &Path) -> bool {
        !self.content.is_empty()
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
        Ok((self.document.id().as_str() == id).then(|| self.document.clone()))
    }
    async fn find_by_filter(
        &self,
        _f: &dyn lattice_core::application::ports::repository_port::Filter,
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
                "Document not found: {document_id}"
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

fn test_executor_services(
    registry: Arc<dyn FunctionRegistryTrait>,
    web: Arc<dyn WebServiceTrait>,
) -> FunctionExecutorServices {
    FunctionExecutorServices {
        registry,
        embedding_service: Arc::new(MockEmbeddingService::default()),
        search_service: Arc::new(MockSearchService::new()),
        bm25_service: Arc::new(MockBM25Search::new()),
        hybrid_search: empty_hybrid_search(),
        document_repository: Arc::new(DddMockDocRepo::new())
            as Arc<dyn lattice_core::application::ports::DocumentRepository>,
        chunk_repository: Arc::new(MockChunkRepository::new()),
        tag_service: Arc::new(MockTagService::new()),
        favorites_repository: Arc::new(MockFavoritesRepository),
        recent_documents_repository: Arc::new(MockRecentDocumentsRepository),
        file_storage: Arc::new(LongContentFileStorage::default()),
        web_service: web,
    }
}

#[tokio::test]
async fn test_execute_unknown_function() {
    let registry = Arc::new(MockFunctionRegistry::new());
    let web = Arc::new(MockWebService::new());
    let executor = FunctionExecutor::new(test_executor_services(
        registry as Arc<dyn FunctionRegistryTrait>,
        web as Arc<dyn WebServiceTrait>,
    ));

    let call = FunctionCall::new("call_123", "unknown_function", serde_json::json!({}));
    let result = executor.execute(call).await.unwrap();
    assert!(!result.success);
    assert_eq!(result.error_code.as_ref().unwrap(), "FUNCTION_NOT_FOUND");
}

#[tokio::test]
async fn test_validate_arguments() {
    use lattice_core::application::contracts::tools::ToolDefinition;
    use serde_json::json;

    let registry = Arc::new(MockFunctionRegistry::new());
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

    let web = Arc::new(MockWebService::new());
    let executor = FunctionExecutor::new(test_executor_services(
        registry as Arc<dyn FunctionRegistryTrait>,
        web as Arc<dyn WebServiceTrait>,
    ));

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
    let recent = Arc::new(RecentDocumentsRepository::new(pool.clone()));
    let mut services = test_executor_services(
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap()),
        Arc::new(MockWebService::new()),
    );
    services.document_repository = documents;
    services.favorites_repository = favorites.clone();
    services.recent_documents_repository = recent.clone();
    services.tag_service = Arc::new(crate::features::tags::service::TagService::new(pool));
    let executor = FunctionExecutor::new(services);
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
    let (all, reads) = crate::tests::common::query_counts::count_reads(executor.execute(
        FunctionCall::new("inventory", "list_documents", json!({"limit": 100})),
    ))
    .await;
    assert!(all.unwrap().success);
    assert_eq!(reads, 3, "inventory, favorites and one batch of tags");
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

    let mut services = test_executor_services(
        registry as Arc<dyn FunctionRegistryTrait>,
        Arc::new(MockWebService::new()),
    );
    services.document_repository = doc_repo;
    services.file_storage = file_storage;
    let executor = FunctionExecutor::new(services);

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
    assert!(
        middle.metadata.is_some(),
        "the citation is built from it, whatever the model sends"
    );

    // A room past 100,000 characters gets the whole document.
    let large_room = read(json!({
        "document_id": document_id,
        "max_content_length": 150_000
    }))
    .await;
    assert_eq!(large_room.content.len(), 2500);
    assert_eq!(large_room.next_offset, None);
    assert!(!large_room.content_truncated);

    // A room under 1,000 characters gets a short part and where to go on.
    let small_room = read(json!({
        "document_id": document_id,
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
    use crate::features::search::engine::text_search::SqliteTextSearch;
    use crate::features::search::use_cases::HybridSearchUseCase;
    use crate::infrastructure::persistence::repositories::ChunkRepository;
    use crate::infrastructure::persistence::repositories::DocumentRepository as SqliteDocuments;
    use crate::infrastructure::vector_index::USearchVectorIndex;
    use lattice_core::application::contracts::tools::SemanticSearchOutput;

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
    let mut services = test_executor_services(
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap()),
        Arc::new(MockWebService::new()),
    );
    services.document_repository = documents;
    services.hybrid_search = search;
    let executor = FunctionExecutor::new(services);

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
    let doc = |id: &str, score: f32, idx: usize| DocumentResult {
        document_id: id.to_string(),
        filename: format!("{id}.pdf"),
        file_path: format!("/tmp/{id}.pdf"),
        mime_type: "application/pdf".to_string(),
        score,
        snippet: "excerpt".to_string(),
        chunk_index: Some(idx),
        chunk_id: None,
        modified_at: now,
        size_bytes: 100,
    };
    let results = vec![
        doc("doc-a", 0.91, 2),
        doc("doc-a", 0.77, 5),
        doc("doc-b", 0.86, 1),
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
    let registry: Arc<dyn FunctionRegistryTrait> = Arc::new(
        crate::features::function_calling::registry::init_function_registry()
            .expect("the shipped registry builds"),
    );
    let executor = FunctionExecutor::new(test_executor_services(
        Arc::clone(&registry),
        Arc::new(MockWebService::new()),
    ));

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
    assert!(registry.get_tool("find_image").is_some());

    // find_image is listed and validated here but only runs inside a chat turn.
    let call = FunctionCall::new(
        "call_image",
        "find_image",
        serde_json::json!({"query": "Saturn V launch"}),
    );
    let result = executor.execute(call).await.expect("execution returns");
    assert!(!result.success);
    assert_eq!(result.error_code.as_deref(), Some("chat_only"));
}

/// `web_search` hands the model every provider's outcome, so a provider that
/// was challenged or paused is named in the result rather than missing.
#[tokio::test]
async fn web_search_result_carries_every_providers_outcome() {
    use lattice_core::application::contracts::web::FinderOutcomeDto;

    let web = Arc::new(MockWebService::new());
    web.set_search_results(vec![
        lattice_core::application::contracts::web::WebSearchResult {
            title: "Tokio".into(),
            url: "https://tokio.rs/".into(),
            snippet: String::new(),
            published_date: None,
            source: Some("bing_html".into()),
        },
    ]);
    web.set_search_outcomes(vec![
        FinderOutcomeDto {
            finder: "duckduckgo".into(),
            result: "skipped".into(),
            leads: 0,
            elapsed_ms: 0,
            cause: Some("paused until 14:02:00 UTC (bot challenge)".into()),
        },
        FinderOutcomeDto {
            finder: "bing_html".into(),
            result: "answered".into(),
            leads: 1,
            elapsed_ms: 640,
            cause: None,
        },
    ]);
    let executor = FunctionExecutor::new(test_executor_services(
        Arc::new(MockFunctionRegistry::new()) as Arc<dyn FunctionRegistryTrait>,
        web as Arc<dyn WebServiceTrait>,
    ));

    let result = executor
        .handle_web_search(&serde_json::json!({ "query": "tokio runtime" }))
        .await
        .unwrap();

    let data = result.data.expect("search data");
    assert_eq!(data["outcomes"][0]["finder"], "duckduckgo");
    assert_eq!(data["outcomes"][0]["result"], "skipped");
    assert_eq!(
        data["outcomes"][0]["cause"],
        "paused until 14:02:00 UTC (bot challenge)"
    );
    assert_eq!(data["outcomes"][1]["leads"], 1);
}

#[tokio::test]
async fn test_execute_find_sources() {
    use lattice_core::application::contracts::web::WebSearchResult;
    use serde_json::json;

    let registry =
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap());
    let web = Arc::new(MockWebService::new());
    web.set_search_results(vec![WebSearchResult {
        title: "Tokio Docs".into(),
        url: "https://docs.rs/tokio".into(),
        snippet: "Async runtime for Rust".into(),
        published_date: None,
        source: Some("mock".into()),
    }]);
    let executor = FunctionExecutor::new(test_executor_services(registry, web));
    let args = json!({"kind": "official_docs", "query": "tokio runtime", "subject": "tokio", "site": "docs.rs"});
    let result = executor
        .execute(FunctionCall::new("c1", "find_sources", args))
        .await
        .unwrap();
    assert!(result.success);
    let output: WebSearchOutput = serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(output.results.len(), 1);
    assert_eq!(output.results[0].title, "Tokio Docs");
    assert_eq!(output.results[0].url, "https://docs.rs/tokio");
}

struct MockLinksWebService {
    leads: Vec<lattice_core::domain::sourcing::Lead>,
}

#[async_trait]
impl WebServiceTrait for MockLinksWebService {
    async fn search_web(&self, _input: &WebSearchInput) -> Result<WebSearchOutput> {
        unimplemented!()
    }
    async fn fetch_url_content(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        unimplemented!()
    }
    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
    async fn follow_links(
        &self,
        _url: &str,
        _near_text: Option<&str>,
    ) -> Result<Vec<lattice_core::domain::sourcing::Lead>> {
        Ok(self.leads.clone())
    }
}

#[tokio::test]
async fn test_execute_follow_links() {
    use lattice_core::application::contracts::tools::FollowLinksOutput;
    use lattice_core::domain::sourcing::{Lead, LeadKind, LinkPlace, Origin};
    use serde_json::json;

    let registry =
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap());
    let page_url = url::Url::parse("https://example.com/source").unwrap();
    let target_url = url::Url::parse("https://example.com/target").unwrap();
    let place = LinkPlace {
        anchor: "anchor citation".into(),
        context: "sentence citing target".into(),
        offset: 120,
    };
    let lead = Lead {
        url: target_url,
        title: Some("Target Title".into()),
        snippet: None,
        published: None,
        kind: LeadKind::ReferenceOf {
            page: page_url,
            place: Some(place),
        },
        owner: Some(Origin::new("example.com", None)),
        rel_canonical: None,
        terms: None,
        found_by: vec![],
    };
    let web = Arc::new(MockLinksWebService { leads: vec![lead] });
    let executor = FunctionExecutor::new(test_executor_services(registry, web));
    let args = json!({"url": "https://example.com/source", "near_text": "sentence citing target"});
    let result = executor
        .execute(FunctionCall::new("c2", "follow_links", args))
        .await
        .unwrap();
    assert!(result.success);
    let output: FollowLinksOutput = serde_json::from_value(result.data.unwrap()).unwrap();
    assert_eq!(output.url, "https://example.com/source");
    assert_eq!(output.links.len(), 1);
    assert_eq!(output.links[0].url, "https://example.com/target");
    assert_eq!(output.links[0].kind, "reference");
    assert_eq!(output.links[0].anchor.as_deref(), Some("anchor citation"));
    assert_eq!(
        output.links[0].context.as_deref(),
        Some("sentence citing target")
    );
    assert_eq!(output.links[0].offset, Some(120));
    assert_eq!(output.links[0].owner.as_deref(), Some("example.com"));
}

#[tokio::test]
async fn test_execute_fetch_url_invalid_as_of() {
    let registry =
        Arc::new(crate::features::function_calling::registry::init_function_registry().unwrap());
    let web = Arc::new(MockWebService::new());
    let executor = FunctionExecutor::new(test_executor_services(registry, web));
    let args = serde_json::json!({"url": "https://example.com", "as_of": "not-a-valid-date"});
    let result = executor
        .execute(FunctionCall::new("c3", "fetch_url_content", args))
        .await
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.error_code.as_deref(), Some("EXECUTION_ERROR"));
}
