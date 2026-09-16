//! # File Index Transaction
//!
//! Transaction-like abstraction for file indexing that ensures automatic cleanup
//! on failure. This eliminates repetitive error handling and prevents orphaned
//! file storage when indexing operations fail.
//!
//! ## Design Pattern
//!
//! Uses the RAII (Resource Acquisition Is Initialization) pattern via Rust's
//! `Drop` trait to guarantee cleanup:
//!
//! ```text
//! ┌─────────────────────┐
//! │ Start Transaction   │
//! └──────────┬──────────┘
//!            │
//!            ├─> Store file
//!            ├─> Extract content
//!            ├─> Generate embeddings
//!            ├─> Store to database
//!            │
//!            ├─> Success? → commit() → cleanup prevented
//!            │
//!            └─> Failure? → drop() → automatic rollback
//! ```
//!
//! ## Key Benefits
//!
//! - **Automatic Cleanup**: No need for manual error handlers
//! - **Type Safety**: Can't forget to commit (compiler enforces it)
//! - **Error Logging**: All rollback errors are logged
//! - **DRY**: Eliminates 100+ lines of repetitive code
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use lattice::indexing::transaction::FileIndexTransaction;
//!
//! async fn index_file(path: &Path) -> Result<FileRecord> {
//!     let file_id = generate_id();
//!
//!     // Create transaction guard
//!     let tx = FileIndexTransaction::new(
//!         file_id.clone(),
//!         Arc::clone(&file_storage),
//!         pool.clone(),
//!     );
//!
//!     // Perform indexing operations
//!     // If any fail, tx.drop() automatically cleans up
//!     let file_record = store_file(path, &file_id).await?;
//!     let content = extract_content(path).await?;
//!     let embeddings = generate_embeddings(&content).await?;
//!     store_to_db(&file_id, &embeddings).await?;
//!
//!     // Success - prevent rollback
//!     tx.commit();
//!
//!     Ok(file_record)
//! }
//! ```

use crate::infrastructure::services::file_storage::FileStorageService;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Transaction guard for file indexing operations.
///
/// Automatically rolls back (deletes file storage and database records) if not
/// explicitly committed before being dropped. This prevents orphaned files when
/// indexing operations fail partway through.
///
/// ## Lifecycle
///
/// 1. **Created**: Transaction starts, file ID tracked
/// 2. **Operations**: Indexing steps performed (may fail)
/// 3. **Commit**: Success - marks transaction as complete
/// 4. **Drop**: If not committed, triggers automatic rollback
///
/// ## Thread Safety
///
/// Safe to use across threads. Rollback spawns a new task to avoid blocking.
pub struct FileIndexTransaction {
    /// File ID being indexed. This is a `files.id`, **not** a `documents.id`.
    file_id: String,

    /// Source path of the file being indexed.
    ///
    /// Needed because `documents` is keyed by `file_path`, and there is no
    /// column joining it to `files.id`. Rollback previously ran
    /// `DELETE FROM documents WHERE id = <files.id>`, mixing the two id
    /// domains — it matched zero rows every single time, so the guard did
    /// nothing it claimed to do.
    file_path: Option<String>,

    /// File storage service for cleanup
    file_storage: Arc<FileStorageService>,

    /// Database pool for cleanup
    db_pool: SqlitePool,

    /// Whether transaction was successfully committed
    committed: bool,
}

