//! SQLite-backed document scope persistence shared by import workflows.

use lattice_core::application::ports::document_scope::DocumentScopePort;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

// Bound JSON encoding and SQLite's per-statement work without one worker
// round trip per document. All batches still share a single transaction.
const SCOPE_BATCH_SIZE: usize = 500;

pub struct SqliteDocumentScope {
    pool: SqlitePool,
}
impl SqliteDocumentScope {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl DocumentScopePort for SqliteDocumentScope {
    async fn space_exists(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM conversation_spaces WHERE id = ?)",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?)
    }
    async fn conversation_exists(&self, id: &str) -> Result<bool> {
        Ok(
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)",
            )
            .bind(id)
            .fetch_one(&self.pool)
            .await?,
        )
    }
    async fn assign_documents(&self, document_ids: &[String], space_id: &str) -> Result<()> {
        if document_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        let now = chrono::Utc::now().to_rfc3339();
        for ids in document_ids.chunks(SCOPE_BATCH_SIZE) {
            let scope = serde_json::to_string(ids)?;
            sqlx::query(
                "INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
                 SELECT value, ?, ? FROM json_each(?)",
            )
            .bind(space_id)
            .bind(&now)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn set_conversation_owner(
        &self,
        document_ids: &[String],
        conversation_id: Option<&str>,
    ) -> Result<()> {
        if document_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for ids in document_ids.chunks(SCOPE_BATCH_SIZE) {
            let scope = serde_json::to_string(ids)?;
            sqlx::query(
                "UPDATE documents SET owner_conversation_id = ?
                 WHERE id IN (SELECT value FROM json_each(?))",
            )
            .bind(conversation_id)
            .bind(scope)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn conversation_owner(&self, document_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT owner_conversation_id FROM documents WHERE id = ?",
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    async fn documents_owned_by_conversation(&self, conversation_id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT id FROM documents WHERE owner_conversation_id = ? ORDER BY rowid",
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn orphaned_conversation_owned_documents(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT d.id FROM documents d
             WHERE d.owner_conversation_id IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM conversations c WHERE c.id = d.owner_conversation_id)
             ORDER BY d.rowid",
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn bulk_repository() -> (SqliteDocumentScope, Vec<String>) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let mut ids: Vec<_> = (0..1001).map(|index| format!("doc-{index}")).collect();
        ids.push("quoted\"; --\n雪".into());
        sqlx::query(
            "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum)
             SELECT value, '/bulk/' || value, value, 1, '2026-10-25T00:00:00Z', 'checksum'
             FROM json_each(?)",
        )
        .bind(serde_json::to_string(&ids).unwrap())
        .execute(&pool)
        .await
        .unwrap();
        (SqliteDocumentScope::new(pool), ids)
    }

    #[tokio::test]
    async fn bulk_scope_changes_use_bounded_statement_counts() {
        use crate::tests::common::query_counts::count_writes;

        let (repo, ids) = bulk_repository().await;
        let mut repeated = ids.clone();
        repeated.push(ids.first().unwrap().clone());
        let (assigned, writes) =
            count_writes(repo.assign_documents(&repeated, "space_general")).await;
        assigned.unwrap();
        assert!(
            (1..=4).contains(&writes),
            "bulk assignment used {writes} statements"
        );
        repo.assign_documents(&repeated, "space_general")
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(count, ids.len() as i64);

        // Missing document IDs remain a no-op for ownership updates.
        repeated.push("missing".into());
        let (owned, writes) =
            count_writes(repo.set_conversation_owner(&repeated, Some("chat"))).await;
        owned.unwrap();
        assert!(
            (1..=4).contains(&writes),
            "bulk ownership used {writes} statements"
        );
        assert_eq!(
            repo.documents_owned_by_conversation("chat").await.unwrap(),
            ids
        );
        repo.set_conversation_owner(&repeated, None).await.unwrap();
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());

        let (empty, writes) = count_writes(repo.assign_documents(&[], "missing-space")).await;
        empty.unwrap();
        assert_eq!(writes, 0);
    }

    #[tokio::test]
    async fn later_batch_failure_rolls_back_all_scope_changes() {
        let (repo, mut ids) = bulk_repository().await;
        ids.push("missing".into());
        assert!(repo.assign_documents(&ids, "space_general").await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&repo.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "a later foreign-key failure must undo earlier batches"
        );

        sqlx::raw_sql(
            "UPDATE documents SET owner_conversation_id = 'prior' WHERE id = 'doc-0';
             CREATE TRIGGER fail_late_owner BEFORE UPDATE OF owner_conversation_id ON documents
             WHEN NEW.id = 'doc-1000' AND NEW.owner_conversation_id = 'chat'
             BEGIN SELECT RAISE(ABORT, 'injected ownership failure'); END;",
        )
        .execute(&repo.pool)
        .await
        .unwrap();
        assert!(repo
            .set_conversation_owner(&ids, Some("chat"))
            .await
            .is_err());
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            repo.conversation_owner("doc-0").await.unwrap().as_deref(),
            Some("prior")
        );
        let failures: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&repo.pool)
            .await
            .unwrap();
        assert!(failures.is_empty());
    }

    #[tokio::test]
    async fn scope_validation_and_atomic_idempotent_assignment() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE conversation_spaces (id TEXT PRIMARY KEY);
            CREATE TABLE conversations (id TEXT PRIMARY KEY);
            CREATE TABLE documents (id TEXT PRIMARY KEY, owner_conversation_id TEXT);
            INSERT INTO documents VALUES ('first', NULL), ('second', NULL);
            CREATE TABLE document_space_memberships (document_id TEXT, space_id TEXT, created_at TEXT, PRIMARY KEY(document_id, space_id));
            INSERT INTO conversation_spaces VALUES ('space'); INSERT INTO conversations VALUES ('chat');
            CREATE TRIGGER fail_second BEFORE INSERT ON document_space_memberships WHEN NEW.document_id = 'second' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;")
            .execute(&pool).await.unwrap();
        let repo = SqliteDocumentScope::new(pool.clone());
        assert!(repo.space_exists("space").await.unwrap());
        assert!(!repo.space_exists("missing").await.unwrap());
        assert!(repo.conversation_exists("chat").await.unwrap());
        assert!(!repo.conversation_exists("missing").await.unwrap());
        let ids = vec!["first".into(), "second".into()];
        assert!(repo.assign_documents(&ids, "space").await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TRIGGER fail_second")
            .execute(&pool)
            .await
            .unwrap();
        repo.assign_documents(&ids, "space").await.unwrap();
        repo.assign_documents(&ids, "space").await.unwrap();
        repo.assign_documents(&[], "space").await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn conversation_ownership_is_stamped_cleared_and_swept() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY);
             CREATE TABLE documents (id TEXT PRIMARY KEY, owner_conversation_id TEXT);
             INSERT INTO conversations VALUES ('chat');
             INSERT INTO documents VALUES ('filed', NULL), ('attached', NULL), ('stray', NULL);",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteDocumentScope::new(pool.clone());

        repo.set_conversation_owner(&["attached".into()], Some("chat"))
            .await
            .unwrap();
        repo.set_conversation_owner(&["stray".into()], Some("deleted-chat"))
            .await
            .unwrap();
        assert_eq!(
            repo.documents_owned_by_conversation("chat").await.unwrap(),
            vec!["attached".to_string()]
        );
        // A live conversation's attachments are not orphans; a dead one's are.
        assert_eq!(
            repo.orphaned_conversation_owned_documents().await.unwrap(),
            vec!["stray".to_string()]
        );

        // "Add to library" releases the row without touching anything else.
        repo.set_conversation_owner(&["attached".into()], None)
            .await
            .unwrap();
        assert!(repo
            .documents_owned_by_conversation("chat")
            .await
            .unwrap()
            .is_empty());
        repo.set_conversation_owner(&[], Some("chat"))
            .await
            .unwrap();
    }
}
