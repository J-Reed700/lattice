//! Dependency Injection Container
//!
//! This module provides dependency injection containers for the application.
//! It manages the lifecycle and wiring of all repositories and services.
//!
//! # Architecture
//!
//! Following the "bricks and studs" philosophy:
//! - **Container**: Pure DDD unified DI container (RECOMMENDED - use this!)
//! - **ServiceContainer**: Legacy DI container (DEPRECATED - being removed)
//! - **AppContainer**: Legacy repository-focused container (DEPRECATED)
//! - **MockAppContainer**: Test container with in-memory mocks
//!
//! # Pure DDD Container (Modern)
//!
//! **Container** is the pure DDD unified container following Domain-Driven Design.
//! It supports optional AI models and uses degraded mocks when models aren't installed.
//!
//! ## Usage in Tauri Commands
//! ```rust
//! use crate::interfaces::di::Container;
//!
//! #[tauri::command]
//! async fn search_documents(
//!     container: tauri::State<'_, Container>,
//!     query: String,
//! ) -> Result<Vec<SearchResult>, AppError> {
//!     // Rate limiting
//!     container.security_context()
//!         .rate_limiters
//!         .search
//!         .check()
//!         .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;
//!
//!     // Use DDD use cases
//!     let use_case = container.semantic_search_use_case();
//!     let results = use_case.execute(query, 10).await?;
//!
//!     Ok(results)
//! }
//! ```
//!
//! ## Legacy AppContainer Usage
//! ```rust
//! use crate::di::AppContainer;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let container = AppContainer::new(pool).await?;
//!
//!     // Use repositories
//!     let doc = container.documents().create(...).await?;
//!
//!     // Use services
//!     let embedding = container.embedding_service().embed_single("text").await?;
//!     let results = container.search_service().search(&embedding, 10);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Testing Usage
//! ```rust
//! use crate::di::MockAppContainer;
//!
//! #[tokio::test]
//! async fn test_with_mocks() {
//!     let container = MockAppContainer::new();
//!
//!     // All dependencies are mocked - no database, no ML models
//!     let doc = container.documents().create(...).await.unwrap();
//!     assert_eq!(doc.file_name(), "test.txt");
//! }
//! ```

use sqlx::SqlitePool;
use std::sync::Arc;

// ============================================================================
// Module Exports
// ============================================================================

// DDD Container - Unified DI container for DDD architecture
pub mod container;
pub use container::Container;

// Domain Modules - Modular replacement for ServiceContainer (2026-01-21)
pub mod modules;
pub use modules::{
    AIModule, CoreModule, FileOpsModule, IndexingModule, LibraryModule, SearchModule, SystemModule,
};

// Legacy exports (for backward compatibility)
use crate::infrastructure::persistence::repositories::mocks::*;
#[cfg(test)]
use crate::features::embedding::mocks::MockEmbeddingService;
#[cfg(test)]
use crate::features::mentions::mocks::MockMentionRepository;
#[cfg(test)]
use crate::features::search::mocks::MockSearchService;
#[cfg(test)]
use crate::features::tags::mocks::MockTagRepository;
use crate::shared::error::{AppError, Result};
// MockDocumentRepository is in infrastructure/persistence/repositories::mocks (imported via line 89)
// Note: MockChunkRepository is in infrastructure::persistence::repositories::mocks, not services::mocks
#[cfg(test)]
use crate::infrastructure::persistence::repositories::mocks::MockEmbeddingRepository;
// Removed: use crate::shared::traits (god object eliminated - traits migrated to infrastructure/services/traits/)
use crate::application::ports::{
    ChunkRepositoryPort, EmbeddingRepositoryPort, MentionRepositoryPort,
};
use crate::infrastructure::persistence::repositories::traits::{
    DocumentRepositoryTrait,
    /* ChunkRepositoryTrait removed - DDD */
    /* MentionRepositoryTrait removed - DDD */
    /* EmbeddingRepositoryTrait removed - DDD */
};
use crate::infrastructure::persistence::repositories::{
    ChunkRepository, DocumentRepository, EmbeddingRepository, MentionRepository, TagRepository,
};
use crate::features::embedding::service::EmbeddingService;
use crate::features::tags::TagRepositoryTrait;
use crate::infrastructure::services::traits::*;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::search::SearchServiceTrait;

