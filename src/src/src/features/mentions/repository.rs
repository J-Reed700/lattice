//! Mention Repository Implementation
//!
//! Implements MentionRepositoryPort for extracting and storing document mentions.
//!
//! # Features
//! - Extract @mentions and `[[wikilinks]]` from text
//! - Store mentions in database with context
//! - Search mentions and get backlinks

use crate::application::ports::mention_repository_port::{
    MentionData, MentionRepositoryPort, MentionWithContextData,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::Utc;
use lazy_regex::regex;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use uuid::Uuid;

// Compile regexes at compile-time for performance and safety
// Note: lazy_regex::regex! validates at compile-time, eliminating runtime panics
fn at_mention_regex() -> &'static regex::Regex {
    regex!(r"@\[([^\]]+)\]")
}

fn wikilink_regex() -> &'static regex::Regex {
    regex!(r"\[\[([^\]]+)\]\]")
}

/// SQLite implementation of MentionRepositoryPort
#[derive(Clone)]
pub struct MentionRepository {
    pool: SqlitePool,
}

impl MentionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Extract context around a mention position
    fn extract_context(&self, text: &str, position: usize) -> String {
        let start = position.saturating_sub(50);
        let end = (position + 50).min(text.len());
        text[start..end].to_string()
    }

    /// Extract mentions from text using regex patterns
    fn extract_mentions_from_text(&self, text: &str) -> HashMap<String, Vec<(String, usize)>> {
        let mut mentions: HashMap<String, Vec<(String, usize)>> = HashMap::new();

        // Extract @[person] mentions
        for cap in at_mention_regex().captures_iter(text) {
            // Defensive: cap[0] should always exist for captures_iter matches,
            // but we handle gracefully to avoid production panics
            let Some(full_match) = cap.get(0) else {
                tracing::warn!("Regex capture missing group 0 for @mention - skipping");
                continue;
            };
            let name = cap[1].to_string();
            let position = full_match.start();

            mentions
                .entry("person".to_string())
                .or_default()
                .push((name, position));
        }

        // Extract [[wikilink]] mentions
        for cap in wikilink_regex().captures_iter(text) {
            // Defensive: cap[0] should always exist for captures_iter matches,
            // but we handle gracefully to avoid production panics
            let Some(full_match) = cap.get(0) else {
                tracing::warn!("Regex capture missing group 0 for wikilink - skipping");
                continue;
            };
            let name = cap[1].to_string();
            let position = full_match.start();

            mentions
                .entry("wikilink".to_string())
                .or_default()
                .push((name, position));
        }

        mentions
    }

    /// Link mention to document with context
    async fn link_mention_to_document(
        &self,
        document_id: &str,
        mention_id: &str,
        context: Option<&str>,
        position: Option<i64>,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO document_mentions (id, document_id, mention_id, context, position, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            id,
            document_id,
            mention_id,
            context,
            position,
            created_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to link mention: {}", e)))?;

        Ok(())
    }

    /// Clear all mentions for a document (for reindexing)
    async fn clear_document_mentions(&self, document_id: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM document_mentions WHERE document_id = ?",
            document_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to clear mentions: {}", e)))?;
        Ok(())
    }

    /// Extract @mentions and `[[wikilinks]]` from text and store them.
    ///
    /// This is a public wrapper for the trait method, allowing direct calls
    /// on MentionRepository instances without going through the trait.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    /// * `text` - Document text content to extract mentions from
    ///
    /// # Returns
    ///
    /// Vector of extracted mentions with context and position.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if storage operations fail
    pub async fn extract_and_store_mentions(
        &self,
        document_id: &str,
        text: &str,
    ) -> Result<Vec<MentionWithContextData>, AppError> {
        // Clear existing mentions for this document
        self.clear_document_mentions(document_id).await?;

        // Extract mentions from text
        let extracted = self.extract_mentions_from_text(text);
        let mut results = Vec::new();

        // Store each mention
        for (mention_type, occurrences) in extracted {
            for (name, position) in occurrences {
                // Create or get mention
                let mention = self.create_mention(&name, &mention_type, None).await?;

                // Extract context
                let context = self.extract_context(text, position);

                // Link to document
                self.link_mention_to_document(
                    document_id,
                    &mention.id,
                    Some(&context),
                    Some(position as i64),
                )
                .await?;

                results.push(MentionWithContextData {
                    mention,
                    document_id: document_id.to_string(),
                    context: Some(context),
                    position: Some(position as i64),
                });
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl MentionRepositoryPort for MentionRepository {
    async fn create_mention(
        &self,
        name: &str,
        mention_type: &str,
        metadata: Option<&str>,
    ) -> Result<MentionData, AppError> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        let row = sqlx::query!(
            r#"
            INSERT INTO mentions (id, name, type, metadata, created_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                type = excluded.type,
                metadata = excluded.metadata
            RETURNING id, name, type, metadata, created_at
            "#,
            id,
            name,
            mention_type,
            metadata,
            created_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create mention: {}", e)))?;

        Ok(MentionData {
            id: row.id.ok_or_else(|| {
                AppError::Database(
                    "Failed to create mention: missing id in RETURNING row".to_string(),
                )
            })?,
            name: row.name,
            mention_type: row.r#type,
            metadata: row.metadata,
            created_at: row.created_at,
        })
    }

    async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>, AppError> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, type, metadata, created_at
            FROM mentions
            WHERE name = ?
            "#,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find mention: {}", e)))?;

        match row {
            Some(r) => {
                let id = r.id.ok_or_else(|| {
                    AppError::Database("Failed to load mention: row has NULL id".to_string())
                })?;
                Ok(Some(MentionData {
                    id,
                    name: r.name,
                    mention_type: r.r#type,
                    metadata: r.metadata,
                    created_at: r.created_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<MentionData>, AppError> {
        let search_pattern = crate::shared::sql_like::contains_pattern(query);

        let rows = sqlx::query(
            r#"
            SELECT id, name, type, metadata, created_at
            FROM mentions
            WHERE name LIKE ? ESCAPE '\'
            ORDER BY name
            LIMIT ?
            "#,
        )
        .bind(search_pattern)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to search mentions: {}", e)))?;

        rows.into_iter()
            .map(|r| {
                let id = r
                    .try_get::<Option<String>, _>("id")
                    .map_err(|e| AppError::Database(format!("Failed to load mention id: {}", e)))?
                    .ok_or_else(|| {
                        AppError::Database("Failed to load mention: row has NULL id".to_string())
                    })?;
                Ok(MentionData {
                    id,
                    name: r.try_get("name").map_err(|e| {
                        AppError::Database(format!("Failed to load mention name: {}", e))
                    })?,
                    mention_type: r.try_get("type").map_err(|e| {
                        AppError::Database(format!("Failed to load mention type: {}", e))
                    })?,
                    metadata: r.try_get("metadata").map_err(|e| {
                        AppError::Database(format!("Failed to load mention metadata: {}", e))
                    })?,
                    created_at: r.try_get("created_at").map_err(|e| {
                        AppError::Database(format!("Failed to load mention created_at: {}", e))
                    })?,
                })
            })
            .collect()
    }

    async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<MentionData>, AppError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, name, type, metadata, created_at
            FROM mentions
            WHERE type = ?
            ORDER BY name
            "#,
            mention_type
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get mentions: {}", e)))?;

        rows.into_iter()
            .map(|r| {
                let id = r.id.ok_or_else(|| {
                    AppError::Database("Failed to load mention: row has NULL id".to_string())
                })?;
                Ok(MentionData {
                    id,
                    name: r.name,
                    mention_type: r.r#type,
                    metadata: r.metadata,
                    created_at: r.created_at,
                })
            })
            .collect()
    }

    async fn get_mentions_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<MentionWithContextData>, AppError> {
        let rows = sqlx::query(
            r#"
            SELECT
                m.id, m.name, m.type, m.metadata, m.created_at,
                dm.document_id, dm.context, dm.position
            FROM mentions m
            INNER JOIN document_mentions dm ON m.id = dm.mention_id
            WHERE dm.document_id = ?
            ORDER BY dm.position
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get document mentions: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| MentionWithContextData {
                mention: MentionData {
                    id: r.get("id"),
                    name: r.get("name"),
                    mention_type: r.get("type"),
                    metadata: r.get("metadata"),
                    created_at: r.get("created_at"),
                },
                document_id: r.get("document_id"),
                context: r.get("context"),
                position: r.get("position"),
            })
            .collect())
    }

    async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT document_id
            FROM document_mentions
            WHERE mention_id = ?
            "#,
            mention_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get backlinks: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.document_id).collect())
    }

    async fn extract_and_store_mentions(
        &self,
        document_id: &str,
        text: &str,
    ) -> Result<Vec<MentionWithContextData>, AppError> {
        // Clear existing mentions for this document
        self.clear_document_mentions(document_id).await?;

        // Extract mentions from text
        let extracted = self.extract_mentions_from_text(text);
        let mut results = Vec::new();

        // Store each mention
        for (mention_type, occurrences) in extracted {
            for (name, position) in occurrences {
                // Create or get mention
                let mention = self.create_mention(&name, &mention_type, None).await?;

                // Extract context
                let context = self.extract_context(text, position);

                // Link to document
                self.link_mention_to_document(
                    document_id,
                    &mention.id,
                    Some(&context),
                    Some(position as i64),
                )
                .await?;

                results.push(MentionWithContextData {
                    mention,
                    document_id: document_id.to_string(),
                    context: Some(context),
                    position: Some(position as i64),
                });
            }
        }

        Ok(results)
    }

    async fn update_mention(
        &self,
        id: &str,
        mention_type: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<MentionData, AppError> {
        // First, get the current mention
        let current = sqlx::query!(
            r#"
            SELECT id, name, type, metadata, created_at
            FROM mentions
            WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch mention: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Mention not found: {}", id)))?;

        // Use provided values or keep existing ones
        let new_type = mention_type.unwrap_or(&current.r#type);
        let new_metadata = metadata.or(current.metadata.as_deref());

        // Update the mention
        let row = sqlx::query!(
            r#"
            UPDATE mentions
            SET type = ?, metadata = ?
            WHERE id = ?
            RETURNING id, name, type, metadata, created_at
            "#,
            new_type,
            new_metadata,
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update mention: {}", e)))?;

        Ok(MentionData {
            id: row.id.ok_or_else(|| {
                AppError::Database(
                    "Failed to update mention: missing id in RETURNING row".to_string(),
                )
            })?,
            name: row.name,
            mention_type: row.r#type,
            metadata: row.metadata,
            created_at: row.created_at,
        })
    }

    async fn delete_mention(&self, id: &str) -> Result<(), AppError> {
        sqlx::query!("DELETE FROM mentions WHERE id = ?", id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete mention: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();

        // Setup schema
        sqlx::query(
            r#"
            CREATE TABLE mentions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                type TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("CREATE UNIQUE INDEX idx_mentions_name ON mentions(name)")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE document_mentions (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                mention_id TEXT NOT NULL,
                context TEXT,
                position INTEGER,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_mention() {
        let pool = create_test_pool().await;
        let repo = MentionRepository::new(pool);

        let mention = repo
            .create_mention("John Doe", "person", None)
            .await
            .unwrap();

        assert_eq!(mention.name, "John Doe");
        assert_eq!(mention.mention_type, "person");
    }

    #[tokio::test]
    async fn test_extract_mentions() {
        let pool = create_test_pool().await;
        let repo = MentionRepository::new(pool);

        let text = "I met @[John Doe] and discussed [[Project Alpha]].";
        let mentions = repo.extract_mentions_from_text(text);

        assert!(mentions.contains_key("person"));
        assert!(mentions.contains_key("wikilink"));
    }

    #[tokio::test]
    async fn test_search_mentions() {
        let pool = create_test_pool().await;
        let repo = MentionRepository::new(pool);

        repo.create_mention("Alice", "person", None).await.unwrap();
        repo.create_mention("Albert", "person", None).await.unwrap();
        repo.create_mention("Bob", "person", None).await.unwrap();

        let results = repo.search_mentions("Al", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_extract_and_store() {
        let pool = create_test_pool().await;
        let repo = MentionRepository::new(pool);

        let text = "Met @[Alice] to discuss [[Rust Project]].";
        let mentions = repo.extract_and_store_mentions("doc1", text).await.unwrap();

        assert_eq!(mentions.len(), 2);
        assert!(mentions[0].context.is_some());
    }
}
