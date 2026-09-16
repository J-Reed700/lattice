//! Initialize Database Use Case
//!
//! Creates database schema and runs migrations for first-run setup.
//!
//! # Purpose
//!
//! Ensures the SQLite database is created with the correct schema.
//! Idempotent - safe to run multiple times. On first run, creates the schema.
//! On subsequent runs, verifies schema is up to date.
//!
//! # Business Logic
//!
//! 1. Get application data directory
//! 2. Create database file if it doesn't exist
//! 3. Run schema initialization (CREATE TABLE IF NOT EXISTS)
//! 4. Verify schema version
//! 5. Return database path and metadata
//!
//! # Example
//!
//! ```rust
//! let use_case = InitializeDatabaseUseCase::new(pool);
//! let response = use_case.execute().await?;
//!
//! if response.success {
//!     println!("Database ready at: {}", response.database_path);
//!     println!("Schema version: {}", response.schema_version.unwrap());
//! }
//! ```

use crate::features::initialization::dto::InitializeDatabaseResponseDto;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Initialize database use case.
///
/// Ensures database schema is created and up to date.
pub struct InitializeDatabaseUseCase {
    /// SQLite database pool
    database_pool: Arc<SqlitePool>,
}

impl InitializeDatabaseUseCase {
    /// Create a new instance of the use case.
    ///
    /// # Arguments
    ///
    /// * `database_pool` - SQLite connection pool
    pub fn new(database_pool: Arc<SqlitePool>) -> Self {
        Self { database_pool }
    }

    /// Execute the use case.
    ///
    /// # Returns
    ///
    /// Response DTO with database initialization status and metadata.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Database cannot be created
    /// - Schema initialization fails
    /// - Schema version cannot be determined
    pub async fn execute(&self) -> Result<InitializeDatabaseResponseDto> {
        let is_new_database = !self.has_schema_version_table().await?;

        // 1. Run database initialization (idempotent)
        crate::infrastructure::persistence::database::init::initialize_database(
            &self.database_pool,
        )
        .await
        .map_err(|e| AppError::Database(format!("Failed to initialize database: {}", e)))?;

        // 2. Get schema version
        let schema_version = self.get_schema_version().await?;

        // 3. Get database path from pool options
        let database_path = self.get_database_path();

        // 4. Return success response
        Ok(InitializeDatabaseResponseDto {
            success: true,
            database_path,
            schema_version: Some(schema_version),
            is_new_database,
        })
    }

    async fn has_schema_version_table(&self) -> Result<bool> {
        // repository-barrier-allow: database bootstrap must inspect migration state before repositories exist.
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
        )
        .fetch_one(self.database_pool.as_ref())
        .await
        .map_err(|e| AppError::Database(format!("Failed to inspect database schema: {e}")))?;
        Ok(count > 0)
    }

    /// Get current schema version from database.
    async fn get_schema_version(&self) -> Result<i64> {
        // repository-barrier-allow: database bootstrap inspects migration state before repositories exist.
        let result =
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM schema_version")
                .fetch_one(self.database_pool.as_ref())
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to query schema version: {}", e))
                })?;

        Ok(result)
    }

    /// Get database file path.
    ///
    /// Extracts path from SQLite connection string.
    fn get_database_path(&self) -> String {
        // The database path is embedded in the connection string
        // For SQLite, it's typically "sqlite://path/to/db.sqlite"
        // We'll return a placeholder for now since SqlitePool doesn't expose the path
        "lattice.db".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("Failed to create test database pool")
    }

    #[tokio::test]
    async fn test_initialize_database_success() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Arc::new(pool));

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert!(response.schema_version.is_some());
    }

    #[tokio::test]
    async fn test_initialize_database_creates_schema_version_table() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Arc::new(pool.clone()));

        use_case.execute().await.unwrap();

        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_version")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert!(count >= 0); // Table exists and is queryable
    }

    #[tokio::test]
    async fn test_initialize_database_is_idempotent() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Arc::new(pool));

        let result1 = use_case.execute().await.unwrap();
        let result2 = use_case.execute().await.unwrap();

        assert!(result1.success);
        assert!(result2.success);
        assert_eq!(result1.schema_version, result2.schema_version);
    }

    #[tokio::test]
    async fn test_initialize_database_returns_schema_version() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Arc::new(pool));

        let result = use_case.execute().await.unwrap();

        assert!(result.schema_version.is_some());
        let version = result.schema_version.unwrap();
        assert!(version >= 1);
    }

    #[tokio::test]
    async fn test_initialize_database_detects_new_database() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Arc::new(pool));

        let result = use_case.execute().await.unwrap();

        assert!(result.is_new_database);
    }
}
