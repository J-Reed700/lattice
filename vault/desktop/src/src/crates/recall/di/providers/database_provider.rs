//! Database Provider - SQLite Connection and Repository Management
//!
//! This provider manages SQLite database connections and repository instances
//! following the "bricks and studs" philosophy.
//!
//! # Architecture
//!
//! - **Brick**: Self-contained database management module
//! - **Studs**: Repository trait objects (DocumentRepositoryPort, ChunkRepositoryPort, etc.)
//! - **Regeneratable**: Can be rebuilt from this specification alone
//!
//! # Purpose
//!
//! - Initialize SQLite connection pool with proper configuration
//! - Run database migrations on startup
//! - Provide repository instances with shared pool
//! - Maintain clean separation between infrastructure and application layers
//!
//! # Contract
//!
//! **Input**: Database URL string (e.g., "sqlite://path/to/db.sqlite")
//! **Output**: DatabaseProvider with initialized repositories
//! **Side Effects**:
//! - Creates/connects to SQLite database
//! - Runs schema migrations
//! - Enables WAL mode and foreign keys
//!
//! # Example Usage
//!
//! ```rust
//! use crate::di::providers::DatabaseProvider;
//!
//! // Initialize provider
//! let provider = DatabaseProvider::new("sqlite://app.db").await?;
//!
//! // Access repositories
//! let doc_repo = provider.document_repository();
//! let documents = doc_repo.find_all(filter).await?;
//!
//! // Access pool directly if needed
//! let pool = provider.pool();
//! ```