// ============================================================================
// Production Container
// ============================================================================

/// Production dependency injection container
///
/// Contains real implementations backed by SQLite database and ONNX models.
/// Use this in production code and integration tests.
///
/// # Example
/// ```rust
/// let pool = get_database_pool().await?;
/// let container = AppContainer::new(pool).await?;
///
/// // Access repositories
/// let documents = container.documents();
/// let chunks = container.chunks();
///
/// // Access services
/// let embedder = container.embedding_service();
/// let searcher = container.search_service();
/// ```
pub struct AppContainer {
    // Repositories
    document_repo: Arc<DocumentRepository>,
    chunk_repo: Arc<ChunkRepository>,
    embedding_repo: Arc<EmbeddingRepository>,
    tag_repo: Arc<TagRepository>,
    mention_repo: Arc<MentionRepository>,

    // Services
    embedding_service: Option<Arc<EmbeddingService>>,
    search_service: Option<Arc<dyn SearchServiceTrait>>,
}

impl AppContainer {
    /// Create a new production container
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    ///
    /// # Returns
    /// Container with production implementations
    ///
    /// # Example
    /// ```rust
    /// use sqlx::sqlite::SqlitePoolOptions;
    ///
    /// let pool = SqlitePoolOptions::new()
    ///     .connect("sqlite:vault.db")
    ///     .await?;
    ///
    /// let container = AppContainer::new(pool).await?;
    /// ```
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        Ok(Self {
            document_repo: Arc::new(DocumentRepository::new(pool.clone())),
            chunk_repo: Arc::new(ChunkRepository::new(pool.clone())),
            embedding_repo: Arc::new(EmbeddingRepository::new(pool.clone())),
            tag_repo: Arc::new(TagRepository::new(pool.clone())),
            mention_repo: Arc::new(MentionRepository::new(pool)),
            embedding_service: None,
            search_service: None,
        })
    }

    /// Create container with embedding service
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    /// * `embedding_service` - Pre-initialized embedding service
    ///
    /// # Returns
    /// Container with embedding service configured
    ///
    /// # Example
    /// ```rust
    /// use crate::features::embedding::service::EmbeddingService;
    ///
    /// let pool = get_pool().await?;
    /// let embedder = EmbeddingService::new("models/model.onnx").await?;
    ///
    /// let container = AppContainer::with_embedding_service(pool, embedder).await?;
    /// ```
    pub async fn with_embedding_service(
        pool: SqlitePool,
        embedding_service: EmbeddingService,
    ) -> Result<Self> {
        Ok(Self {
            document_repo: Arc::new(DocumentRepository::new(pool.clone())),
            chunk_repo: Arc::new(ChunkRepository::new(pool.clone())),
            embedding_repo: Arc::new(EmbeddingRepository::new(pool.clone())),
            tag_repo: Arc::new(TagRepository::new(pool.clone())),
            mention_repo: Arc::new(MentionRepository::new(pool)),
            embedding_service: Some(Arc::new(embedding_service)),
            search_service: None,
        })
    }

    /// Create container with both embedding and search services
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    /// * `embedding_service` - Pre-initialized embedding service
    /// * `search_service` - Pre-initialized search service
    ///
    /// # Example
    /// ```rust
    /// let embedder = EmbeddingService::new(model_path).await?;
    /// let usearch_index = USearchVectorIndex::new(768, None)?;
    /// let searcher = Arc::new(usearch_index) as Arc<dyn SearchServiceTrait>;
    ///
    /// let container = AppContainer::with_services(pool, embedder, searcher).await?;
    /// ```
    pub async fn with_services(
        pool: SqlitePool,
        embedding_service: EmbeddingService,
        search_service: Arc<dyn SearchServiceTrait>,
    ) -> Result<Self> {
        Ok(Self {
            document_repo: Arc::new(DocumentRepository::new(pool.clone())),
            chunk_repo: Arc::new(ChunkRepository::new(pool.clone())),
            embedding_repo: Arc::new(EmbeddingRepository::new(pool.clone())),
            tag_repo: Arc::new(TagRepository::new(pool.clone())),
            mention_repo: Arc::new(MentionRepository::new(pool)),
            embedding_service: Some(Arc::new(embedding_service)),
            search_service: Some(search_service),
        })
    }

    // Repository accessors

    /// Get document repository
    pub fn documents(&self) -> Arc<DocumentRepository> {
        Arc::clone(&self.document_repo)
    }

    /// Get chunk repository
    pub fn chunks(&self) -> Arc<ChunkRepository> {
        Arc::clone(&self.chunk_repo)
    }

    /// Get embedding repository
    pub fn embeddings(&self) -> Arc<EmbeddingRepository> {
        Arc::clone(&self.embedding_repo)
    }

    /// Get tag repository
    pub fn tags(&self) -> Arc<TagRepository> {
        Arc::clone(&self.tag_repo)
    }

    /// Get mention repository
    pub fn mentions(&self) -> Arc<MentionRepository> {
        Arc::clone(&self.mention_repo)
    }

    // Service accessors

    /// Get embedding service
    ///
    /// # Returns
    /// Result containing the embedding service, or error if not initialized
    pub fn embedding_service(&self) -> Result<Arc<EmbeddingService>> {
        self.try_embedding_service()
            .ok_or_else(|| AppError::InternalError("EmbeddingService not initialized".to_string()))
    }

    /// Try to get embedding service
    ///
    /// # Returns
    /// `Some` if service is configured, `None` otherwise
    pub fn try_embedding_service(&self) -> Option<Arc<EmbeddingService>> {
        self.embedding_service.as_ref().map(Arc::clone)
    }

    /// Get search service
    ///
    /// # Returns
    /// Result containing the search service, or error if not initialized
    pub fn search_service(&self) -> Result<Arc<dyn SearchServiceTrait>> {
        self.try_search_service()
            .ok_or_else(|| AppError::InternalError("SearchService not initialized".to_string()))
    }

    /// Try to get search service
    ///
    /// # Returns
    /// `Some` if service is configured, `None` otherwise
    pub fn try_search_service(&self) -> Option<Arc<dyn SearchServiceTrait>> {
        self.search_service.as_ref().map(Arc::clone)
    }
}

