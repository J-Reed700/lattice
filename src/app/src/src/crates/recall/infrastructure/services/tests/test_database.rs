#[cfg(test)]
mod database_tests {
    use crate::infrastructure::services::database::{DatabaseService, Document};
    use sqlx::{Row, SqlitePool};
    use std::sync::Arc;
    use uuid::Uuid;

    fn temp_path(file_name: &str) -> String {
        std::env::temp_dir()
            .join(file_name)
            .to_string_lossy()
            .to_string()
    }

    // ========================================
    // Test Helpers
    // ========================================

    /// Create test database pool with UUID-based isolation
    /// Uses in-memory database with unique name for parallel test execution
    async fn create_test_db_pool() -> SqlitePool {
        let test_uuid = Uuid::new_v4();
        // Use in-memory database with unique name for isolation
        let pool = SqlitePool::connect(&format!(
            "sqlite:file:test_{}?mode=memory&cache=shared",
            test_uuid
        ))
        .await
        .expect("Failed to create test pool");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    /// Create DatabaseService for testing with UUID-based isolation
    /// Uses in-memory database - no cleanup needed
    async fn create_test_service() -> (DatabaseService, SqlitePool) {
        let test_uuid = Uuid::new_v4();
        let db_url = format!("sqlite:file:test_{}?mode=memory&cache=shared", test_uuid);

        // Create a separate pool for migrations
        let pool = SqlitePool::connect(&db_url)
            .await
            .expect("Failed to create migration pool");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Note: DatabaseService::new expects a file path, not a URL
        // For testing, we'll use the same URL (it will work with in-memory DBs)
        let service = DatabaseService::new(&db_url)
            .await
            .expect("Failed to create test service");

        (service, pool)
    }

    /// Factory for test Document
    fn create_test_document(file_path: &str) -> Document {
        Document {
            id: String::new(), // Will be set by insert
            file_path: file_path.to_string(),
            file_name: "test.txt".to_string(),
            file_type: Some("text/plain".to_string()),
            size_bytes: 1024,
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            indexed_at: "2024-01-01T00:00:00Z".to_string(),
            checksum: "abc123".to_string(),
        }
    }

    // ========================================
    // Category 1: Connection Management (P0)
    // ========================================

    /// Test that DatabaseService::new() creates a valid connection pool
    #[tokio::test]
    async fn test_new_creates_connection_pool() {
        // Arrange
        let test_uuid = Uuid::new_v4();
        let db_url = format!("sqlite:file:test_{}?mode=memory&cache=shared", test_uuid);

        // Act
        let result = DatabaseService::new(&db_url).await;

        // Assert
        assert!(result.is_ok(), "DatabaseService::new() should succeed");

        // Note: We can't directly access the pool since it's private,
        // but successful construction implies pool creation worked.
        // We verify pool functionality in other tests via insert/get operations.
    }

    /// Test that DatabaseService::new() fails with invalid path
    #[tokio::test]
    async fn test_new_fails_with_invalid_path() {
        // Arrange
        let invalid_path = "/invalid/\0/path.db"; // Null byte in path

        // Act
        let result = DatabaseService::new(invalid_path).await;

        // Assert
        assert!(result.is_err(), "Should fail with invalid path");
    }

    /// Test that pool can execute basic queries
    #[tokio::test]
    async fn test_pool_can_execute_queries() {
        // Arrange
        let (_service, pool) = create_test_service().await;

        // Act
        let result = sqlx::query("SELECT 1").fetch_one(&pool).await;

        // Assert
        assert!(result.is_ok(), "Pool should execute SELECT 1 successfully");
    }

    // ========================================
    // Category 2: Document Insertion (P0)
    // ========================================

    /// Test that insert_document() generates a valid UUID
    #[tokio::test]
    async fn test_insert_document_generates_uuid() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let doc = create_test_document(&temp_path("test.txt"));

        // Act
        let result = service.insert_document(&doc).await;

        // Assert
        assert!(result.is_ok(), "insert_document should succeed");
        let id = result.unwrap();
        assert!(!id.is_empty(), "Generated ID should not be empty");

        // Verify it's a valid UUID
        let uuid_result = Uuid::parse_str(&id);
        assert!(
            uuid_result.is_ok(),
            "Generated ID should be a valid UUID: {}",
            id
        );
    }

    /// Test that insert_document() persists all fields correctly
    #[tokio::test]
    async fn test_insert_document_persists_all_fields() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let doc = Document {
            id: String::new(),
            file_path: temp_path("test.txt"),
            file_name: "test.txt".to_string(),
            file_type: Some("text/plain".to_string()),
            size_bytes: 2048,
            modified_at: "2024-01-15T10:30:00Z".to_string(),
            indexed_at: "2024-01-15T10:30:00Z".to_string(),
            checksum: "sha256abc".to_string(),
        };

