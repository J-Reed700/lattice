//! Tag Repository Implementation
//!
//! Infrastructure implementation for tag persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the `RepositoryPort<TagEntity>` trait,
//! providing CRUD operations for tags using SQLite. All database
//! operations go through the TagMapper layer to maintain clean
//! separation between domain entities and database models.
//!
//! # Migration Notes
//!
//! - DB models are now internal and NOT exported

use crate::application::ports::{Filter, RepositoryPort};
use crate::features::tags::entity::Tag as TagEntity;
use crate::infrastructure::persistence::mappers::{TagMapper, TagModel};
use crate::shared::domain_types::TagName;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

/// Filter for querying tags by various criteria.
#[derive(Debug, Clone)]
pub struct TagFilter {
    /// Filter by name pattern (case-insensitive partial match)
    pub name_pattern: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
}

impl Filter for TagFilter {
    fn validate(&self) -> Result<()> {
        if let Some(limit) = self.limit {
            if limit == 0 || limit > 10000 {
                return Err(AppError::InvalidInput(
                    "Limit must be between 1 and 10000".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// SQLite implementation of tag repository.
///
/// Handles persistence of tag metadata in SQLite database.
/// Uses TagMapper to convert between domain entities and database models.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct TagRepository {
    pool: SqlitePool,
}

impl TagRepository {
    /// Create a new tag repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Find tag by name (case-insensitive).
    ///
    /// # Arguments
    ///
    /// * `name` - Tag name
    ///
    /// # Returns
    ///
    /// `Some(TagEntity)` if found, `None` if not found.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn find_by_name(&self, name: &str) -> Result<Option<TagEntity>> {
        let db_model = sqlx::query_as!(
            TagModel,
            r#"
            SELECT id as "id!", name as "name!", color as "color!", created_at as "created_at!", updated_at as "updated_at!"
            FROM tags
            WHERE LOWER(name) = LOWER(?)
            "#,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find tag by name: {}", e)))?;

        match db_model {
            Some(model) => Ok(Some(TagMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    /// Get or create a tag by name (case-insensitive).
    ///
    /// If a tag with the given name exists, it is returned. Otherwise, a new
    /// tag is created with the provided name and color.
    ///
    /// # Arguments
    ///
    /// * `name` - Tag name
    /// * `color` - Optional color (defaults to "#6366f1")
    ///
    /// # Returns
    ///
    /// The existing or newly created tag entity.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidData` if name validation fails
    pub async fn get_or_create(&self, name: &str, color: Option<&str>) -> Result<TagEntity> {
        // Try to find existing tag
        if let Some(tag) = self.find_by_name(name).await? {
            return Ok(tag);
        }

        let tag_name = TagName::new(name.to_string())
            .map_err(|e| AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
        let tag_color = color.unwrap_or("#6366f1");
        let entity = TagEntity::new(tag_name, tag_color.to_string());

        self.save(&entity).await?;

        Ok(entity)
    }

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

    /// Get all tags with document counts.
    ///
    /// # Returns
    ///
    /// Vector of tuples (tag, document_count) ordered by name.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn get_all_with_counts(&self) -> Result<Vec<(TagEntity, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                t.created_at,
                t.updated_at,
                COUNT(dt.document_id) as document_count
            FROM tags t
            LEFT JOIN document_tags dt ON t.id = dt.tag_id
            GROUP BY t.id
            ORDER BY t.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get tags with counts: {}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            let model = TagModel {
                id: row.get("id"),
                name: row.get("name"),
                color: row.get("color"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            };
            let document_count: i64 = row.get("document_count");
            let entity = TagMapper::to_entity(&model)?;
            result.push((entity, document_count));
        }

        Ok(result)
    }

    /// Update tag (name and/or color).
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Tag ID
    /// * `name` - Optional new name
    /// * `color` - Optional new color
    ///
    /// # Returns
    ///
    /// Updated tag entity.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if tag doesn't exist
    /// - `AppError::Database` if update fails
    /// - `AppError::InvalidData` if name is invalid
    pub async fn update(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<TagEntity> {
        let mut tag = self
            .find_by_id(tag_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tag with id {} not found", tag_id)))?;

        if let Some(new_name) = name {
            let tag_name = TagName::new(new_name.to_string())
                .map_err(|e| AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
            tag = tag.with_name(tag_name);
        }

        if let Some(new_color) = color {
            tag = tag.with_color(new_color.to_string());
        }

        self.save(&tag).await?;

        Ok(tag)
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

    /// Get or create tags in batch.
    ///
    /// # Arguments
    ///
    /// * `tag_names` - Tag names
    ///
    /// # Returns
    ///
    /// Vector of tag entities.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if operations fail
    pub async fn get_or_create_batch(&self, tag_names: Vec<String>) -> Result<Vec<TagEntity>> {
        let mut tags = Vec::new();
        for name in tag_names {
            let tag = self.get_or_create(&name, None).await?;
            tags.push(tag);
        }
        Ok(tags)
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

    /// Create a new tag (used by tag service).
    ///
    /// Creates tag and returns it on success. If tag with same name exists,
    /// updates its color and returns the updated tag.
    ///
    /// # Arguments
    ///
    /// * `name` - Tag name
    /// * `color` - Optional color (defaults to "#6366f1")
    ///
    /// # Returns
    ///
    /// The newly created or updated tag entity.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if creation fails
    /// - `AppError::InvalidData` if name validation fails
    pub async fn create(&self, name: &str, color: Option<&str>) -> Result<TagEntity> {
        let tag_name = TagName::new(name.to_string())
            .map_err(|e| AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
        let tag_color = color.unwrap_or("#6366f1");
        let entity = TagEntity::new(tag_name, tag_color.to_string());

        let now = chrono::Utc::now().to_rfc3339();

        // Store values to avoid E0716 temporary value dropped errors
        let id = entity.id();
        let name = entity.name();
        let color = entity.color();
        let id_str = id.as_str();
        let name_str = name.as_str();

        sqlx::query!(
            r#"
            INSERT INTO tags (id, name, color, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (name) DO UPDATE
            SET color = excluded.color, updated_at = excluded.updated_at
            "#,
            id_str,
            name_str,
            color,
            now,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create tag: {}", e)))?;

        self.find_by_name(name.as_ref())
            .await?
            .ok_or_else(|| AppError::Database("Tag creation failed".into()))
    }

    /// Get all tags ordered by name (alias for find_all).
    ///
    /// This method exists for API compatibility with tag service.
    ///
    /// # Returns
    ///
    /// Vector of all tag entities ordered by name.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    pub async fn get_all(&self) -> Result<Vec<TagEntity>> {
        self.find_all().await
    }

    /// Delete a tag by ID (wrapper for RepositoryPort::delete).
    ///
    /// This method exists for API compatibility with tag service.
    ///
    /// # Arguments
    ///
    /// * `id` - Tag ID to delete
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    pub async fn delete(&self, id: &str) -> Result<()> {
        use crate::application::ports::RepositoryPort;
        RepositoryPort::delete(self, id).await
    }
}

#[async_trait]
impl RepositoryPort<TagEntity> for TagRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<TagEntity>> {
        let db_model = sqlx::query_as!(
            TagModel,
            r#"
            SELECT id as "id!", name as "name!", color as "color!", created_at as "created_at!", updated_at as "updated_at!"
            FROM tags
            WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find tag by id: {}", e)))?;

        match db_model {
            Some(model) => Ok(Some(TagMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<TagEntity>> {
        filter.validate()?;

        // Downcast to TagFilter if possible
        let filter_any = filter.as_any();
        if let Some(tag_filter) = filter_any.downcast_ref::<TagFilter>() {
            let limit = tag_filter.limit.unwrap_or(1000);

            let db_models = if let Some(pattern) = &tag_filter.name_pattern {
                // Case-insensitive partial match
                let search_pattern =
                    crate::shared::sql_like::contains_pattern(&pattern.to_lowercase());
                let limit_i64 = limit as i64;
                sqlx::query(
                    r#"
                    SELECT id, name, color, created_at, updated_at
                    FROM tags
                    WHERE LOWER(name) LIKE ? ESCAPE '\'
                    ORDER BY name ASC
                    LIMIT ?
                    "#,
                )
                .bind(search_pattern)
                .bind(limit_i64)
                .fetch_all(&self.pool)
                .await
                .and_then(|rows| {
                    rows.into_iter()
                        .map(|row| {
                            Ok(TagModel {
                                id: row.try_get("id")?,
                                name: row.try_get("name")?,
                                color: row.try_get("color")?,
                                created_at: row.try_get("created_at")?,
                                updated_at: row.try_get("updated_at")?,
                            })
                        })
                        .collect()
                })
            } else {
                let limit_i64 = limit as i64;
                sqlx::query_as!(
                    TagModel,
                    r#"
                    SELECT id as "id!", name as "name!", color as "color!", created_at as "created_at!", updated_at as "updated_at!"
                    FROM tags
                    ORDER BY name ASC
                    LIMIT ?
                    "#,
                    limit_i64
                )
                .fetch_all(&self.pool)
                .await
            }
            .map_err(|e| AppError::Database(format!("Failed to find tags by filter: {}", e)))?;

            Ok(TagMapper::to_entities(&db_models))
        } else {
            // If not a TagFilter, return all tags
            self.find_all().await
        }
    }

    async fn find_all(&self) -> Result<Vec<TagEntity>> {
        let db_models = sqlx::query_as!(
            TagModel,
            r#"
            SELECT id as "id!", name as "name!", color as "color!", created_at as "created_at!", updated_at as "updated_at!"
            FROM tags
            ORDER BY name ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get all tags: {}", e)))?;

        Ok(TagMapper::to_entities(&db_models))
    }

    async fn save(&self, entity: &TagEntity) -> Result<()> {
        let model = TagMapper::to_model(entity);

        sqlx::query!(
            r#"
            INSERT INTO tags (id, name, color, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                color = excluded.color,
                updated_at = excluded.updated_at
            "#,
            model.id,
            model.name,
            model.color,
            model.created_at,
            model.updated_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save tag: {}", e)))?;

        Ok(())
    }

    async fn save_batch(&self, entities: &[TagEntity]) -> Result<()> {
        if entities.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for entity in entities {
            let model = TagMapper::to_model(entity);

            sqlx::query!(
                r#"
                INSERT INTO tags (id, name, color, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    color = excluded.color,
                    updated_at = excluded.updated_at
                "#,
                model.id,
                model.name,
                model.color,
                model.created_at,
                model.updated_at
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to save tag in batch: {}", e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query!("DELETE FROM tags WHERE id = ?", id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete tag: {}", e)))?;
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        for id in ids {
            sqlx::query!("DELETE FROM tags WHERE id = ?", id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("Failed to delete tag in batch: {}", e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to count tags: {}", e)))?;
        Ok(count as usize)
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tags WHERE id = ?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to check tag existence: {}", e)))
    }
}

#[async_trait]
impl crate::features::tags::TagRepositoryTrait for TagRepository {
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        let entity = self.get_or_create(name, color).await?;
        Ok(crate::features::tags::entity::Tag::with_id(
            entity.id().clone(),
            entity.name().clone(),
            entity.color().to_string(),
            *entity.created_at(),
            *entity.updated_at(),
        ))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<TagEntity>> {
        TagRepository::find_by_name(self, name).await
    }

    async fn get_or_create(&self, name: &str, color: Option<&str>) -> Result<TagEntity> {
        TagRepository::get_or_create(self, name, color).await
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<TagEntity>> {
        RepositoryPort::find_by_id(self, id).await
    }

    async fn find_by_filter(
        &self,
        filter: &dyn crate::application::ports::Filter,
    ) -> Result<Vec<TagEntity>> {
        RepositoryPort::find_by_filter(self, filter).await
    }

    async fn find_all(&self) -> Result<Vec<TagEntity>> {
        RepositoryPort::find_all(self).await
    }

    async fn save(&self, entity: &TagEntity) -> Result<()> {
        RepositoryPort::save(self, entity).await
    }

    async fn save_batch(&self, entities: &[TagEntity]) -> Result<()> {
        RepositoryPort::save_batch(self, entities).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        RepositoryPort::delete(self, id).await
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        RepositoryPort::delete_batch(self, ids).await
    }

    async fn count(&self) -> Result<usize> {
        RepositoryPort::count(self).await
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        RepositoryPort::exists(self, id).await
    }

    async fn get_tags_for_document(&self, document_id: &str) -> Result<Vec<TagEntity>> {
        TagRepository::get_tags_for_document(self, document_id).await
    }

    async fn add_tag_to_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        TagRepository::add_tag_to_document(self, document_id, tag_id).await
    }

    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        TagRepository::remove_tag_from_document(self, document_id, tag_id).await
    }

    async fn find_documents_by_tag(&self, tag_id: &str) -> Result<Vec<String>> {
        TagRepository::find_documents_by_tag(self, tag_id).await
    }

    async fn find_documents_by_tag_name(&self, tag_name: &str) -> Result<Vec<String>> {
        TagRepository::find_documents_by_tag_name(self, tag_name).await
    }

    async fn get_all_with_counts(&self) -> Result<Vec<(TagEntity, i64)>> {
        TagRepository::get_all_with_counts(self).await
    }

    async fn update(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<TagEntity> {
        TagRepository::update(self, tag_id, name, color).await
    }

    async fn add_tags_to_document_by_names(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<TagEntity>> {
        TagRepository::add_tags_to_document_by_names(self, document_id, tag_names).await
    }

    async fn get_tags_for_documents(
        &self,
        document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<TagEntity>>> {
        TagRepository::get_tags_for_documents(self, document_ids).await
    }

    async fn remove_all_tags_from_document(&self, document_id: &str) -> Result<()> {
        TagRepository::remove_all_tags_from_document(self, document_id).await
    }

    async fn get_or_create_batch(&self, tag_names: Vec<String>) -> Result<Vec<TagEntity>> {
        TagRepository::get_or_create_batch(self, tag_names).await
    }

    async fn add_tags_to_documents_batch(
        &self,
        document_tags: Vec<(String, Vec<String>)>,
    ) -> Result<usize> {
        TagRepository::add_tags_to_documents_batch(self, document_tags).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        sqlx::query(
            r#"
            CREATE TABLE tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL DEFAULT '#6366f1',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_save_and_find_by_id() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let name = TagName::new("rust".to_string()).unwrap();
        let entity = TagEntity::new(name, "#ff5733".to_string());

        repo.save(&entity).await.unwrap();

        let found = repo.find_by_id(entity.id().as_str()).await.unwrap();
        assert!(found.is_some());

        let found_entity = found.unwrap();
        assert_eq!(found_entity.id().as_str(), entity.id().as_str());
        assert_eq!(found_entity.name().as_str(), "rust");
        assert_eq!(found_entity.color(), "#ff5733");
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let name = TagName::new("python".to_string()).unwrap();
        let entity = TagEntity::new(name, "#6366f1".to_string());

        repo.save(&entity).await.unwrap();

        let found = repo.find_by_name("python").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name().as_str(), "python");

        // Case-insensitive search
        let found = repo.find_by_name("PYTHON").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name().as_str(), "python");
    }

    #[tokio::test]
    async fn test_get_or_create() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        // First call creates the tag
        let tag1 = repo.get_or_create("rust", Some("#ff5733")).await.unwrap();
        assert_eq!(tag1.name().as_str(), "rust");
        assert_eq!(repo.count().await.unwrap(), 1);

        // Second call returns existing tag
        let tag2 = repo.get_or_create("rust", Some("#ff5733")).await.unwrap();
        assert_eq!(tag2.id().as_str(), tag1.id().as_str());
        assert_eq!(repo.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_find_all() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let tag1 = TagEntity::new(
            TagName::new("rust".to_string()).unwrap(),
            "#ff5733".to_string(),
        );
        let tag2 = TagEntity::new(
            TagName::new("python".to_string()).unwrap(),
            "#6366f1".to_string(),
        );

        repo.save(&tag1).await.unwrap();
        repo.save(&tag2).await.unwrap();

        let all_tags = repo.find_all().await.unwrap();
        assert_eq!(all_tags.len(), 2);
        assert_eq!(all_tags[0].name().as_str(), "python");
        assert_eq!(all_tags[1].name().as_str(), "rust");
    }

    #[tokio::test]
    async fn test_save_batch() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let entities = vec![
            TagEntity::new(
                TagName::new("tag1".to_string()).unwrap(),
                "#ff0000".to_string(),
            ),
            TagEntity::new(
                TagName::new("tag2".to_string()).unwrap(),
                "#00ff00".to_string(),
            ),
            TagEntity::new(
                TagName::new("tag3".to_string()).unwrap(),
                "#0000ff".to_string(),
            ),
        ];

        repo.save_batch(&entities).await.unwrap();

        let count = repo.count().await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let entity = TagEntity::new(
            TagName::new("rust".to_string()).unwrap(),
            "#ff5733".to_string(),
        );

        repo.save(&entity).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 1);

        repo.delete(entity.id().as_str()).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_delete_batch() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let entities = vec![
            TagEntity::new(
                TagName::new("tag1".to_string()).unwrap(),
                "#ff0000".to_string(),
            ),
            TagEntity::new(
                TagName::new("tag2".to_string()).unwrap(),
                "#00ff00".to_string(),
            ),
        ];

        repo.save_batch(&entities).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 2);

        let ids: Vec<&str> = entities.iter().map(|e| e.id().as_str()).collect();
        repo.delete_batch(&ids).await.unwrap();
        assert_eq!(repo.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_exists() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let entity = TagEntity::new(
            TagName::new("rust".to_string()).unwrap(),
            "#ff5733".to_string(),
        );

        assert!(!repo.exists(entity.id().as_str()).await.unwrap());

        repo.save(&entity).await.unwrap();

        assert!(repo.exists(entity.id().as_str()).await.unwrap());
    }

    #[tokio::test]
    async fn test_find_by_filter() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = TagRepository::new(pool);

        let entities = vec![
            TagEntity::new(
                TagName::new("rust".to_string()).unwrap(),
                "#ff0000".to_string(),
            ),
            TagEntity::new(
                TagName::new("python".to_string()).unwrap(),
                "#00ff00".to_string(),
            ),
            TagEntity::new(
                TagName::new("ruby".to_string()).unwrap(),
                "#0000ff".to_string(),
            ),
        ];

        repo.save_batch(&entities).await.unwrap();

        let filter = TagFilter {
            name_pattern: Some("ru".to_string()),
            limit: None,
        };

        let results = repo.find_by_filter(&filter).await.unwrap();
        assert_eq!(results.len(), 2); // rust and ruby
        assert!(results.iter().any(|t| t.name().as_str() == "rust"));
        assert!(results.iter().any(|t| t.name().as_str() == "ruby"));
    }
}