// ============================================================================
// Mock Container
// ============================================================================

/// Mock dependency injection container for testing
///
/// Contains in-memory mock implementations. No database or ML models required.
/// Use this in unit tests for fast, isolated testing.
///
/// # Example
/// ```rust
/// #[tokio::test]
/// async fn test_document_workflow() {
///     let container = MockAppContainer::new();
///
///     // Create a document using mock repository
///     let doc = container.documents()
///         .create("/test.txt", "test.txt", "text/plain", 100, "2024-01-01", "abc")
///         .await
///         .unwrap();
///
///     // Verify it was stored
///     let found = container.documents()
///         .find_by_id(&doc.id())
///         .await
///         .unwrap();
///     assert!(found.is_some());
/// }
/// ```
#[cfg(test)]
pub struct MockAppContainer {
    // Mock Repositories
    document_repo: Arc<MockDocumentRepository>,
    chunk_repo: Arc<MockChunkRepository>,
    embedding_repo: Arc<MockEmbeddingRepository>,
    tag_repo: Arc<MockTagRepository>,
    mention_repo: Arc<MockMentionRepository>,

    // Mock Services
    embedding_service: Arc<MockEmbeddingService>,
    search_service: Arc<MockSearchService>,
}

#[cfg(test)]
impl MockAppContainer {
    /// Create a new mock container
    ///
    /// All dependencies are in-memory mocks. No external resources needed.
    ///
    /// # Example
    /// ```rust
    /// let container = MockAppContainer::new();
    /// ```
    pub fn new() -> Self {
        Self {
            document_repo: Arc::new(MockDocumentRepository::new()),
            chunk_repo: Arc::new(MockChunkRepository::new()),
            embedding_repo: Arc::new(MockEmbeddingRepository::new()),
            tag_repo: Arc::new(MockTagRepository::new()),
            mention_repo: Arc::new(MockMentionRepository::new()),
            embedding_service: Arc::new(MockEmbeddingService::new(
                crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM,
            )),
            search_service: Arc::new(MockSearchService::new()),
        }
    }