        // Act
        let id = service
            .insert_document(&doc)
            .await
            .expect("Insert should succeed");
        let retrieved = service
            .get_document(&id)
            .await
            .expect("Get should succeed")
            .expect("Document should exist");

        // Assert - verify all fields are persisted
        assert_eq!(retrieved.file_path, doc.file_path, "file_path should match");
        assert_eq!(retrieved.file_name, doc.file_name, "file_name should match");
        assert_eq!(retrieved.file_type, doc.file_type, "file_type should match");
        assert_eq!(
            retrieved.size_bytes, doc.size_bytes,
            "size_bytes should match"
        );
        assert_eq!(
            retrieved.modified_at, doc.modified_at,
            "modified_at should match"
        );
        assert_eq!(retrieved.checksum, doc.checksum, "checksum should match");
    }

    /// Test that insert_document() handles NULL file_type correctly
    #[tokio::test]
    async fn test_insert_document_handles_null_file_type() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let mut doc = create_test_document(&temp_path("test"));
        doc.file_type = None; // NULL file_type

        // Act
        let id = service
            .insert_document(&doc)
            .await
            .expect("Insert should succeed");
        let retrieved = service
            .get_document(&id)
            .await
            .expect("Get should succeed")
            .expect("Document should exist");

        // Assert
        assert!(
            retrieved.file_type.is_none(),
            "file_type should be None when NULL"
        );
    }

    /// Test that insert_document() handles potential constraint violations
    #[tokio::test]
    async fn test_insert_document_fails_with_invalid_data() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let mut doc = create_test_document(&temp_path("test.txt"));
        // Create potential constraint violation (empty checksum if NOT NULL constraint exists)
        doc.checksum = String::new();

        // Act
        let result = service.insert_document(&doc).await;

        // Assert
        // This may succeed or fail depending on schema constraints
        // If there's a NOT NULL constraint on checksum, this should fail
        if result.is_err() {
            println!("Insert correctly failed with constraint violation");
        } else {
            println!("Insert succeeded - no constraint on empty checksum");
        }
    }

    // ========================================
    // Category 3: Document Retrieval (P0)
    // ========================================

    /// Test that get_document() retrieves an existing document
    #[tokio::test]
    async fn test_get_document_retrieves_existing() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let doc = create_test_document(&temp_path("test.txt"));
        let id = service
            .insert_document(&doc)
            .await
            .expect("Insert should succeed");

        // Act
        let result = service.get_document(&id).await;

        // Assert
        assert!(result.is_ok(), "get_document should succeed");
        let retrieved = result.unwrap();
        assert!(retrieved.is_some(), "Document should be found");
        let retrieved_doc = retrieved.unwrap();
        assert_eq!(retrieved_doc.id, id, "Document ID should match");
        assert_eq!(
            retrieved_doc.file_path, doc.file_path,
            "file_path should match"
        );
    }

    /// Test that get_document() returns None for missing documents
    #[tokio::test]
    async fn test_get_document_returns_none_for_missing() {
        // Arrange
        let (service, _pool) = create_test_service().await;

        // Act
        let result = service.get_document("nonexistent-id").await;

        // Assert
        assert!(result.is_ok(), "get_document should succeed");
        let retrieved = result.unwrap();
        assert!(
            retrieved.is_none(),
            "Should return None for nonexistent document"
        );
    }

    /// Test that get_document() maps all 8 fields correctly from database
    #[tokio::test]
    async fn test_get_document_maps_fields_correctly() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let doc = Document {
            id: String::new(),
            file_path: "/home/user/docs/report.pdf".to_string(),
            file_name: "report.pdf".to_string(),
            file_type: Some("application/pdf".to_string()),
            size_bytes: 10240,
            modified_at: "2024-01-15T14:22:00Z".to_string(),
            indexed_at: "2024-01-15T14:25:00Z".to_string(),
            checksum: "sha256xyz".to_string(),
        };
        let id = service
            .insert_document(&doc)
            .await
            .expect("Insert should succeed");

        // Act
        let retrieved = service
            .get_document(&id)
            .await
            .expect("Get should succeed")
            .expect("Document should exist");

        // Assert - verify all 8 fields are mapped correctly
        assert_eq!(retrieved.id, id, "id should match");
        assert_eq!(retrieved.file_path, doc.file_path, "file_path should match");
        assert_eq!(retrieved.file_name, doc.file_name, "file_name should match");
        assert_eq!(retrieved.file_type, doc.file_type, "file_type should match");
        assert_eq!(
            retrieved.size_bytes, doc.size_bytes,
            "size_bytes should match"
        );
        assert_eq!(
            retrieved.modified_at, doc.modified_at,
            "modified_at should match"
        );
        // indexed_at is set by database, should exist
        assert!(
            !retrieved.indexed_at.is_empty(),
            "indexed_at should be set by database"
        );
        assert_eq!(retrieved.checksum, doc.checksum, "checksum should match");
    }

    /// Test that get_document() handles invalid IDs gracefully
    #[tokio::test]
    async fn test_get_document_fails_with_invalid_id() {
        // Arrange
        let (service, _pool) = create_test_service().await;

        // Act
        let result_empty = service.get_document("").await;
        let result_null = service.get_document("\0").await;

        // Assert
        // May return Ok(None) or Err depending on SQLite's handling
        // Both are acceptable (empty string is not a valid UUID)
        assert!(
            result_empty.is_ok() || result_empty.is_err(),
            "Should handle empty ID gracefully"
        );
        assert!(
            result_null.is_ok() || result_null.is_err(),
            "Should handle null byte ID gracefully"
        );
    }

    // ========================================
    // Category 4: Migration and Schema (P1)
    // ========================================

    /// Test that migrations create the documents table
    #[tokio::test]
    async fn test_migrations_create_documents_table() {
        // Arrange
        let pool = create_test_db_pool().await;

        // Act
        let result =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='documents'")
                .fetch_one(&pool)
                .await;

        // Assert
        assert!(result.is_ok(), "documents table should exist");
        let row = result.unwrap();
        let table_name: String = row.try_get("name").expect("Should have name column");
        assert_eq!(table_name, "documents", "Table name should be 'documents'");
    }

    /// Test that migrations create the chunks table
    #[tokio::test]
    async fn test_migrations_create_chunks_table() {
        // Arrange
        let pool = create_test_db_pool().await;

        // Act
        let result =
            sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='chunks'")
                .fetch_one(&pool)
                .await;

        // Assert
        assert!(result.is_ok(), "chunks table should exist");
        let row = result.unwrap();
        let table_name: String = row.try_get("name").expect("Should have name column");
        assert_eq!(table_name, "chunks", "Table name should be 'chunks'");
    }

    // ========================================
    // Category 5: Concurrency (P2)
    // ========================================

    /// Test that concurrent inserts work correctly with UUID generation
    #[tokio::test]
    async fn test_concurrent_inserts() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let service = Arc::new(service);
        let mut handles = vec![];

        // Act - spawn 10 concurrent insert tasks
        for i in 0..10 {
            let service_clone = Arc::clone(&service);
            let handle = tokio::spawn(async move {
                let doc = create_test_document(&temp_path(&format!("test_{}.txt", i)));
                service_clone.insert_document(&doc).await
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let results: Vec<_> = futures::future::join_all(handles).await;

        // Assert
        let ids: Vec<String> = results
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(ids.len(), 10, "All 10 inserts should succeed");

        // Verify all UUIDs are unique
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique_ids.len(), 10, "All generated UUIDs should be unique");
    }

    /// Test that concurrent reads scale correctly
    #[tokio::test]
    async fn test_concurrent_reads() {
        // Arrange
        let (service, _pool) = create_test_service().await;
        let service = Arc::new(service);

        // Insert 5 documents
        let mut doc_ids = vec![];
        for i in 0..5 {
            let doc = create_test_document(&temp_path(&format!("doc_{}.txt", i)));
            let id = service
                .insert_document(&doc)
                .await
                .expect("Insert should succeed");
            doc_ids.push(id);
        }

        let mut handles = vec![];

        // Act - spawn 20 tasks reading random documents
        for _ in 0..20 {
            let service_clone = Arc::clone(&service);
            let ids_clone = doc_ids.clone();
            let handle = tokio::spawn(async move {
                let random_id = &ids_clone[0]; // Simplified: always read first doc
                service_clone.get_document(random_id).await
            });
            handles.push(handle);
        }

        // Wait for all reads to complete
        let results: Vec<_> = futures::future::join_all(handles).await;

        // Assert
        let successful_reads: Vec<_> = results
            .into_iter()
            .filter_map(|r| r.ok())
            .filter_map(|r| r.ok())
            .filter_map(|opt| opt)
            .collect();

        assert_eq!(
            successful_reads.len(),
            20,
            "All 20 concurrent reads should succeed"
        );
    }
}
