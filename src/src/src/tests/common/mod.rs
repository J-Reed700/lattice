//! Common test utilities for plugin smoke tests
//!
//! This module provides test harness infrastructure that matches the current
//! production container architecture, not the legacy
//! ServiceContainer (15 individual services).
//!
//! # Usage
//!
//! ```rust,no_run
//! use lattice::tests::common::setup_test_container;
//!
//! #[tokio::test]
//! async fn test_search_plugin() {
//!     let container = setup_test_container().await.unwrap();
//!     let result = search_commands::semantic_search(
//!         "test query".to_string(),
//!         Some(10),
//!         tauri::State::from(&container)
//!     ).await;
//!     assert!(result.is_ok());
//! }
//! ```

use crate::infrastructure::persistence::database::DatabaseConnection;
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;

/// Setup a test Container with in-memory database
///
/// This function creates a production-aligned Container using the same
/// Container::new() signature as main.rs, but with in-memory SQLite
/// and minimal configuration suitable for smoke testing.
///
/// # Returns
///
/// A fully initialized Container ready for plugin command testing
///
/// # Examples
///
/// ```rust,no_run
/// let container = setup_test_container().await.unwrap();
/// ```
pub async fn setup_test_container() -> Result<Container> {
    let db_pool = SqlitePool::connect(":memory:")
        .await
        .map_err(|e| crate::shared::error::AppError::Database(e.to_string()))?;

    sqlx::migrate!("./migrations")
        .run(&db_pool)
        .await
        .map_err(|e| {
            crate::shared::error::AppError::Database(format!("Migration failed: {}", e))
        })?;

    let temp_dir = std::env::temp_dir().join(format!("recall_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| crate::shared::error::AppError::Io {
        message: e.to_string(),
        kind: format!("{:?}", e.kind()),
    })?;

    let temp_db_path = temp_dir.join("test.db");

    let db_conn = Arc::new(DatabaseConnection::new(temp_db_path).await?);

    Container::new(
        db_pool,
        db_conn,
        None,                     // No embedding model for smoke tests
        "http://localhost:11434", // Ollama default (won't be called in smoke tests)
        "llama2",                 // Default model
        temp_dir,
    )
    .await
}

/// Cleanup test resources
///
/// Call this after tests complete to remove temporary directories
pub fn cleanup_test_resources(data_dir: &PathBuf) -> Result<()> {
    if data_dir.starts_with(std::env::temp_dir()) {
        std::fs::remove_dir_all(data_dir).map_err(|e| crate::shared::error::AppError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_initialization() {
        let container = setup_test_container().await;
        assert!(
            container.is_ok(),
            "Container should initialize successfully"
        );
    }
}