    /// Create a new mock container with custom embedding dimension
    ///
    /// # Arguments
    /// * `embedding_dim` - Dimension of mock embeddings (e.g., DEFAULT_EMBEDDING_DIM)
    pub fn with_dimension(embedding_dim: usize) -> Self {
        Self {
            document_repo: Arc::new(MockDocumentRepository::new()),
            chunk_repo: Arc::new(MockChunkRepository::new()),
            embedding_repo: Arc::new(MockEmbeddingRepository::new()),
            tag_repo: Arc::new(MockTagRepository::new()),
            mention_repo: Arc::new(MockMentionRepository::new()),
            embedding_service: Arc::new(MockEmbeddingService::new(embedding_dim)),
            search_service: Arc::new(MockSearchService::new()),
        }
    }

    // Repository accessors (trait objects for polymorphism)

    /// Get document repository as trait object
    pub fn documents(&self) -> Arc<dyn DocumentRepositoryTrait> {
        Arc::clone(&self.document_repo) as Arc<dyn DocumentRepositoryTrait>
    }

    /// Get document repository as concrete type (for test inspection)
    pub fn documents_concrete(&self) -> Arc<MockDocumentRepository> {
        Arc::clone(&self.document_repo)
    }

    /// Get chunk repository as trait object (DDD port)
    pub fn chunks(&self) -> Arc<dyn ChunkRepositoryPort> {
        Arc::clone(&self.chunk_repo) as Arc<dyn ChunkRepositoryPort>
    }

    /// Get chunk repository as concrete type (for test inspection)
    pub fn chunks_concrete(&self) -> Arc<MockChunkRepository> {
        Arc::clone(&self.chunk_repo)
    }

    /// Get embedding repository as trait object
    pub fn embeddings(&self) -> Arc<dyn EmbeddingRepositoryPort> {
        Arc::clone(&self.embedding_repo) as Arc<dyn EmbeddingRepositoryPort>
    }

    /// Get embedding repository as concrete type (for test inspection)
    pub fn embeddings_concrete(&self) -> Arc<MockEmbeddingRepository> {
        Arc::clone(&self.embedding_repo)
    }

    /// Get tag repository as trait object
    pub fn tags(&self) -> Arc<dyn TagRepositoryTrait> {
        Arc::clone(&self.tag_repo) as Arc<dyn TagRepositoryTrait>
    }

    /// Get tag repository as concrete type (for test inspection)
    pub fn tags_concrete(&self) -> Arc<MockTagRepository> {
        Arc::clone(&self.tag_repo)
    }

    /// Get mention repository as trait object
    pub fn mentions(&self) -> Arc<dyn MentionRepositoryPort> {
        Arc::clone(&self.mention_repo) as Arc<dyn MentionRepositoryPort>
    }

    /// Get mention repository as concrete type (for test inspection)
    pub fn mentions_concrete(&self) -> Arc<MockMentionRepository> {
        Arc::clone(&self.mention_repo)
    }

