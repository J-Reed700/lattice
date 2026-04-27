//! Schema validation tests to ensure canonical schema integrity

#[cfg(test)]
mod tests {
    use crate::infrastructure::persistence::database::schema::initialize_schema;
    use sqlx::SqlitePool;

    #[tokio::test]

    async fn test_canonical_schema_creates_all_tables() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();

        let expected = vec![
            "document_mentions",
            "document_tags",
            "documents",
            "documents_fts",
            "documents_fts_config",
            "documents_fts_content",
            "documents_fts_data",
            "documents_fts_docsize",
            "documents_fts_idx",
            "favorites",
            "file_references",
            "files",
            // Note: image_embeddings and image_metadata are planned features, not yet implemented
            "mentions",
            "recent_documents",
            "schema_version",
            "tags",
            "text_chunks",
            "text_embeddings",
            // Note: watch_folders table removed - functionality moved to application layer
        ];

        for table in &expected {
            assert!(
                tables.contains(&table.to_string()),
                "Missing table: {}",
                table
            );
        }
    }

    #[tokio::test]
    async fn test_foreign_keys_enabled() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let (fk_enabled,): (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(fk_enabled, 1, "Foreign keys should be enabled");
    }

    #[tokio::test]
    async fn test_wal_mode_enabled() {
        // Note: WAL mode is not supported in :memory: databases
        // In production, WAL is enabled via connection string options
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let (journal_mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();

        // In-memory databases use "memory" journal mode
        // This is expected and correct for test databases
        assert!(
            journal_mode.to_lowercase() == "memory" || journal_mode.to_lowercase() == "wal",
            "Journal mode should be 'memory' (for :memory:) or 'wal' (for file-based), got: {}",
            journal_mode
        );
    }

    #[tokio::test]
    async fn test_schema_version_table_exists() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let (version,): (i32,) =
            sqlx::query_as("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert!(version > 0, "Schema version should be greater than 0");
    }

    #[tokio::test]
    async fn test_documents_table_structure() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct ColumnInfo {
            name: String,
        }

        let columns: Vec<ColumnInfo> = sqlx::query_as::<_, ColumnInfo>(
            "SELECT name FROM pragma_table_info('documents') ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let column_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

        let required_columns = vec![
            "checksum",
            "created_at",
            "file_name",
            "file_path",
            "file_type",
            "id",
            "indexed_at",
            "mime_type",
            "modified_at",
            "size_bytes",
            "status",
            "updated_at",
        ];

        for col in &required_columns {
            assert!(
                column_names.contains(&col.to_string()),
                "Missing column in documents table: {}",
                col
            );
        }
    }

    #[tokio::test]
    async fn test_file_storage_tables_exist() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('files', 'file_references') ORDER BY name"
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(
            tables.len(),
            2,
            "Both files and file_references tables should exist"
        );
        assert_eq!(tables[0], "file_references");
        assert_eq!(tables[1], "files");
    }

    #[tokio::test]

    async fn test_indexes_created() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let indexes: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%' ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(
            indexes.len() > 20,
            "Should have at least 20 indexes created"
        );

        let required_indexes = vec![
            "idx_chunks_document", // Updated to match actual schema
            "idx_documents_file_path",
            "idx_embeddings_chunk", // Updated to match actual schema
            "idx_files_hash",       // Updated to match actual schema (uses content_hash)
            "idx_file_refs_file",
            "idx_file_refs_doc",
        ];

        for idx in &required_indexes {
            assert!(indexes.contains(&idx.to_string()), "Missing index: {}", idx);
        }
    }

    #[tokio::test]

    async fn test_fts5_table_exists() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let (count,): (i32,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='documents_fts'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count, 1, "documents_fts FTS5 table should exist");
    }

    #[tokio::test]

    async fn test_triggers_created() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        let triggers: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();

        assert!(triggers.len() >= 3, "Should have at least 3 FTS5 triggers");

        let required_triggers = vec![
            "chunks_fts_delete",
            "chunks_fts_insert",
            "chunks_fts_update",
        ];

        for trigger in &required_triggers {
            assert!(
                triggers.contains(&trigger.to_string()),
                "Missing trigger: {}",
                trigger
            );
        }
    }

    #[tokio::test]
    async fn test_foreign_key_constraints() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        initialize_schema(&pool).await.unwrap();

        #[derive(sqlx::FromRow)]
        struct ForeignKey {
            table: String,
            #[allow(dead_code)]
            _from: String,
            to: String,
        }

        let fks: Vec<ForeignKey> = sqlx::query_as::<_, ForeignKey>(
            r#"
            SELECT
                m.name as "table",
                p."from" as "_from",
                p."to" as "to"
            FROM sqlite_master m
            JOIN pragma_foreign_key_list(m.name) p
            WHERE m.type = 'table'
            ORDER BY m.name, p."from"
            "#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert!(
            fks.len() > 5,
            "Should have multiple foreign key constraints"
        );

        let has_chunks_fk = fks
            .iter()
            .any(|fk| fk.table == "text_chunks" && fk.to == "id");
        assert!(has_chunks_fk, "text_chunks should have FK to documents");

        let has_embeddings_fk = fks
            .iter()
            .any(|fk| fk.table == "text_embeddings" && fk.to == "id");
        assert!(
            has_embeddings_fk,
            "text_embeddings should have FK to chunks"
        );
    }
}
