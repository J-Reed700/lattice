use crate::features::file::custom_collections::dto::{
    CreateCustomCollectionRequest, CustomCollectionDto, CustomCollectionKind,
};
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
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

    pub async fn import_legacy(&self, collections: Vec<CustomCollectionDto>) -> Result<()> {
        validate_collection_set(&collections)?;
        let mut canonical = collections.clone();
        canonical.sort_by(|a, b| a.id.cmp(&b.id));
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&canonical)?));
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let imported: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_collection_import_receipts WHERE payload_sha256 = ?)")
            .bind(&digest).fetch_one(&mut *tx).await?;
        if imported {
            return Ok(());
        }
        for item in collections {
            let kind = kind_str(item.kind);
            let inserted = sqlx::query("INSERT INTO custom_collections (id, name, kind, parent_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING")
                .bind(&item.id).bind(item.name.trim()).bind(kind).bind(&item.parent_id)
                .bind(&item.created_at).bind(&item.updated_at).execute(&mut *tx).await?.rows_affected() == 1;
            if inserted {
                insert_memberships(&mut tx, &item.id, &item.document_ids).await?;
            } else {
                // A retry is safe only when the persisted record exactly matches. If an ID
                // collision represents different data, fail and retain the browser backup.
                let existing = sqlx::query("SELECT id, name, kind, parent_id, created_at, updated_at FROM custom_collections WHERE id = ?")
                    .bind(&item.id).fetch_one(&mut *tx).await?;
                let record_matches: bool = existing.try_get::<String, _>("name")?
                    == item.name.trim()
                    && existing.try_get::<String, _>("kind")? == kind
                    && existing.try_get::<Option<String>, _>("parent_id")? == item.parent_id
                    && existing.try_get::<String, _>("created_at")? == item.created_at
                    && existing.try_get::<String, _>("updated_at")? == item.updated_at;
                let stored_docs: Vec<String> = sqlx::query_scalar("SELECT document_id FROM custom_collection_documents WHERE collection_id = ? ORDER BY ordinal")
                    .bind(&item.id).fetch_all(&mut *tx).await?;
                if !record_matches || stored_docs != item.document_ids {
                    return Err(AppError::InvalidInput(format!(
                        "Legacy collection '{}' conflicts with persisted data",
                        item.id
                    )));
                }
            }
        }
        sqlx::query("INSERT INTO custom_collection_import_receipts (payload_sha256, imported_at) VALUES (?, ?)")
            .bind(digest).bind(Utc::now().to_rfc3339()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
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

fn validate_collection_set(collections: &[CustomCollectionDto]) -> Result<()> {
    use std::collections::{HashMap, HashSet};
    let by_id: HashMap<&str, &CustomCollectionDto> = collections
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    if by_id.len() != collections.len() {
        return Err(AppError::InvalidInput(
            "Collection IDs must be unique".into(),
        ));
    }
    let mut names = HashSet::new();
    for item in collections {
        if item.id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Collection ID cannot be empty".into(),
            ));
        }
        normalize_name(&item.name)?;
        validate_kind_memberships(item.kind, &item.document_ids)?;
        let parent = item.parent_id.as_deref().unwrap_or("");
        if item
            .parent_id
            .as_deref()
            .is_some_and(|id| !by_id.contains_key(id))
        {
            return Err(AppError::InvalidInput(
                "Collection parent does not exist".into(),
            ));
        }
        if !names.insert((parent.to_lowercase(), item.name.trim().to_lowercase())) {
            return Err(AppError::InvalidInput(
                "Collection names must be unique within a parent".into(),
            ));
        }
    }
    for item in collections {
        let mut visited = HashSet::new();
        let mut parent = item.parent_id.as_deref();
        while let Some(parent_id) = parent {
            if !visited.insert(parent_id) || parent_id == item.id {
                return Err(AppError::InvalidInput(
                    "Collection hierarchy cannot contain cycles".into(),
                ));
            }
            parent = by_id
                .get(parent_id)
                .and_then(|candidate| candidate.parent_id.as_deref());
        }
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
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260930010000_custom_collections_import_receipt.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        CustomCollectionsRepository::new(pool)
    }
    fn legacy(id: &str, parent_id: Option<&str>, document_ids: &[&str]) -> CustomCollectionDto {
        CustomCollectionDto {
            id: id.into(),
            name: id.into(),
            kind: CustomCollectionKind::Manual,
            parent_id: parent_id.map(str::to_string),
            document_ids: document_ids.iter().map(|v| (*v).into()).collect(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn legacy_import_preserves_tree_memberships_and_retries_idempotently() {
        let repo = repo().await;
        let old = vec![
            legacy("parent", None, &[]),
            legacy("child", Some("parent"), &["doc-a", "doc-b"]),
        ];
        repo.import_legacy(old.clone()).await.unwrap();
        repo.import_legacy(old).await.unwrap();
        let loaded = repo.list().await.unwrap();
        let child = loaded.iter().find(|item| item.id == "child").unwrap();
        assert_eq!(child.parent_id.as_deref(), Some("parent"));
        assert_eq!(child.document_ids, ["doc-a", "doc-b"]);
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn legacy_id_conflict_fails_and_membership_mutations_are_atomic() {
        let repo = repo().await;
        repo.import_legacy(vec![legacy("same", None, &["original"])])
            .await
            .unwrap();
        assert!(repo
            .import_legacy(vec![legacy("same", None, &["different"])])
            .await
            .is_err());
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
    async fn repeated_legacy_import_after_edit_does_not_restore_deleted_membership() {
        let repo = repo().await;
        let old = vec![legacy("same", None, &["old-doc"])];
        repo.import_legacy(old.clone()).await.unwrap();
        repo.remove_documents("same", &["old-doc".into()])
            .await
            .unwrap();
        repo.import_legacy(old).await.unwrap();
        assert!(repo.list().await.unwrap()[0].document_ids.is_empty());
    }

    #[tokio::test]
    async fn repeated_legacy_import_after_delete_does_not_resurrect_collection() {
        let repo = repo().await;
        let old = vec![legacy("same", None, &["old-doc"])];
        repo.import_legacy(old.clone()).await.unwrap();
        repo.delete("same").await.unwrap();
        repo.import_legacy(old).await.unwrap();
        assert!(repo.list().await.unwrap().is_empty());
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
        sqlx::raw_sql(include_str!(
            "../../../../migrations/20260930010000_custom_collections_import_receipt.sql"
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
