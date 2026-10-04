//! SQLite adapter for supplemental chat context.

use crate::application::ports::conversation_context::{
    ConversationContextPort, LinkedConversationSource,
};
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

pub struct SqliteConversationContext {
    pool: SqlitePool,
}

impl SqliteConversationContext {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct SourceRow {
    title: Option<String>,
    url: String,
    excerpt: Option<String>,
}

#[async_trait::async_trait]
impl ConversationContextPort for SqliteConversationContext {
    async fn space_prompt(&self, conversation_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT COALESCE(NULLIF(TRIM(f.instructions), ''), s.space_prompt)
             FROM conversations c
             LEFT JOIN conversation_spaces s ON s.id = c.space_id
             LEFT JOIN explorer_folders f ON f.root = c.explorer_root
             WHERE c.id = ? LIMIT 1",
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
        .map_err(|e| AppError::Database(format!("Failed to load space prompt: {}", e)))
    }

    async fn linked_sources(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> Result<Vec<LinkedConversationSource>> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, SourceRow>(
            "SELECT title, url, excerpt FROM conversation_web_sources WHERE conversation_id = ? ORDER BY added_at DESC LIMIT ?"
        ).bind(conversation_id).bind(limit).fetch_all(&self.pool).await
            .map_err(|e| AppError::Database(format!("Failed to load linked web sources for conversation context: {}", e)))?;
        Ok(rows
            .into_iter()
            .map(|row| LinkedConversationSource {
                title: row.title,
                url: row.url,
                excerpt: row.excerpt,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn context_reads_are_scoped_ordered_and_bounded() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE conversation_spaces (id TEXT PRIMARY KEY, space_prompt TEXT);
            CREATE TABLE conversations (id TEXT PRIMARY KEY, space_id TEXT, explorer_root TEXT);
            CREATE TABLE explorer_folders (root TEXT PRIMARY KEY, instructions TEXT);
            CREATE TABLE conversation_web_sources (conversation_id TEXT, title TEXT, url TEXT, excerpt TEXT, added_at TEXT);
            INSERT INTO conversation_spaces VALUES ('space', 'prompt');
            INSERT INTO explorer_folders VALUES ('/plugin', 'C++20 audio plugin'), ('/blank', '   ');
            INSERT INTO conversations VALUES ('chat', 'space', NULL), ('orphan', 'missing', NULL),
                ('folder', 'space', '/plugin'), ('blank', 'space', '/blank'), ('unlisted', 'space', '/gone');
            INSERT INTO conversation_web_sources VALUES ('chat', 'old', 'old-url', NULL, '2026-01-01'), ('chat', NULL, 'new-url', 'excerpt', '2026-02-01'), ('other', 'other', 'other-url', NULL, '2026-03-01');")
            .execute(&pool).await.unwrap();
        let adapter = SqliteConversationContext::new(pool);
        assert_eq!(
            adapter.space_prompt("chat").await.unwrap().as_deref(),
            Some("prompt")
        );
        assert_eq!(adapter.space_prompt("missing").await.unwrap(), None);
        assert_eq!(adapter.space_prompt("orphan").await.unwrap(), None);
        // A folder's instructions stand in for its space's prompt; blank ones
        // and an unlisted folder leave the space's in place.
        assert_eq!(
            adapter.space_prompt("folder").await.unwrap().as_deref(),
            Some("C++20 audio plugin")
        );
        for chat in ["blank", "unlisted"] {
            assert_eq!(
                adapter.space_prompt(chat).await.unwrap().as_deref(),
                Some("prompt")
            );
        }
        let rows = adapter.linked_sources("chat", 1).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].url, "new-url");
        assert_eq!(rows[0].title, None);
        assert_eq!(rows[0].excerpt.as_deref(), Some("excerpt"));
        assert!(adapter.linked_sources("chat", 0).await.unwrap().is_empty());
        assert!(adapter
            .linked_sources("missing", 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn context_database_failures_are_reported() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let adapter = SqliteConversationContext::new(pool);
        assert!(matches!(
            adapter.space_prompt("chat").await,
            Err(AppError::Database(_))
        ));
        assert!(matches!(
            adapter.linked_sources("chat", 10).await,
            Err(AppError::Database(_))
        ));
    }
}
