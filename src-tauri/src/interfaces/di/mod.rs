//! Dependency Injection Container
//!
//! This module provides the dependency injection container for the application.
//! It manages the lifecycle and wiring of all repositories and services.
//!
//! # Architecture
//!
//! - **Container**: the unified DI container, composed from domain modules
//! - **MockAppContainer**: test container with in-memory mocks
//!
//! **Container** supports optional AI models and uses degraded mocks when
//! models aren't installed.
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

#[cfg(test)]
use std::sync::Arc;

// DDD Container - Unified DI container for DDD architecture
pub mod container;
pub use container::Container;

#[cfg(test)]
mod file_preview_tests;
pub mod modules;
pub use modules::{
    AIModule, CoreModule, FileOpsModule, IndexingModule, LibraryModule, SearchModule, SystemModule,
};

#[cfg(test)]
use crate::application::ports::{
    ChunkRepositoryPort, EmbeddingRepositoryPort, MentionRepositoryPort,
};
#[cfg(test)]
use crate::features::embedding::mocks::MockEmbeddingService;
#[cfg(test)]
use crate::features::embedding::EmbeddingServiceTrait;
#[cfg(test)]
use crate::features::mentions::mocks::MockMentionRepository;
#[cfg(test)]
use crate::features::search::mocks::MockSearchService;
#[cfg(test)]
use crate::features::search::SearchServiceTrait;
#[cfg(test)]
use crate::features::tags::mocks::MockTagRepository;
#[cfg(test)]
use crate::features::tags::TagRepositoryTrait;
#[cfg(test)]
use crate::infrastructure::persistence::repositories::mocks::{
    MockChunkRepository, MockDocumentRepository, MockEmbeddingRepository,
};
#[cfg(test)]
use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;

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
    document_repo: Arc<MockDocumentRepository>,
    chunk_repo: Arc<MockChunkRepository>,
    embedding_repo: Arc<MockEmbeddingRepository>,
    tag_repo: Arc<MockTagRepository>,
    mention_repo: Arc<MockMentionRepository>,

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
                crate::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM,
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

#[cfg(test)]
mod tests_basic {
    use super::*;

    #[tokio::test]
    async fn test_mock_container_basic_workflow() {
        let container = MockAppContainer::new();

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

        let found = container
            .documents()
            .find_by_id(doc.id().as_str())
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file_name(), "test.txt");

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

        let _chunk1 = container
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

        let _chunk2 = container
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

        let embedding = container
            .embedding_service()
            .embed_single("Test content")
            .await?;

        assert_eq!(
            embedding.len(),
            crate::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM
        );

        let _emb_id = container
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

        let tag = container
            .tags()
            .as_ref()
            .create_tag("important", Some("#ff0000"))
            .await
            .unwrap();

        assert_eq!(tag.name().as_str(), "important");
        assert_eq!(tag.color(), "#ff0000");

        container
            .tags()
            .add_tag_to_document(doc.id().as_str(), tag.id().as_str())
            .await
            .unwrap();

        let doc_tags = container
            .tags()
            .get_tags_for_document(doc.id().as_str())
            .await
            .unwrap();
        assert_eq!(doc_tags.len(), 1);
        assert_eq!(doc_tags[0].name().as_str(), "important");

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

        let text = "I met @[Alice] and discussed [[Project X]] with her.";
        let mentions = container
            .mentions()
            .extract_and_store_mentions(doc.id().as_str(), text)
            .await
            .unwrap();

        assert_eq!(mentions.len(), 2);

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

        let _search_service = container.search_service_concrete();

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