use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepositoryPort, RepositoryPort,
};
use crate::domain::entities::{chunk::Chunk, tag::Tag, Document};
use crate::infrastructure::persistence::database::{initialize_database, run_migrations};
use crate::infrastructure::persistence::repositories::{
    ChunkRepository, DocumentRepository, TagRepository,
};
use crate::shared::error::{AppError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

/// Database provider managing SQLite connections and repositories.
///
/// This provider follows the "bricks and studs" philosophy:
/// - Self-contained database management
/// - Clear public interface via repository traits
/// - Shared connection pool across all repositories
/// - Initialized and ready to use
///
/// # Thread Safety
///
/// This provider is thread-safe and can be shared across async tasks.
/// SQLitePool internally manages connection pooling and synchronization.
#[derive(Clone)]
pub struct DatabaseProvider {
    /// SQLite connection pool shared across all repositories
    pool: SqlitePool,

    /// Document repository for document-specific operations
    document_repo: Arc<dyn RepositoryPort<Document>>,

    /// Chunk repository for text chunk operations
    chunk_repo: Arc<dyn ChunkRepositoryPort>,

    /// Tag repository for tag operations
    tag_repo: Arc<dyn RepositoryPort<Tag>>,
}

impl DatabaseProvider {
    /// Create a new database provider with initialized repositories.
    ///
    /// This method performs the following initialization steps:
    /// 1. Creates SQLite connection pool with optimal configuration
    /// 2. Enables WAL mode and foreign key constraints
    /// 3. Runs database schema migrations
    /// 4. Initializes all repository instances
    ///
    /// # Arguments
    ///
    /// * `database_url` - SQLite connection string (e.g., "sqlite://app.db" or "sqlite::memory:")
    ///
    /// # Returns
    ///
    /// Initialized DatabaseProvider ready for use.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if connection fails
    /// - `AppError::Database` if migrations fail
    /// - `AppError::InvalidInput` if database URL is malformed
    ///
    /// # Example
    ///
    /// ```rust
    /// // Production database
    /// let provider = DatabaseProvider::new("sqlite://data/app.db").await?;
    ///
    /// // In-memory for testing
    /// let provider = DatabaseProvider::new("sqlite::memory:").await?;
    /// ```
    pub async fn new(database_url: &str) -> Result<Self> {
        // Parse connection options from URL
        let connect_options = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| {
                AppError::InvalidInput(format!("Invalid database URL: {}", e))
            })?
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5))
            .pragma("foreign_keys", "ON")
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL")
            .pragma("cache_size", "-20000") // 20MB cache
            .pragma("temp_store", "MEMORY");

        // Create connection pool with optimal settings
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(600)) // 10 minutes
            .max_lifetime(Duration::from_secs(1800)) // 30 minutes
            .connect_with(connect_options)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to connect to database: {}", e))
            })?;

        // Verify foreign keys are enabled
        let fk_enabled: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to check foreign key status: {}", e))
            })?;

        if fk_enabled.0 != 1 {
            return Err(AppError::Database(
                "Failed to enable foreign key constraints".to_string(),
            ));
        }

        // Initialize database schema (creates tables if not exist)
        initialize_database(&pool).await.map_err(|e| {
            AppError::Database(format!("Failed to initialize database schema: {}", e))
        })?;

        // Run migrations to upgrade schema if needed
        run_migrations(&pool).await.map_err(|e| {
            AppError::Database(format!("Failed to run database migrations: {}", e))
        })?;

        // Initialize repositories with shared pool
        let document_repo: Arc<dyn RepositoryPort<Document>> =
            Arc::new(DocumentRepository::new(pool.clone()));
        let chunk_repo: Arc<dyn ChunkRepositoryPort> =
            Arc::new(ChunkRepository::new(pool.clone()));
        let tag_repo: Arc<dyn RepositoryPort<Tag>> =
            Arc::new(TagRepository::new(pool.clone()));

        tracing::info!("Database provider initialized successfully");

        Ok(Self {
            pool,
            document_repo,
            chunk_repo,
            tag_repo,
        })
    }

    /// Get the SQLite connection pool.
    ///
    /// Use this when you need direct database access for custom queries
    /// or operations not covered by repository interfaces.
    ///
    /// # Returns
    ///
    /// Reference to the SQLite connection pool.
    ///
    /// # Example
    ///
    /// ```rust
    /// let pool = provider.pool();
    /// let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
    ///     .fetch_one(pool)
    ///     .await?;
    /// ```
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get the document repository.
    ///
    /// Provides access to document-specific operations including:
    /// - CRUD operations (create, read, update, delete)
    /// - File path lookups
    /// - Document existence checks
    ///
    /// # Returns
    ///
    /// Arc-wrapped trait object implementing RepositoryPort<Document>.
    ///
    /// # Example
    ///
    /// ```rust
    /// let repo = provider.document_repository();
    /// let doc = repo.find_by_id("doc-123").await?;
    /// ```
    pub fn document_repository(&self) -> Arc<dyn RepositoryPort<Document>> {
        Arc::clone(&self.document_repo)
    }

    /// Get the chunk repository.
    ///
    /// Provides access to text chunk operations including:
    /// - Finding chunks by document ID
    /// - Deleting chunks by document (cascade delete)
    ///
    /// # Returns
    ///
    /// Arc-wrapped trait object implementing ChunkRepositoryPort.
    ///
    /// # Example
    ///
    /// ```rust
    /// let repo = provider.chunk_repository();
    /// let chunks = repo.find_by_document("doc-123").await?;
    /// ```
    pub fn chunk_repository(&self) -> Arc<dyn ChunkRepositoryPort> {
        Arc::clone(&self.chunk_repo)
    }

    /// Get the tag repository.
    ///
    /// Provides access to tag operations including:
    /// - CRUD operations for tags
    /// - Tag name lookups
    ///
    /// # Returns
    ///
    /// Arc-wrapped trait object implementing RepositoryPort<Tag>.
    ///
    /// # Example
    ///
    /// ```rust
    /// let repo = provider.tag_repository();
    /// let tag = repo.find_by_id("tag-123").await?;
    /// ```
    pub fn tag_repository(&self) -> Arc<dyn RepositoryPort<Tag>> {
        Arc::clone(&self.tag_repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_provider_initialization() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        assert!(!provider.pool().is_closed());
    }

    #[tokio::test]
    async fn test_database_provider_repositories() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        // Test each repository accessor returns valid trait objects
        let _doc_repo = provider.document_repository();
        let _chunk_repo = provider.chunk_repository();
        let _tag_repo = provider.tag_repository();

        // Verify pool is accessible
        let pool = provider.pool();
        assert!(!pool.is_closed());
    }

    #[tokio::test]
    async fn test_database_provider_invalid_url() {
        let result = DatabaseProvider::new("invalid://url").await;
        assert!(result.is_err());

        if let Err(AppError::InvalidInput(msg)) = result {
            assert!(msg.contains("Invalid database URL"));
        } else {
            panic!("Expected InvalidInput error");
        }
    }

    #[tokio::test]
    async fn test_database_provider_foreign_keys_enabled() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        let pool = provider.pool();
        let fk_enabled: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(pool)
            .await
            .expect("Failed to query foreign keys");

        assert_eq!(fk_enabled.0, 1, "Foreign keys should be enabled");
    }

    #[tokio::test]
    async fn test_database_provider_wal_mode() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        let pool = provider.pool();
        let journal_mode: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(pool)
            .await
            .expect("Failed to query journal mode");

        assert_eq!(
            journal_mode.0.to_uppercase(),
            "WAL",
            "Journal mode should be WAL"
        );
    }

    #[tokio::test]
    async fn test_database_provider_schema_initialized() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        let pool = provider.pool();

        // Verify critical tables exist
        let tables = vec!["documents", "chunks", "tags"];
        for table in tables {
            let count: (i32,) = sqlx::query_as(&format!(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                table
            ))
            .fetch_one(pool)
            .await
            .expect("Failed to check table existence");

            assert_eq!(count.0, 1, "Table '{}' should exist", table);
        }
    }

    #[tokio::test]
    async fn test_database_provider_migrations_run() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        let pool = provider.pool();

        // Verify schema_version table exists
        let count: (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
        )
        .fetch_one(pool)
        .await
        .expect("Failed to check schema_version table");

        assert_eq!(count.0, 1, "schema_version table should exist");

        // Verify at least one migration was recorded
        let version: (i32,) = sqlx::query_as("SELECT MAX(version) FROM schema_version")
            .fetch_one(pool)
            .await
            .expect("Failed to query schema version");

        assert!(version.0 > 0, "At least one migration should be recorded");
    }

    #[tokio::test]
    async fn test_database_provider_clone() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        // Clone should work (Arc is cloneable)
        let cloned = provider.clone();

        // Both should access same pool
        assert_eq!(
            provider.pool() as *const _,
            cloned.pool() as *const _,
            "Cloned provider should share same pool"
        );
    }

    #[tokio::test]
    async fn test_database_provider_concurrent_access() {
        let provider = DatabaseProvider::new("sqlite::memory:")
            .await
            .expect("Failed to initialize database provider");

        // Spawn multiple concurrent tasks accessing repositories
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let provider = provider.clone();
                tokio::spawn(async move {
                    let _doc_repo = provider.document_repository();
                    let _chunk_repo = provider.chunk_repository();
                    let _tag_repo = provider.tag_repository();
                    // Just accessing repositories should work without panics
                })
            })
            .collect();

        for handle in handles {
            handle.await.expect("Task should complete successfully");
        }
    }
}
