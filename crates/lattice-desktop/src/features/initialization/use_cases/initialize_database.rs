//! Initialize Database Use Case
//!
//! Creates database schema and runs migrations for first-run setup.
//!
//! # Business Logic
//!
//! 1. Get application data directory
//! 2. Create database file if it doesn't exist
//! 3. Run schema initialization (CREATE TABLE IF NOT EXISTS)
//! 4. Verify schema version
//! 5. Return database path and metadata

use crate::features::initialization::dto::InitializeDatabaseResponseDto;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

/// Initialize database use case.
///
/// Ensures database schema is created and up to date.
pub struct InitializeDatabaseUseCase {
    /// SQLite database pool
    database: Database,
    /// Configured filesystem path (or `":memory:"` for in-memory test instances)
    db_path: String,
}

impl InitializeDatabaseUseCase {
    /// Create a new instance of the use case.
    pub fn new(database: Database) -> Self {
        Self {
            database,
            db_path: ":memory:".to_string(),
        }
    }

    /// Set the real database file path returned by `get_database_path`.
    pub fn with_db_path(mut self, db_path: impl Into<String>) -> Self {
        self.db_path = db_path.into();
        self
    }

    /// Execute the use case.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Database cannot be created
    /// - Schema initialization fails
    /// - Schema version cannot be determined
    pub async fn execute(&self) -> Result<InitializeDatabaseResponseDto> {
        let is_new_database = !self.has_schema_version_table().await?;

        // 1. Run database initialization only when the database is not yet initialized.
        //    Startup already runs migrations before Tauri commands execute.
        if is_new_database {
            crate::infrastructure::persistence::database::init::initialize_database(&self.database)
                .await
                .map_err(|e| AppError::Database(format!("Failed to initialize database: {}", e)))?;
        }

        // 2. Get schema version
        let schema_version = self.get_schema_version().await?;

        // 3. Get database path
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
        crate::features::initialization::repository::has_schema_version_table(&self.database).await
    }

    /// Get current schema version from database.
    async fn get_schema_version(&self) -> Result<i64> {
        crate::features::initialization::repository::schema_version(&self.database).await
    }

    /// Get database file path.
    fn get_database_path(&self) -> String {
        self.db_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

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
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool));

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert!(response.schema_version.is_some());
    }

    #[tokio::test]
    async fn test_initialize_database_creates_schema_version_table() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool.clone()));

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
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool));

        let result1 = use_case.execute().await.unwrap();
        let result2 = use_case.execute().await.unwrap();

        assert!(result1.success);
        assert!(result2.success);
        assert_eq!(result1.schema_version, result2.schema_version);
    }

    #[tokio::test]
    async fn test_initialize_database_returns_schema_version() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool));

        let result = use_case.execute().await.unwrap();

        assert!(result.schema_version.is_some());
        let version = result.schema_version.unwrap();
        assert!(version >= 1);
    }

    #[tokio::test]
    async fn test_initialize_database_detects_new_database() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool));

        let result = use_case.execute().await.unwrap();

        assert!(result.is_new_database);
    }

    #[tokio::test]
    async fn test_initialize_database_reports_configured_path_and_skips_redundant_migration() {
        let pool = create_test_pool().await;
        let use_case = InitializeDatabaseUseCase::new(Database::new(pool))
            .with_db_path("/var/lib/lattice/lattice.db");

        let first = use_case.execute().await.unwrap();
        assert!(first.is_new_database);
        assert_eq!(first.database_path, "/var/lib/lattice/lattice.db");

        let second = use_case.execute().await.unwrap();
        assert!(!second.is_new_database);
        assert_eq!(second.database_path, "/var/lib/lattice/lattice.db");
        assert_eq!(first.schema_version, second.schema_version);
    }
}
