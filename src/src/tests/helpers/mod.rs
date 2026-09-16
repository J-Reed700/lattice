//! # Integration Test Helpers
//!
//! Comprehensive helper utilities for integration testing of the Tauri backend.
//!
//! ## Overview
//!
//! This module provides reusable test infrastructure including:
//! - **TestContext**: Complete test environment setup
//! - **Factories**: Test data generation
//! - **Mocks**: Mock implementations for external dependencies
//! - **Utilities**: Common test operations
//!
//! ## Usage Example
//!
//! ```rust
//! use lattice::tests::helpers::TestContext;
//!
//! #[tokio::test]
//! async fn test_document_indexing() {
//!     let ctx = TestContext::new().await.unwrap();
//!
//!     let doc = ctx.create_test_document("test.md", "Test content").await.unwrap();
//!     let chunks = ctx.create_test_chunks(&doc.id, 5).await.unwrap();
//!
//!     assert_eq!(chunks.len(), 5);
//!     // Context automatically cleans up when dropped
//! }
//! ```

use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;
use lattice::error::Result;
use lattice::infrastructure::persistence::repositories::{
    chunk_repository::ChunkRepository,
    document_repository::DocumentRepository,
    embedding_repository::EmbeddingRepository,
    mention_repository::MentionRepository,
    tag_repository::TagRepository,
};
use uuid::Uuid;

pub mod factories;
pub mod mocks;
pub mod assertions;
pub mod dependency_builder;

// Re-export commonly used items
pub use factories::*;
pub use mocks::*;
pub use dependency_builder::{
    DependencyBuilder,
    DependencyBuildResult,
    create_indexed_document,
    create_document_tree,
    create_minimal_document,
    create_chunked_document,
};

// ============================================================================
// TestContext - Main Test Environment
// ============================================================================

/// Comprehensive test context for integration tests.
///
/// Provides a complete test environment including:
/// - In-memory SQLite database
/// - Mock embedding service
/// - All repositories
/// - Automatic cleanup on drop
///
/// # Examples
///
/// ```rust
/// let ctx = TestContext::new().await?;
/// let doc_repo = ctx.doc_repo();
/// let doc = ctx.create_test_document("test.md", "content").await?;
/// ```
pub struct TestContext {
    /// SQLite connection pool (in-memory)
    pub pool: SqlitePool,

    /// Temporary directory for test files
    pub temp_dir: TempDir,

    /// Mock embedding service
    pub embedder: Arc<MockEmbedder>,

    /// Test data cleanup tracker
    cleanup_tracker: Arc<Mutex<CleanupTracker>>,
}

