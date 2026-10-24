//! SQLite implementation of tag repository.

use crate::features::tags::persistence_mapper::{TagMapper, TagModel};
use lattice_core::application::ports::{Filter, RepositoryPort};
use lattice_core::domain::entities::Tag as TagEntity;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::TagName;
use sqlx::{Row, SqlitePool};

mod document_tags;
mod ports;

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
#[derive(Debug, Clone)]
pub struct TagRepository {
    pool: SqlitePool,
}

impl TagRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Find tag by name (case-insensitive).
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
    pub async fn get_or_create(&self, name: &str, color: Option<&str>) -> Result<TagEntity> {
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

    /// Get all tags with document counts.
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

    /// Get or create tags in batch.
    pub async fn get_or_create_batch(&self, tag_names: Vec<String>) -> Result<Vec<TagEntity>> {
        let mut tags = Vec::new();
        for name in tag_names {
            let tag = self.get_or_create(&name, None).await?;
            tags.push(tag);
        }
        Ok(tags)
    }

    /// Create a new tag (or update color on conflict).
    pub async fn create(&self, name: &str, color: Option<&str>) -> Result<TagEntity> {
        let tag_name = TagName::new(name.to_string())
            .map_err(|e| AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
        let tag_color = color.unwrap_or("#6366f1");
        let entity = TagEntity::new(tag_name, tag_color.to_string());

        let now = chrono::Utc::now().to_rfc3339();

        // Store values to avoid E0716 temporary value dropped errors
        let id = entity.id();
        let name_val = entity.name();
        let color_val = entity.color();
        let id_str = id.as_str();
        let name_str = name_val.as_str();

        sqlx::query!(
            r#"
            INSERT INTO tags (id, name, color, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (name) DO UPDATE
            SET color = excluded.color, updated_at = excluded.updated_at
            "#,
            id_str,
            name_str,
            color_val,
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
    pub async fn get_all(&self) -> Result<Vec<TagEntity>> {
        self.find_all().await
    }

    /// Delete a tag by ID.
    pub async fn delete(&self, id: &str) -> Result<()> {
        use lattice_core::application::ports::RepositoryPort;
        RepositoryPort::delete(self, id).await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod replacement_tests;