    // Service accessors

    /// Get embedding service as trait object
    pub fn embedding_service(&self) -> Arc<dyn EmbeddingServiceTrait> {
        Arc::clone(&self.embedding_service) as Arc<dyn EmbeddingServiceTrait>
    }

    /// Get embedding service as concrete type (for test configuration)
    pub fn embedding_service_concrete(&self) -> Arc<MockEmbeddingService> {
        Arc::clone(&self.embedding_service)
    }

    /// Get search service as trait object
    pub fn search_service(&self) -> Arc<dyn SearchServiceTrait> {
        Arc::clone(&self.search_service) as Arc<dyn SearchServiceTrait>
    }

    /// Get search service as concrete type (for test configuration)
    pub fn search_service_concrete(&self) -> Arc<MockSearchService> {
        Arc::clone(&self.search_service)
    }

    /// Clear all data from mock repositories and services
    ///
    /// Useful for resetting state between tests.
    pub fn clear_all(&self) {
        self.document_repo.clear();
        self.chunk_repo.clear();
        self.embedding_repo.clear();
        self.tag_repo.clear();
        self.mention_repo.clear();
        // Note: MockEmbeddingService is stateless (deterministic hash)
        // Note: MockSearchService needs to be cleared via Arc::get_mut or similar
    }
}

#[cfg(test)]
impl Default for MockAppContainer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests_basic {
    use super::*;

    #[tokio::test]
    async fn test_mock_container_basic_workflow() {
        let container = MockAppContainer::new();

        // Create a document
        let doc = container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        assert_eq!(doc.file_name(), "test.txt");
        assert_eq!(doc.mime_type(), "text/plain");
        use crate::domain::entities::document::DocumentStatus;
        assert_eq!(doc.status(), DocumentStatus::Indexed);

        // Find it by ID
        let found = container
            .documents()
            .find_by_id(doc.id().as_str())
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name(), "test.txt");