impl TestContext {
    /// Create a new test context with in-memory database.
    ///
    /// This sets up:
    /// - In-memory SQLite database
    /// - Database schema and migrations
    /// - Mock services
    /// - Temporary directory
    ///
    /// # Errors
    ///
    /// Returns error if database initialization fails.
    pub async fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| lattice::error::AppError::Other(format!("Failed to create temp dir: {}", e)))?;

        let uuid = Uuid::new_v4().simple().to_string();
        let db_path = temp_dir.path().join(format!("test_{}.db", uuid));
        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        // Create connection pool
        let pool = SqlitePool::connect(&db_url).await
            .map_err(|e| lattice::error::AppError::Database(format!("Failed to connect: {}", e)))?;

        // Initialize database schema
        lattice::infrastructure::persistence::database::init::initialize_database(&pool).await
            .map_err(|e| lattice::error::AppError::Database(format!("Failed to initialize: {}", e)))?;

        // Create mock embedder
        let embedder = Arc::new(MockEmbedder::new(384)); // Standard embedding dimension

        Ok(Self {
            pool,
            temp_dir,
            embedder,
            cleanup_tracker: Arc::new(Mutex::new(CleanupTracker::new())),
        })
    }

    /// Create a test context with custom embedding dimensions.
    pub async fn with_embedding_dim(dim: usize) -> Result<Self> {
        let mut ctx = Self::new().await?;
        ctx.embedder = Arc::new(MockEmbedder::new(dim));
        Ok(ctx)
    }

    // ========================================================================
    // Repository Accessors
    // ========================================================================

    /// Get document repository instance.
    pub fn doc_repo(&self) -> DocumentRepository {
        DocumentRepository::new(self.pool.clone())
    }

    /// Get tag repository instance.
    pub fn tag_repo(&self) -> TagRepository {
        TagRepository::new(self.pool.clone())
    }

    /// Get mention repository instance.
    pub fn mention_repo(&self) -> MentionRepository {
        MentionRepository::new(self.pool.clone())
    }

    /// Get chunk repository instance.
    pub fn chunk_repo(&self) -> ChunkRepository {
        ChunkRepository::new(self.pool.clone())
    }

    /// Get embedding repository instance.
    pub fn embedding_repo(&self) -> EmbeddingRepository {
        EmbeddingRepository::new(self.pool.clone())
    }

    // ========================================================================
    // Test Data Factories (Convenience Methods)
    // ========================================================================

    /// Create a test document with default values.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Name of the file
    /// * `content` - Document content
    ///
    /// # Returns
    ///
    /// Created document with generated ID
    pub async fn create_test_document(&self, file_name: &str, content: &str) -> Result<TestDocument> {
        // Use DependencyBuilder for proper test data creation
        let builder = DependencyBuilder::new(self.pool.clone())
            .with_document(file_name, content);

        let result = builder.build().await?;

        // Convert from DependencyBuildResult to TestDocument
        let doc = TestDocument {
            id: result.document.id.clone(),
            vault_id: "test-lattice".to_string(),
            file_path: result.document.file_path,
            file_name: result.document.file_name,
            file_type: "text".to_string(),
            file_size: result.document.content.len() as i64,
            content: result.document.content,
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };

        self.track_cleanup("document", &doc.id).await;
        Ok(doc)
    }

    /// Create multiple test chunks for a document.
    ///
    /// # Arguments
    ///
    /// * `doc_id` - Document ID to associate chunks with
    /// * `count` - Number of chunks to create
    ///
    /// # Returns
    ///
    /// Vector of created chunks
    pub async fn create_test_chunks(&self, doc_id: &str, count: usize) -> Result<Vec<TestChunk>> {
        let mut chunks = Vec::new();
        let chunk_repo = self.chunk_repo();

        for i in 0..count {
            let chunk = ChunkFactory::new()
                .document_id(doc_id)
                .content(&format!("Test chunk content {}", i))
                .chunk_index(i as i32)
                .build();

            // Use repository to insert chunk
            let chunk_id = chunk.insert_into_db(&chunk_repo).await?;
            self.track_cleanup("chunk", &chunk_id).await;
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    /// Create test tags.
    ///
    /// # Arguments
    ///
    /// * `names` - Tag names to create
    ///
    /// # Returns
    ///
    /// Vector of created tags
    pub async fn create_test_tags(&self, names: &[&str]) -> Result<Vec<TestTag>> {
        let tag_repo = self.tag_repo();
        let mut tags = Vec::new();

        for name in names {
            let tag = tag_repo.get_or_create(name, None).await?;
            let test_tag = TestTag {
                id: tag.id().to_string(),
                name: tag.name().as_str().to_string(),
                color: Some(tag.color().to_string()),
            };

            self.track_cleanup("tag", &test_tag.id).await;
            tags.push(test_tag);
        }

        Ok(tags)
    }

    /// Create test embeddings for chunks.
    ///
    /// # Arguments
    ///
    /// * `chunk_ids` - Chunk IDs to create embeddings for
    ///
    /// # Returns
    ///
    /// Vector of created embeddings
    pub async fn create_test_embeddings(&self, chunk_ids: &[&str]) -> Result<Vec<TestEmbedding>> {
        let mut embeddings = Vec::new();
        let embedding_repo = self.embedding_repo();

        for chunk_id in chunk_ids {
            let embedding_vec = self.embedder.embed_text("test").await?;

            let embedding = EmbeddingFactory::new()
                .chunk_id(chunk_id)
                .embedding(embedding_vec)
                .build();

            // Use repository to insert embedding
            embedding.insert_into_db(&embedding_repo).await?;
            self.track_cleanup("embedding", chunk_id).await;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    // ========================================================================
    // Utility Methods
    // ========================================================================

    /// Get temporary directory path.
    pub fn temp_path(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Create a test file in temp directory.
    pub async fn create_temp_file(&self, name: &str, content: &str) -> Result<PathBuf> {
        let path = self.temp_dir.path().join(name);
        tokio::fs::write(&path, content).await
            .map_err(|e| lattice::error::AppError::Other(format!("Failed to write file: {}", e)))?;
        Ok(path)
    }

    /// Track resource for cleanup.
    async fn track_cleanup(&self, resource_type: &str, id: &str) {
        let mut tracker = self.cleanup_tracker.lock().await;
        tracker.track(resource_type.to_string(), id.to_string());
    }

    /// Get database statistics.
    pub async fn get_db_stats(&self) -> Result<DatabaseStats> {
        let mut conn = self.pool.acquire().await
            .map_err(|e| lattice::error::AppError::Database(format!("Failed to acquire: {}", e)))?;

        let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&mut *conn).await.unwrap_or(0);

        let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
            .fetch_one(&mut *conn).await.unwrap_or(0);

        let tag_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags")
            .fetch_one(&mut *conn).await.unwrap_or(0);

        let mention_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mentions")
            .fetch_one(&mut *conn).await.unwrap_or(0);

        Ok(DatabaseStats {
            documents: doc_count as usize,
            chunks: chunk_count as usize,
            tags: tag_count as usize,
            mentions: mention_count as usize,
        })
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Cleanup is automatic - temp_dir will be deleted
        // Database is in-memory, so no cleanup needed
    }
}

// ============================================================================
// Helper Structs
// ============================================================================

#[derive(Debug, Clone)]
struct CleanupTracker {
    resources: Vec<(String, String)>, // (type, id)
}

impl CleanupTracker {
    fn new() -> Self {
        Self {
            resources: Vec::new(),
        }
    }

    fn track(&mut self, resource_type: String, id: String) {
        self.resources.push((resource_type, id));
    }
}

/// Database statistics for testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseStats {
    pub documents: usize,
    pub chunks: usize,
    pub tags: usize,
    pub mentions: usize,
}

// ============================================================================
// Standalone Helper Functions
// ============================================================================

/// Create an in-memory test database.
///
/// # Returns
///
/// Initialized SQLite pool
pub async fn setup_test_db() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await
        .map_err(|e| lattice::error::AppError::Database(format!("Failed to connect: {}", e)))?;

    lattice::infrastructure::persistence::database::init::initialize_database(&pool).await
        .map_err(|e| lattice::error::AppError::Database(format!("Failed to initialize: {}", e)))?;

    Ok(pool)
}

/// Create a mock embedder with specified dimensions.
///
/// # Arguments
///
/// * `dimensions` - Embedding vector size
///
/// # Returns
///
/// Mock embedder instance
pub fn setup_test_embedder(dimensions: usize) -> Arc<MockEmbedder> {
    Arc::new(MockEmbedder::new(dimensions))
}

// ============================================================================
// Test Data Models
// ============================================================================

#[derive(Debug, Clone)]
pub struct TestDocument {
    pub id: String,
    pub vault_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: i64,
    pub content: String,
    pub indexed_at: String,
}

#[derive(Debug, Clone)]
pub struct TestChunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub chunk_index: i32,
    pub start_char: Option<i32>,
    pub end_char: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct TestTag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestEmbedding {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_creation() {
        let ctx = TestContext::new().await.unwrap();
        assert!(ctx.pool.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn test_document_creation() {
        let ctx = TestContext::new().await.unwrap();
        let doc = ctx.create_test_document("test.md", "Test content").await.unwrap();

        assert!(!doc.id.is_empty());
        assert_eq!(doc.file_name, "test.md");
        assert_eq!(doc.content, "Test content");
    }

    #[tokio::test]
    async fn test_chunk_creation() {
        let ctx = TestContext::new().await.unwrap();
        let doc = ctx.create_test_document("test.md", "content").await.unwrap();
        let chunks = ctx.create_test_chunks(&doc.id, 3).await.unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[2].chunk_index, 2);
    }

    #[tokio::test]
    async fn test_db_stats() {
        let ctx = TestContext::new().await.unwrap();

        let initial_stats = ctx.get_db_stats().await.unwrap();
        assert_eq!(initial_stats.documents, 0);

        ctx.create_test_document("test.md", "content").await.unwrap();

        let final_stats = ctx.get_db_stats().await.unwrap();
        assert_eq!(final_stats.documents, 1);
    }

    #[tokio::test]
    async fn test_uuid_based_database_paths() {
        let ctx1 = TestContext::new().await.unwrap();
        let ctx2 = TestContext::new().await.unwrap();

        let path1_str = ctx1.temp_dir.path().to_string_lossy().to_string();
        let path2_str = ctx2.temp_dir.path().to_string_lossy().to_string();

        assert_ne!(path1_str, path2_str, "Each TestContext should have unique temp directory");
        assert!(path1_str.len() > 0, "Path should not be empty");
        assert!(path2_str.len() > 0, "Path should not be empty");
    }

    #[tokio::test]
    async fn test_parallel_database_isolation() {
        let ctx1 = TestContext::new().await.unwrap();
        let ctx2 = TestContext::new().await.unwrap();

        ctx1.create_test_document("doc1.md", "content1").await.unwrap();
        ctx2.create_test_document("doc2.md", "content2").await.unwrap();

        let stats1 = ctx1.get_db_stats().await.unwrap();
        let stats2 = ctx2.get_db_stats().await.unwrap();

        assert_eq!(stats1.documents, 1, "Context 1 should have 1 document");
        assert_eq!(stats2.documents, 1, "Context 2 should have 1 document");
    }

    #[tokio::test]
    async fn test_concurrent_context_creation() {
        let handles: Vec<_> = (0..5)
            .map(|i| {
                tokio::spawn(async move {
                    let ctx = TestContext::new().await.unwrap();
                    ctx.create_test_document(
                        &format!("doc_{}.md", i),
                        &format!("content {}", i)
                    ).await.unwrap();

                    let stats = ctx.get_db_stats().await.unwrap();
                    assert_eq!(stats.documents, 1, "Each context should have exactly 1 document");
                })
            })
            .collect();

        for handle in handles {
            handle.await.unwrap();
        }
    }
}

pub mod llm_helpers;