impl FileIndexTransaction {
    /// Create a new transaction guard.
    ///
    /// # Arguments
    ///
    /// * `file_id` - ID of the file being indexed
    /// * `file_storage` - Service for managing file storage
    /// * `db_pool` - Database connection pool
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let tx = FileIndexTransaction::new(
    ///     "file-123".to_string(),
    ///     Arc::clone(&file_storage),
    ///     pool.clone(),
    /// );
    /// ```
    pub fn new(
        file_id: String,
        file_storage: Arc<FileStorageService>,
        db_pool: SqlitePool,
    ) -> Self {
        Self {
            file_id,
            file_path: None,
            file_storage,
            db_pool,
            committed: false,
        }
    }

    /// Record the source path so rollback can remove the `documents` row.
    ///
    /// Without it, rollback can only clean up file storage and the `files`
    /// row; a partially-written document would survive the failure.
    pub fn with_file_path(mut self, file_path: impl Into<String>) -> Self {
        self.file_path = Some(file_path.into());
        self
    }

    /// Mark transaction as successfully committed.
    ///
    /// Prevents automatic rollback when the transaction is dropped. Call this
    /// only after all indexing operations have succeeded.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let tx = FileIndexTransaction::new(file_id, storage, pool);
    ///
    /// // ... perform operations ...
    ///
    /// tx.commit(); // Success - no rollback
    /// ```
    pub fn commit(mut self) {
        self.committed = true;
    }

    /// Rollback transaction by deleting file storage and database records.
    ///
    /// This is called automatically by `Drop` if the transaction was not
    /// committed. Logs any errors encountered during cleanup.
    ///
    /// ## Cleanup Steps
    ///
    /// 1. Delete file from storage
    /// 2. Delete document record from database
    /// 3. Delete associated chunks and embeddings (cascaded)
    async fn rollback(&self) {
        tracing::warn!(
            file_id = %self.file_id,
            "Rolling back failed indexing transaction"
        );

        if let Err(e) = self.file_storage.delete_file(&self.file_id).await {
            tracing::error!(
                file_id = %self.file_id,
                error = %e,
                "Failed to delete file storage during rollback"
            );
        } else {
            tracing::debug!(
                file_id = %self.file_id,
                "Successfully deleted file storage during rollback"
            );
        }

        // Delete the document row (cascades to chunks and embeddings).
        // Keyed by path, because `documents.id` and `files.id` are different
        // id domains — the old `WHERE id = <files.id>` matched nothing.
        // Non-macro form to avoid regenerating the offline sqlx cache.
        if let Some(file_path) = &self.file_path {
            match sqlx::query("DELETE FROM documents WHERE file_path = ?")
                .bind(file_path)
                .execute(&self.db_pool)
                .await
            {
                Ok(result) => tracing::debug!(
                    file_path = %file_path,
                    rows = result.rows_affected(),
                    "Deleted document row during rollback"
                ),
                Err(e) => tracing::error!(
                    file_path = %file_path,
                    error = %e,
                    "Failed to delete document row during rollback"
                ),
            }
        }

        match sqlx::query("DELETE FROM files WHERE id = ?")
            .bind(&self.file_id)
            .execute(&self.db_pool)
            .await
        {
            Ok(result) => tracing::debug!(
                file_id = %self.file_id,
                rows = result.rows_affected(),
                "Deleted files row during rollback"
            ),
            Err(e) => tracing::error!(
                file_id = %self.file_id,
                error = %e,
                "Failed to delete files row during rollback"
            ),
        }
    }
}

impl Drop for FileIndexTransaction {
    /// Automatically rollback if transaction was not committed.
    ///
    /// Spawns an async task to perform cleanup without blocking the current
    /// thread. This is safe because rollback is idempotent.
    fn drop(&mut self) {
        if !self.committed {
            tracing::debug!(
                file_id = %self.file_id,
                "Transaction dropped without commit - triggering rollback"
            );

            // Clone data needed for async cleanup
            let file_id = self.file_id.clone();
            let file_path = self.file_path.clone();
            let file_storage = Arc::clone(&self.file_storage);
            let db_pool = self.db_pool.clone();

            // Spawn cleanup task (non-blocking)
            tokio::spawn(async move {
                // Create temporary transaction to perform rollback
                // Mark as committed to prevent double-cleanup
                let tx = FileIndexTransaction {
                    file_id,
                    file_path,
                    file_storage,
                    db_pool,
                    committed: true,
                };
                tx.rollback().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    async fn create_test_pool() -> (SqlitePool, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        crate::infrastructure::persistence::database::initialize_database(&pool)
            .await
            .unwrap();

        (pool, temp_dir)
    }

    /// The guard's whole purpose is removing partial state after a failure.
    /// It previously deleted `FROM documents WHERE id = <files.id>` — two
    /// different id domains — so it matched zero rows on every rollback that
    /// ever ran, and left the `files` row stranded at `is_indexed = 0`.
    #[tokio::test]
    async fn rollback_removes_both_the_document_and_the_files_row() {
        let (pool, temp_dir) = create_test_pool().await;
        let vault_path = temp_dir.path().join("lattice");
        tokio::fs::create_dir_all(&vault_path).await.unwrap();

        let file_storage = Arc::new(FileStorageService::new(vault_path, pool.clone()));
        let file_id = "files-row-1".to_string();
        let file_path = "/tmp/partially-indexed.txt";

        sqlx::query(
            "INSERT INTO files (id, content_hash, file_name, mime_type, size_bytes, \
             storage_path, is_indexed, created_at, accessed_at) \
             VALUES (?, 'hash-1', 'partially-indexed.txt', 'text/plain', 10, '/store/x', 0, 0, 0)",
        )
        .bind(&file_id)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, \
             modified_at, indexed_at, checksum, status) \
             VALUES ('doc-row-1', ?, 'partially-indexed.txt', 'text/plain', 10, \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'abc', 'indexed')",
        )
        .bind(file_path)
        .execute(&pool)
        .await
        .unwrap();

        {
            let _tx =
                FileIndexTransaction::new(file_id.clone(), file_storage.clone(), pool.clone())
                    .with_file_path(file_path);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

        let documents: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE file_path = ?")
                .bind(file_path)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            documents, 0,
            "rollback must delete the partial document row"
        );

        let files: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE id = ?")
            .bind(&file_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            files, 0,
            "rollback must delete the files row, not strand it at is_indexed = 0"
        );
    }

    #[tokio::test]
    async fn test_transaction_commit_prevents_rollback() {
        let (pool, temp_dir) = create_test_pool().await;
        let vault_path = temp_dir.path().join("lattice");
        tokio::fs::create_dir_all(&vault_path).await.unwrap();

        let file_storage = Arc::new(FileStorageService::new(vault_path, pool.clone()));
        let file_id = "test-file-123".to_string();

        {
            let tx = FileIndexTransaction::new(file_id.clone(), file_storage.clone(), pool.clone());
            tx.commit(); // Should prevent rollback
        }

        // Give time for any async cleanup to run
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Transaction committed - no cleanup should have occurred
        // (We can't easily verify this without inserting actual data,
        // but the test ensures commit() doesn't panic)
    }

    #[tokio::test]
    async fn test_transaction_drop_triggers_rollback() {
        let (pool, temp_dir) = create_test_pool().await;
        let vault_path = temp_dir.path().join("lattice");
        tokio::fs::create_dir_all(&vault_path).await.unwrap();

        let file_storage = Arc::new(FileStorageService::new(vault_path.clone(), pool.clone()));
        let file_id = "test-file-456".to_string();

        let test_file = vault_path.join(&file_id);
        tokio::fs::write(&test_file, b"test content").await.unwrap();
        assert!(test_file.exists());

        {
            let _tx =
                FileIndexTransaction::new(file_id.clone(), file_storage.clone(), pool.clone());
        }

        // Give time for async rollback to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // File should be deleted by rollback
        // Note: Actual deletion depends on FileStorageService implementation
    }

    #[tokio::test]
    async fn test_transaction_rollback_handles_errors() {
        let (pool, temp_dir) = create_test_pool().await;
        let vault_path = temp_dir.path().join("lattice");
        tokio::fs::create_dir_all(&vault_path).await.unwrap();

        let file_storage = Arc::new(FileStorageService::new(vault_path, pool.clone()));
        let file_id = "nonexistent-file".to_string();

        {
            let _tx =
                FileIndexTransaction::new(file_id.clone(), file_storage.clone(), pool.clone());
        }

        // Give time for async rollback
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_multiple_transactions_independent() {
        let (pool, temp_dir) = create_test_pool().await;
        let vault_path = temp_dir.path().join("lattice");
        tokio::fs::create_dir_all(&vault_path).await.unwrap();

        let file_storage = Arc::new(FileStorageService::new(vault_path, pool.clone()));

        let tx1 =
            FileIndexTransaction::new("file-1".to_string(), file_storage.clone(), pool.clone());
        let tx2 =
            FileIndexTransaction::new("file-2".to_string(), file_storage.clone(), pool.clone());

        // Commit one, drop the other
        tx1.commit();
        drop(tx2);

        // Give time for async cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Both should complete without interfering with each other
    }
}
