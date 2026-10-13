//! Links between tags and documents: reading, adding, replacing and removing them.

use sqlx::Row;

use crate::features::tags::entity::Tag as TagEntity;
use crate::features::tags::persistence_mapper::{TagMapper, TagModel};
use crate::shared::error::{AppError, Result};
use crate::shared::types::TagName;

use super::TagRepository;

impl TagRepository {
    /// Get all tags for a document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    ///
    /// # Returns
    ///
    /// Vector of tag entities associated with the document.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn get_tags_for_document(&self, document_id: &str) -> Result<Vec<TagEntity>> {
        let db_models = sqlx::query_as!(
            TagModel,
            r#"
            SELECT t.id as "id!", t.name as "name!", t.color as "color!", t.created_at as "created_at!", t.updated_at as "updated_at!"
            FROM tags t
            INNER JOIN document_tags dt ON t.id = dt.tag_id
            WHERE dt.document_id = ?
            ORDER BY t.name ASC
            "#,
            document_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get tags for document: {}", e)))?;

        Ok(TagMapper::to_entities(&db_models))
    }

    /// Add tag to document (idempotent).
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    /// * `tag_id` - Tag ID
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if insert fails
    pub async fn add_tag_to_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        sqlx::query!(
            "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?, ?)",
            document_id,
            tag_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to add tag to document: {}", e)))?;
        Ok(())
    }

    /// Remove tag from document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    /// * `tag_id` - Tag ID
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if delete fails
    pub async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM document_tags WHERE document_id = ? AND tag_id = ?",
            document_id,
            tag_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to remove tag from document: {}", e)))?;
        Ok(())
    }

    /// Find documents by tag ID.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Tag ID
    ///
    /// # Returns
    ///
    /// Vector of document IDs that have this tag.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_documents_by_tag(&self, tag_id: &str) -> Result<Vec<String>> {
        let records = sqlx::query!(
            "SELECT document_id FROM document_tags WHERE tag_id = ?",
            tag_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find documents by tag: {}", e)))?;

        Ok(records.into_iter().map(|r| r.document_id).collect())
    }

    /// Find documents by tag name (case-insensitive).
    ///
    /// # Arguments
    ///
    /// * `tag_name` - Tag name
    ///
    /// # Returns
    ///
    /// Vector of document IDs that have a tag with this name.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_documents_by_tag_name(&self, tag_name: &str) -> Result<Vec<String>> {
        let records = sqlx::query!(
            r#"
            SELECT dt.document_id
            FROM document_tags dt
            INNER JOIN tags t ON dt.tag_id = t.id
            WHERE LOWER(t.name) = LOWER(?)
            "#,
            tag_name
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find documents by tag name: {}", e)))?;

        Ok(records.into_iter().map(|r| r.document_id).collect())
    }

    /// Add multiple tags to a document by tag names.
    ///
    /// Creates tags if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    /// * `tag_names` - Tag names
    ///
    /// # Returns
    ///
    /// Vector of tags that were added.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if operations fail
    pub async fn add_tags_to_document_by_names(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<TagEntity>> {
        let mut tags = Vec::new();

        for name in tag_names {
            let tag = self.get_or_create(&name, None).await?;
            let tag_id = tag.id();
            self.add_tag_to_document(document_id, tag_id.as_str())
                .await?;
            tags.push(tag);
        }

        Ok(tags)
    }

    /// Validate before writing, then replace memberships and create missing tags
    /// in one transaction. The write reservation serializes competing replacements.
    pub async fn replace_document_tags(&self, document_id: &str, names: Vec<String>) -> Result<()> {
        let names = names
            .into_iter()
            .map(|name| {
                TagName::new(name.trim().to_lowercase())
                    .map_err(|error| AppError::InvalidInput(format!("Invalid tag name: {error}")))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM document_tags WHERE document_id = ?")
            .bind(document_id)
            .execute(&mut *tx)
            .await?;
        for name in names {
            let existing =
                sqlx::query_scalar::<_, String>("SELECT id FROM tags WHERE LOWER(name) = LOWER(?)")
                    .bind(name.as_str())
                    .fetch_optional(&mut *tx)
                    .await?;
            let tag_id = if let Some(id) = existing {
                id
            } else {
                let tag = TagEntity::new(name, "#6366f1".into());
                let model = TagMapper::to_model(&tag);
                sqlx::query("INSERT INTO tags (id, name, color, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                    .bind(&model.id).bind(&model.name).bind(&model.color)
                    .bind(&model.created_at).bind(&model.updated_at)
                    .execute(&mut *tx).await?;
                model.id
            };
            sqlx::query("INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?, ?)")
                .bind(document_id)
                .bind(tag_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Get tags for multiple documents (optimized batch query).
    ///
    /// Handles SQLite's 999 parameter limit by batching.
    ///
    /// # Arguments
    ///
    /// * `document_ids` - Document IDs
    ///
    /// # Returns
    ///
    /// HashMap mapping document IDs to their tags.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn get_tags_for_documents(
        &self,
        document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<TagEntity>>> {
        use std::collections::HashMap;

        if document_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // SQLite has a 999 parameter limit, so batch requests
        const BATCH_SIZE: usize = 900;
        let mut all_results = HashMap::new();

        for chunk in document_ids.chunks(BATCH_SIZE) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                r#"
                SELECT dt.document_id, t.id, t.name, t.color, t.created_at, t.updated_at
                FROM document_tags dt
                INNER JOIN tags t ON dt.tag_id = t.id
                WHERE dt.document_id IN ({})
                ORDER BY t.name ASC
                "#,
                placeholders
            );

            let mut query_builder = sqlx::query(&query);
            for id in chunk {
                query_builder = query_builder.bind(id);
            }

            let rows = query_builder.fetch_all(&self.pool).await.map_err(|e| {
                AppError::Database(format!("Failed to get tags for documents: {}", e))
            })?;

            for row in rows {
                let document_id: String = row.get("document_id");

                let model = TagModel {
                    id: row.get("id"),
                    name: row.get("name"),
                    color: row.get("color"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                };

                let entity = TagMapper::to_entity(&model)?;
                all_results
                    .entry(document_id)
                    .or_insert_with(Vec::new)
                    .push(entity);
            }
        }

        Ok(all_results)
    }

    /// Remove all tags from a document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - Document ID
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if delete fails
    pub async fn remove_all_tags_from_document(&self, document_id: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM document_tags WHERE document_id = ?",
            document_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to remove all tags from document: {}", e))
        })?;
        Ok(())
    }

    /// Bulk add tags to multiple documents.
    ///
    /// # Arguments
    ///
    /// * `document_tags` - Vector of (document_id, tag_names) pairs
    ///
    /// # Returns
    ///
    /// Number of tag associations created.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if operations fail
    pub async fn add_tags_to_documents_batch(
        &self,
        document_tags: Vec<(String, Vec<String>)>,
    ) -> Result<usize> {
        let mut count = 0;

        for (document_id, tag_names) in document_tags {
            for tag_name in tag_names {
                let tag = self.get_or_create(&tag_name, None).await?;
                let tag_id = tag.id();
                self.add_tag_to_document(&document_id, tag_id.as_str())
                    .await?;
                count += 1;
            }
        }

        Ok(count)
    }
}
