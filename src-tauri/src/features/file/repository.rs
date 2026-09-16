//! Read-only view of "which conversations cite this document".
//!
//! `conversation_documents` is written by the conversation slice; this is a
//! purpose-built read model over it, not a second `Conversation` entity, so
//! the Library can answer "where has this come up?" in one round trip.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::shared::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitingConversation {
    pub conversation_id: String,
    pub title: String,
    pub updated_at: String,
    pub passage_count: i64,
}

#[async_trait]
pub trait CitingConversationsRepositoryPort: Send + Sync {
    async fn find_citing_conversations(
        &self,
        document_id: &str,
        limit: i64,
    ) -> Result<Vec<CitingConversation>>;
}

pub struct SqliteCitingConversationsRepository {
    pool: SqlitePool,
}

impl SqliteCitingConversationsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CitingConversationsRepositoryPort for SqliteCitingConversationsRepository {
    async fn find_citing_conversations(
        &self,
        document_id: &str,
        limit: i64,
    ) -> Result<Vec<CitingConversation>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id            AS conversation_id,
                   c.title         AS title,
                   c.updated_at    AS updated_at,
                   COUNT(cd.id)    AS passage_count
            FROM conversation_documents cd
            INNER JOIN conversations c ON c.id = cd.conversation_id
            WHERE cd.document_id = ?
            GROUP BY c.id, c.title, c.updated_at
            ORDER BY c.updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(document_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("citing conversations: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|row| CitingConversation {
                conversation_id: row.try_get("conversation_id").unwrap_or_default(),
                title: row.try_get("title").unwrap_or_default(),
                updated_at: row.try_get("updated_at").unwrap_or_default(),
                passage_count: row.try_get("passage_count").unwrap_or(0),
            })
            .collect())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_document(pool: &SqlitePool, id: &str) {
        sqlx::query(
            r#"
            INSERT INTO documents
                (id, file_path, file_name, file_type, size_bytes, modified_at, checksum, status)
            VALUES (?, ?, ?, 'pdf', 0, '2026-01-01T00:00:00Z', 'sum', 'indexed')
            "#,
        )
        .bind(id)
        .bind(format!("/tmp/{id}.pdf"))
        .bind(format!("{id}.pdf"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_conversation(pool: &SqlitePool, id: &str, title: &str, updated_at: &str) {
        sqlx::query(
            r#"
            INSERT INTO conversations (id, title, model_name, created_at, updated_at)
            VALUES (?, ?, 'test-model', ?, ?)
            "#,
        )
        .bind(id)
        .bind(title)
        .bind(updated_at)
        .bind(updated_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn cite(pool: &SqlitePool, conversation_id: &str, document_id: &str, chunk_id: &str) {
        sqlx::query(
            r#"
            INSERT INTO conversation_documents (conversation_id, document_id, chunk_id)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(conversation_id)
        .bind(document_id)
        .bind(chunk_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn groups_by_conversation_newest_first() {
        let pool = pool().await;
        insert_document(&pool, "doc-a").await;
        insert_document(&pool, "doc-b").await;
        insert_conversation(&pool, "conv-old", "Older", "2026-01-01T00:00:00Z").await;
        insert_conversation(&pool, "conv-new", "Newer", "2026-02-01T00:00:00Z").await;
        insert_conversation(&pool, "conv-other", "Other doc", "2026-03-01T00:00:00Z").await;

        cite(&pool, "conv-new", "doc-a", "chunk-1").await;
        cite(&pool, "conv-new", "doc-a", "chunk-2").await;
        cite(&pool, "conv-old", "doc-a", "chunk-3").await;
        cite(&pool, "conv-other", "doc-b", "chunk-4").await;

        let repo = SqliteCitingConversationsRepository::new(pool);
        let rows = repo.find_citing_conversations("doc-a", 10).await.unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].conversation_id, "conv-new");
        assert_eq!(rows[0].passage_count, 2);
        assert_eq!(rows[1].conversation_id, "conv-old");
        assert_eq!(rows[1].passage_count, 1);
    }

    #[tokio::test]
    async fn an_uncited_document_returns_nothing() {
        let pool = pool().await;
        insert_document(&pool, "doc-a").await;
        let repo = SqliteCitingConversationsRepository::new(pool);
        assert!(repo
            .find_citing_conversations("doc-a", 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn honours_the_limit() {
        let pool = pool().await;
        insert_document(&pool, "doc-a").await;
        for i in 0..5 {
            let id = format!("conv-{i}");
            insert_conversation(&pool, &id, &id, &format!("2026-0{}-01T00:00:00Z", i + 1)).await;
            cite(&pool, &id, "doc-a", &format!("chunk-{i}")).await;
        }
        let repo = SqliteCitingConversationsRepository::new(pool);
        let rows = repo.find_citing_conversations("doc-a", 2).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].conversation_id, "conv-4");
    }
}
