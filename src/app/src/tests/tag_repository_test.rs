#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

#[cfg(test)]
mod tag_repository_tests {
    use sqlx::SqlitePool;
    use lattice::infrastructure::persistence::repositories::TagRepository;
    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory SQLite pool");

        // Create tags table
        sqlx::query(
            r#"
            CREATE TABLE tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                color TEXT NOT NULL DEFAULT '#6366f1',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create tags table");

        // Create documents table for foreign keys
        sqlx::query(
            r#"
            CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                mime_type TEXT,
                size_bytes INTEGER NOT NULL,
                modified_at TEXT NOT NULL,
                indexed_at TEXT NOT NULL,
                checksum TEXT NOT NULL,
                status TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create documents table");

        // Create document_tags junction table
        sqlx::query(
            r#"
            CREATE TABLE document_tags (
                document_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (document_id, tag_id),
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
                FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create document_tags table");

        pool
    }

    #[tokio::test]
    async fn test_create_tag() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        let tag = repo
            .create("rust", Some("#ff6b6b"))
            .await
            .expect("Failed to create tag 'rust'");

        assert_eq!(tag.name().as_str(), "rust");
        assert_eq!(tag.color(), "#ff6b6b");
        assert!(!tag.id().as_str().is_empty());
    }

    #[tokio::test]
    async fn test_get_or_create_tag() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        // First call creates the tag
        let tag1 = repo
            .get_or_create("programming", None)
            .await
            .expect("Failed to create tag 'programming'");
        assert_eq!(tag1.name().as_str(), "programming");

        // Second call retrieves the same tag (case-insensitive)
        let tag2 = repo
            .get_or_create("Programming", None)
            .await
            .expect("Failed to get or create tag 'Programming'");
        assert_eq!(tag1.id(), tag2.id());
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        repo.create("typescript", None)
            .await
            .expect("Failed to create tag 'typescript'");

        // Case-insensitive search
        let tag = repo
            .find_by_name("TypeScript")
            .await
            .expect("Failed to find tag 'TypeScript'");
        assert_eq!(tag.expect("Tag should exist").name().as_str(), "typescript");
    }

    #[tokio::test]
    async fn test_get_all_tags() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        repo.create("tag1", None)
            .await
            .expect("Failed to create tag 'tag1'");
        repo.create("tag2", None)
            .await
            .expect("Failed to create tag 'tag2'");
        repo.create("tag3", None)
            .await
            .expect("Failed to create tag 'tag3'");

        let tags = repo.get_all().await.expect("Failed to get all tags");
        assert_eq!(tags.len(), 3);
    }

    #[tokio::test]
    async fn test_update_tag() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        let tag = repo
            .create("old-name", Some("#000000"))
            .await
            .expect("Failed to create tag 'old-name'");

        let updated = repo
            .update(tag.id().as_str(), Some("new-name"), Some("#ffffff"))
            .await
            .expect("Failed to update tag");

        assert_eq!(updated.name().as_str(), "new-name");
        assert_eq!(updated.color(), "#ffffff");
    }

    #[tokio::test]
    async fn test_delete_tag() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool);

        let tag = repo
            .create("to-delete", None)
            .await
            .expect("Failed to create tag 'to-delete'");

        repo.delete(tag.id().as_str())
            .await
            .expect("Failed to delete tag");

        let result = repo
            .find_by_name("to-delete")
            .await
            .expect("Query should succeed");
        assert!(result.is_none(), "Tag should not exist after deletion");
    }

    #[tokio::test]
    async fn test_add_tag_to_document() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool.clone());

        // Create a test document
        let doc_id = "test-doc-1";
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, modified_at, indexed_at, checksum, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(doc_id)
        .bind("/test/path")
        .bind("test.txt")
        .bind("text/plain")
        .bind(100)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .bind("checksum")
        .bind("indexed")
        .execute(&pool)
        .await
        .expect("Failed to insert test document");

        let tag = repo
            .create("document-tag", None)
            .await
            .expect("Failed to create tag 'document-tag'");

        repo.add_tag_to_document(doc_id, tag.id().as_str())
            .await
            .expect("Failed to add tag to document");

        let tags = repo
            .get_tags_for_document(doc_id)
            .await
            .expect("Failed to get tags for document");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name().as_str(), "document-tag");
    }

    #[tokio::test]
    async fn test_remove_tag_from_document() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool.clone());

        // Create a test document
        let doc_id = "test-doc-2";
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, modified_at, indexed_at, checksum, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(doc_id)
        .bind("/test/path2")
        .bind("test2.txt")
        .bind("text/plain")
        .bind(100)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .bind("checksum2")
        .bind("indexed")
        .execute(&pool)
        .await
        .expect("Failed to insert test document");

        let tag = repo
            .create("removable-tag", None)
            .await
            .expect("Failed to create tag 'removable-tag'");
        repo.add_tag_to_document(doc_id, tag.id().as_str())
            .await
            .expect("Failed to add tag to document");

        repo.remove_tag_from_document(doc_id, tag.id().as_str())
            .await
            .expect("Failed to remove tag from document");

        let tags = repo
            .get_tags_for_document(doc_id)
            .await
            .expect("Failed to get tags for document");
        assert_eq!(tags.len(), 0);
    }

    #[tokio::test]
    async fn test_bulk_add_tags() {
        let pool = setup_test_db().await;
        let repo = TagRepository::new(pool.clone());

        // Create a test document
        let doc_id = "test-doc-3";
        sqlx::query(
            r#"
            INSERT INTO documents (id, file_path, file_name, mime_type, size_bytes, modified_at, indexed_at, checksum, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(doc_id)
        .bind("/test/path3")
        .bind("test3.txt")
        .bind("text/plain")
        .bind(100)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .bind("checksum3")
        .bind("indexed")
        .execute(&pool)
        .await
        .expect("Failed to insert test document");

        let tag_names = vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()];

        let tags = repo
            .add_tags_to_document_by_names(doc_id, tag_names)
            .await
            .expect("Failed to add tags to document by names");

        assert_eq!(tags.len(), 3);

        let doc_tags = repo
            .get_tags_for_document(doc_id)
            .await
            .expect("Failed to get document tags");
        assert_eq!(doc_tags.len(), 3);
    }
}
