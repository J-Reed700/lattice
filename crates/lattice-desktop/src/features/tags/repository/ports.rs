//! `RepositoryPort<TagEntity>` and `TagRepositoryTrait` over the SQLite repository.

use async_trait::async_trait;
use sqlx::Row;

use crate::features::tags::persistence_mapper::{TagMapper, TagModel};
use lattice_core::application::ports::{Filter, RepositoryPort};
use lattice_core::domain::entities::Tag as TagEntity;
use lattice_core::shared::error::{AppError, Result};

use super::{TagFilter, TagRepository};

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
                    lattice_core::shared::persistence::sql_like::contains_pattern(&pattern.to_lowercase());
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
    ) -> Result<lattice_core::domain::entities::Tag> {
        let entity = self.get_or_create(name, color).await?;
        Ok(lattice_core::domain::entities::Tag::with_id(
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
        filter: &dyn lattice_core::application::ports::Filter,
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
