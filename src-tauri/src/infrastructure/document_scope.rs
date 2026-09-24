//! SQLite-backed document scope persistence shared by import workflows.

use crate::application::ports::document_scope::DocumentScopePort;
use crate::shared::error::Result;
use sqlx::SqlitePool;

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
        for id in document_ids {
            sqlx::query("INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at) VALUES (?, ?, ?)")
                .bind(id).bind(space_id).bind(&now).execute(&mut *tx).await?;
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
        for id in document_ids {
            sqlx::query("UPDATE documents SET owner_conversation_id = ? WHERE id = ?")
                .bind(conversation_id)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
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