        // Find it by path
        let found_by_path = container
            .documents()
            .find_by_path("/test.txt")
            .await
            .unwrap();
        assert!(found_by_path.is_some());
        assert_eq!(found_by_path.unwrap().id().as_str(), doc.id().as_str());
    }

    #[tokio::test]
    async fn test_mock_container_chunk_workflow() {
        let container = MockAppContainer::new();

        // Create a document first
        let doc = container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        // Create chunks
        let chunk1 = container
            .chunks()
            .create(
                doc.id().as_str(),
                "First chunk",
                None,
                None,
                0,
                Some(0),
                Some(11),
            )
            .await
            .unwrap();

        let chunk2 = container
            .chunks()
            .create(
                doc.id().as_str(),
                "Second chunk",
                None,
                None,
                1,
                Some(11),
                Some(23),
            )
            .await
            .unwrap();

        // Find chunks by document
        let chunks = container
            .chunks()
            .find_by_document(doc.id().as_str())
            .await
            .unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content(), "First chunk");
        assert_eq!(chunks[1].content(), "Second chunk");
    }

    #[tokio::test]
    async fn test_mock_container_embedding_workflow() -> Result<(), Box<dyn std::error::Error>> {
        let container = MockAppContainer::new();

        // Create document and chunk
        let doc = container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        let chunk = container
            .chunks()
            .create(doc.id().as_str(), "Test content", None, None, 0, None, None)
            .await
            .unwrap();

        // Generate embedding using service
        let embedding = container
            .embedding_service()
            .embed_single("Test content")
            .await?;

        assert_eq!(embedding.len(), 768);

        // Store embedding
        let emb_id = container
            .embeddings()
            .create(chunk.id().as_str(), &embedding, "mock-model")
            .await
            .unwrap();

        // Retrieve embedding
        let stored = container
            .embeddings()
            .find_by_chunk(chunk.id().as_str())
            .await
            .unwrap();
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.chunk_id().as_str(), chunk.id().as_str());
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_container_tag_workflow() {
        let container = MockAppContainer::new();

        // Create document
        let doc = container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        // Create tag
        let tag = container
            .tags()
            .as_ref()
            .create_tag("important", Some("#ff0000"))
            .await
            .unwrap();

        assert_eq!(tag.name().as_str(), "important");
        assert_eq!(tag.color(), "#ff0000");

        // Add tag to document
        container
            .tags()
            .add_tag_to_document(doc.id().as_str(), tag.id().as_str())
            .await
            .unwrap();

        // Get tags for document
        let doc_tags = container
            .tags()
            .get_tags_for_document(doc.id().as_str())
            .await
            .unwrap();
        assert_eq!(doc_tags.len(), 1);
        assert_eq!(doc_tags[0].name().as_str(), "important");

        // Get documents by tag
        let tagged_docs = container
            .tags()
            .find_documents_by_tag(tag.id().as_str())
            .await
            .unwrap();
        assert_eq!(tagged_docs.len(), 1);
        assert_eq!(tagged_docs[0], doc.id().to_string());
    }

    #[tokio::test]
    async fn test_mock_container_mention_workflow() {
        let container = MockAppContainer::new();

        // Create document
        let doc = container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        // Extract mentions from text
        let text = "I met @[Alice] and discussed [[Project X]] with her.";
        let mentions = container
            .mentions()
            .extract_and_store_mentions(doc.id().as_str(), text)
            .await
            .unwrap();

        assert_eq!(mentions.len(), 2);

        // Verify mentions were stored
        let doc_mentions = container
            .mentions()
            .get_mentions_for_document(doc.id().as_str())
            .await
            .unwrap();

        assert_eq!(doc_mentions.len(), 2);
        assert!(doc_mentions.iter().any(|m| m.mention.name == "Alice"));
        assert!(doc_mentions.iter().any(|m| m.mention.name == "Project X"));
    }

    #[tokio::test]
    async fn test_mock_container_search_workflow() {
        let container = MockAppContainer::new();

        // Get mutable access to search service to add test data
        let search_service = container.search_service_concrete();

        // This test demonstrates the limitation: MockSearchService wrapped in Arc
        // can't be mutated easily. In practice, you'd populate the index differently.
        // For now, test that search returns empty results
        let query = vec![0.1; 768];
        let results = container
            .search_service()
            .search(&query, 10)
            .expect("search failed");
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_container_clear_all() {
        let container = MockAppContainer::new();

        // Add some data
        container
            .documents()
            .create(
                "/test.txt",
                "test.txt",
                "text/plain",
                1024,
                "2024-01-01T00:00:00Z",
                &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
            )
            .await
            .unwrap();

        container
            .tags()
            .as_ref()
            .create_tag("test-tag", None)
            .await
            .unwrap();

        // Clear all
        container.clear_all();

        // Verify data is cleared
        let docs = container.documents().list_all().await.unwrap();
        assert_eq!(docs.len(), 0);

        let tags = container.tags().get_all().await.unwrap();
        assert_eq!(tags.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_container_polymorphism() {
        let container = MockAppContainer::new();

        // This demonstrates that we can pass trait objects to functions
        async fn create_and_find(repo: &dyn DocumentRepositoryTrait) -> bool {
            let doc = repo
                .create(
                    "/poly.txt",
                    "poly.txt",
                    "text/plain",
                    100,
                    "2024-01-01T00:00:00Z",
                    &"a".repeat(64), // Valid SHA-256 checksum (64 hex chars)
                )
                .await
                .unwrap();

            repo.find_by_id(&doc.id().to_string())
                .await
                .unwrap()
                .is_some()
        }

        let repo = container.documents();
        let found = create_and_find(repo.as_ref()).await;
        assert!(found);
    }
}

// Comprehensive integration tests demonstrating DI patterns
#[cfg(test)]
mod tests;
