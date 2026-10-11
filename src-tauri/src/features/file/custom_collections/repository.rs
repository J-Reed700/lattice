use crate::features::file::custom_collections::dto::{
    CreateCustomCollectionRequest, CustomCollectionDto, CustomCollectionKind,
};
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CustomCollectionsRepository {
    pool: SqlitePool,
}

impl CustomCollectionsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<CustomCollectionDto>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query("SELECT id, name, kind, parent_id, created_at, updated_at FROM custom_collections ORDER BY created_at DESC, rowid DESC")
            .fetch_all(&mut *tx).await?;
        let memberships = sqlx::query("SELECT collection_id, document_id FROM custom_collection_documents ORDER BY collection_id, ordinal")
            .fetch_all(&mut *tx).await?;
        let mut docs = std::collections::HashMap::<String, Vec<String>>::new();
        for row in memberships {
            docs.entry(row.try_get("collection_id")?)
                .or_default()
                .push(row.try_get("document_id")?);
        }
        let result: Vec<CustomCollectionDto> = rows
            .into_iter()
            .map(|row| -> Result<CustomCollectionDto> {
                let id: String = row.try_get("id")?;
                Ok(CustomCollectionDto {
                    document_ids: docs.remove(&id).unwrap_or_default(),
                    id,
                    name: row.try_get("name")?,
                    kind: parse_kind(&row.try_get::<String, _>("kind")?)?,
                    parent_id: row.try_get("parent_id")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create(&self, request: CreateCustomCollectionRequest) -> Result<String> {
        let name = normalize_name(&request.name)?;
        validate_kind_memberships(request.kind, &request.document_ids)?;
        let id = format!("custom:{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("INSERT INTO custom_collections (id, name, kind, parent_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(name).bind(kind_str(request.kind)).bind(&request.parent_id).bind(&now).bind(&now)
            .execute(&mut *tx).await.map_err(map_unique_name)?;
        insert_memberships(&mut tx, &id, &request.document_ids).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn rename(&self, id: &str, name: &str) -> Result<()> {
        let name = normalize_name(name)?;
        sqlx::query("UPDATE custom_collections SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_unique_name)?;
        Ok(())
    }

    pub async fn move_to(&self, id: &str, parent_id: Option<&str>) -> Result<()> {
        if parent_id == Some(id) {
            return Err(AppError::InvalidInput(
                "A collection cannot be its own parent".into(),
            ));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(parent) = parent_id {
            let cycle: bool = sqlx::query_scalar("WITH RECURSIVE descendants(id) AS (SELECT id FROM custom_collections WHERE parent_id = ? UNION ALL SELECT c.id FROM custom_collections c JOIN descendants d ON c.parent_id = d.id) SELECT EXISTS(SELECT 1 FROM descendants WHERE id = ?)")
                .bind(id).bind(parent).fetch_one(&mut *tx).await?;
            if cycle {
                return Err(AppError::InvalidInput(
                    "A collection cannot be moved inside its descendant".into(),
                ));
            }
        }
        sqlx::query("UPDATE custom_collections SET parent_id = ?, updated_at = ? WHERE id = ?")
            .bind(parent_id)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_unique_name)?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM custom_collections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_documents(&self, id: &str, document_ids: &[String]) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let kind =
            sqlx::query_scalar::<_, String>("SELECT kind FROM custom_collections WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Collection '{id}' not found")))?;
        if kind == "snapshot" {
            return Err(AppError::InvalidInput(
                "Snapshot collection membership is immutable".into(),
            ));
        }
        for document_id in document_ids {
            if document_id.is_empty() {
                continue;
            }
            sqlx::query("INSERT INTO custom_collection_documents (collection_id, document_id, ordinal) VALUES (?, ?, (SELECT COALESCE(MAX(ordinal), -1) + 1 FROM custom_collection_documents WHERE collection_id = ?)) ON CONFLICT(collection_id, document_id) DO NOTHING")
                .bind(id).bind(document_id).bind(id).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE custom_collections SET updated_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn remove_documents(&self, id: &str, document_ids: &[String]) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let kind =
            sqlx::query_scalar::<_, String>("SELECT kind FROM custom_collections WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Collection '{id}' not found")))?;
        if kind == "snapshot" {
            return Err(AppError::InvalidInput(
                "Snapshot collection membership is immutable".into(),
            ));
        }
        for document_id in document_ids {
            sqlx::query("DELETE FROM custom_collection_documents WHERE collection_id = ? AND document_id = ?").bind(id).bind(document_id).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE custom_collections SET updated_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn insert_memberships(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
    ids: &[String],
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for (ordinal, document_id) in ids.iter().enumerate() {
        if document_id.is_empty() || !seen.insert(document_id) {
            continue;
        }
        sqlx::query("INSERT INTO custom_collection_documents (collection_id, document_id, ordinal) VALUES (?, ?, ?)")
            .bind(id).bind(document_id).bind(ordinal as i64).execute(&mut **tx).await?;
    }
    Ok(())
}

fn normalize_name(name: &str) -> Result<String> {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(AppError::InvalidInput(
            "Collection name cannot be empty".into(),
        ));
    }
    Ok(normalized)
}
fn validate_kind_memberships(kind: CustomCollectionKind, ids: &[String]) -> Result<()> {
    if kind == CustomCollectionKind::Manual && ids.iter().any(String::is_empty) {
        return Err(AppError::InvalidInput(
            "Document IDs cannot be empty".into(),
        ));
    }
    if kind == CustomCollectionKind::Snapshot && ids.is_empty() {
        return Err(AppError::InvalidInput(
            "A snapshot must contain at least one document".into(),
        ));
    }
    Ok(())
}
fn kind_str(kind: CustomCollectionKind) -> &'static str {
    match kind {
        CustomCollectionKind::Manual => "manual",
        CustomCollectionKind::Snapshot => "snapshot",
    }
}
fn parse_kind(kind: &str) -> Result<CustomCollectionKind> {
    match kind {
        "manual" => Ok(CustomCollectionKind::Manual),
        "snapshot" => Ok(CustomCollectionKind::Snapshot),
        _ => Err(AppError::InvalidInput(format!(
            "Unknown collection kind: {kind}"
        ))),
    }
}
fn map_unique_name(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db) = error {
        if db.is_unique_violation() {
            return AppError::InvalidInput("Collection name already exists in this parent".into());
        }
    }
    error.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn repo() -> CustomCollectionsRepository {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260930000000_custom_collections.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        CustomCollectionsRepository::new(pool)
    }
    #[tokio::test]
    async fn membership_mutations_are_atomic() {
        let repo = repo().await;
        let id = repo
            .create(CreateCustomCollectionRequest {
                name: "Manual".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: None,
                document_ids: vec![],
            })
            .await
            .unwrap();
        repo.add_documents(&id, &["one".into(), "two".into()])
            .await
            .unwrap();
        repo.remove_documents(&id, &["one".into()]).await.unwrap();
        let loaded = repo.list().await.unwrap();
        assert_eq!(
            loaded
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .document_ids,
            ["two"]
        );
    }

    #[tokio::test]
    async fn enforces_unique_sibling_names_and_rejects_descendant_moves() {
        let repo = repo().await;
        let parent = repo
            .create(CreateCustomCollectionRequest {
                name: "Parent".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: None,
                document_ids: vec![],
            })
            .await
            .unwrap();
        let child = repo
            .create(CreateCustomCollectionRequest {
                name: "Child".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: Some(parent.clone()),
                document_ids: vec![],
            })
            .await
            .unwrap();
        assert!(repo
            .create(CreateCustomCollectionRequest {
                name: "parent".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: None,
                document_ids: vec![]
            })
            .await
            .is_err());
        assert!(repo.move_to(&parent, Some(&child)).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_membership_adds_keep_both_updates() {
        let pool = SqlitePoolOptions::new()
            .max_connections(3)
            .connect("sqlite::memory:?cache=shared")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260930000000_custom_collections.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let repo = CustomCollectionsRepository::new(pool);
        let id = repo
            .create(CreateCustomCollectionRequest {
                name: "Concurrent".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: None,
                document_ids: vec![],
            })
            .await
            .unwrap();
        let first_document = vec!["first".to_string()];
        let second_document = vec!["second".to_string()];
        let (first, second) = tokio::join!(
            repo.add_documents(&id, &first_document),
            repo.add_documents(&id, &second_document)
        );
        first.unwrap();
        second.unwrap();
        let mut ids = repo.list().await.unwrap()[0].document_ids.clone();
        ids.sort();
        assert_eq!(ids, ["first", "second"]);
    }

    #[tokio::test]
    async fn deleting_a_parent_cascades_and_snapshots_keep_fixed_membership() {
        let repo = repo().await;
        let parent = repo
            .create(CreateCustomCollectionRequest {
                name: "Parent".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: None,
                document_ids: vec![],
            })
            .await
            .unwrap();
        let _child = repo
            .create(CreateCustomCollectionRequest {
                name: "Child".into(),
                kind: CustomCollectionKind::Manual,
                parent_id: Some(parent.clone()),
                document_ids: vec!["doc".into()],
            })
            .await
            .unwrap();
        let snapshot = repo
            .create(CreateCustomCollectionRequest {
                name: "Snapshot".into(),
                kind: CustomCollectionKind::Snapshot,
                parent_id: None,
                document_ids: vec!["fixed".into()],
            })
            .await
            .unwrap();
        assert!(repo
            .add_documents(&snapshot, &["another".into()])
            .await
            .is_err());
        assert!(repo
            .remove_documents(&snapshot, &["fixed".into()])
            .await
            .is_err());
        repo.delete(&parent).await.unwrap();
        let remaining = repo.list().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].document_ids, ["fixed"]);
    }
}
