//! The chat starter cache and the aggregate reads behind the starter prompt.
use super::chat::ports::ChatStarterCache;
use crate::features::conversation::repository::ConversationRepository;
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::bind_limit::rows_per_statement;
use lattice_core::shared::persistence::Database;
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};

/// How many document ids one statement binds at a time; the aggregates over
/// more are merged here rather than in SQL.
const ID_BIND_BATCH: usize = rows_per_statement(1, 0);

/// Repository-backed cache for corpus-derived chat starters, one row per
/// corpus fingerprint, plus the aggregate reads the starter prompt needs. Every
/// read is scoped to the document ids the chat's space can see.
pub struct ChatStarterCacheRepository {
    db: Database,
}

impl ChatStarterCacheRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// The cached `(starters_json, created_at)` for a fingerprint, if any.
    pub async fn get(&self, fingerprint: &str) -> Result<Option<(String, String)>> {
        let mut read = self.db.begin_read().await?;
        let row = sqlx::query(
            r#"
            SELECT starters_json, created_at
            FROM chat_starter_cache
            WHERE fingerprint = ?
            "#,
        )
        .bind(fingerprint)
        .fetch_optional(&mut *read)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read chat starter cache: {}", e)))?;
        read.commit().await?;

        match row {
            Some(row) => {
                let starters_json: String = row.try_get("starters_json").map_err(|e| {
                    AppError::Database(format!("Malformed chat starter cache row: {}", e))
                })?;
                let created_at: String = row.try_get("created_at").map_err(|e| {
                    AppError::Database(format!("Malformed chat starter cache row: {}", e))
                })?;
                Ok(Some((starters_json, created_at)))
            }
            None => Ok(None),
        }
    }

    /// Upsert one fingerprint's starters.
    pub async fn put(
        &self,
        fingerprint: &str,
        starters_json: &str,
        created_at: &str,
    ) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        sqlx::query(
            r#"
            INSERT INTO chat_starter_cache (fingerprint, starters_json, created_at)
            VALUES (?, ?, ?)
            ON CONFLICT(fingerprint) DO UPDATE SET
                starters_json = excluded.starters_json,
                created_at = excluded.created_at
            "#,
        )
        .bind(fingerprint)
        .bind(starters_json)
        .bind(created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to write chat starter cache: {}", e)))?;
        tx.commit().await?;

        Ok(())
    }

    /// Keep only the newest `keep` rows; the cache is a convenience, not a record.
    pub async fn prune(&self, keep: i64) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        sqlx::query(
            r#"
            DELETE FROM chat_starter_cache
            WHERE fingerprint NOT IN (
                SELECT fingerprint FROM chat_starter_cache
                ORDER BY created_at DESC, fingerprint ASC
                LIMIT ?
            )
            "#,
        )
        .bind(keep)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to prune chat starter cache: {}", e)))?;
        tx.commit().await?;

        Ok(())
    }

    /// `(file_type, count)` over `allowed_ids`, most common first.
    ///
    /// Documents with no `file_type` are reported as `"other"` so the mix always
    /// sums to the document count.
    pub async fn type_mix(&self, allowed_ids: &HashSet<String>) -> Result<Vec<(String, i64)>> {
        let mut read = self.db.begin_read().await?;
        let mut totals: HashMap<String, i64> = HashMap::new();
        for batch in sorted_ids(allowed_ids).chunks(ID_BIND_BATCH) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT COALESCE(NULLIF(TRIM(LOWER(file_type)), ''), 'other') AS kind, COUNT(*) AS n FROM documents WHERE id IN (",
            );
            let mut separated = query.separated(", ");
            for id in batch {
                separated.push_bind(*id);
            }
            query.push(") GROUP BY kind");

            let rows: Vec<(String, i64)> = query
                .build_query_as()
                .fetch_all(&mut *read)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to read corpus type mix: {}", e))
                })?;
            for (kind, n) in rows {
                *totals.entry(kind).or_default() += n;
            }
        }
        read.commit().await?;

        let mut mix: Vec<(String, i64)> = totals.into_iter().collect();
        mix.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(mix)
    }

    /// The newest document titles in `allowed_ids`, newest `indexed_at` first.
    ///
    /// Blank names are dropped in SQL so a batch never spends its limit on
    /// titles the prompt would discard anyway.
    pub async fn recent_titles(&self, allowed_ids: &HashSet<String>) -> Result<Vec<String>> {
        let mut read = self.db.begin_read().await?;
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for batch in sorted_ids(allowed_ids).chunks(ID_BIND_BATCH) {
            let mut query = QueryBuilder::<Sqlite>::new(
                "SELECT indexed_at, id, file_name FROM documents WHERE TRIM(file_name) <> '' AND id IN (",
            );
            let mut separated = query.separated(", ");
            for id in batch {
                separated.push_bind(*id);
            }
            query.push(") ORDER BY indexed_at DESC, id ASC");

            let batch_rows: Vec<(String, String, String)> = query
                .build_query_as()
                .fetch_all(&mut *read)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to read recent document titles: {}", e))
                })?;
            rows.extend(batch_rows);
        }
        read.commit().await?;

        // Each batch is ordered on its own, so the batches are merged here.
        rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        Ok(rows
            .into_iter()
            .map(|(_, _, file_name)| file_name)
            .collect())
    }

    /// Newest `indexed_at` in `allowed_ids`, or an empty string when there is none.
    pub async fn latest_indexed_at(&self, allowed_ids: &HashSet<String>) -> Result<String> {
        let mut read = self.db.begin_read().await?;
        let mut latest = String::new();
        for batch in sorted_ids(allowed_ids).chunks(ID_BIND_BATCH) {
            let mut query =
                QueryBuilder::<Sqlite>::new("SELECT MAX(indexed_at) FROM documents WHERE id IN (");
            let mut separated = query.separated(", ");
            for id in batch {
                separated.push_bind(*id);
            }
            query.push(")");

            let value: Option<String> = query
                .build_query_scalar()
                .fetch_one(&mut *read)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to read latest indexed_at: {}", e))
                })?;
            if let Some(value) = value {
                if value > latest {
                    latest = value;
                }
            }
        }
        read.commit().await?;
        Ok(latest)
    }

    /// The documents this space can see — the same allow-list every retrieval
    /// path uses, so the questions can only be about documents this chat could
    /// actually answer from. The rule stays in the one statement
    /// [`ConversationRepository::space_document_scope`] owns.
    pub async fn space_document_scope(&self, space_id: &str) -> Result<HashSet<String>> {
        ConversationRepository::new(self.db.clone())
            .space_document_scope(space_id)
            .await
    }
}

#[async_trait]
impl ChatStarterCache for ChatStarterCacheRepository {
    async fn space_document_scope(&self, space_id: &str) -> Result<HashSet<String>> {
        ChatStarterCacheRepository::space_document_scope(self, space_id).await
    }
    async fn get(&self, fingerprint: &str) -> Result<Option<(String, String)>> {
        ChatStarterCacheRepository::get(self, fingerprint).await
    }
    async fn put(&self, fingerprint: &str, starters_json: &str, created_at: &str) -> Result<()> {
        ChatStarterCacheRepository::put(self, fingerprint, starters_json, created_at).await
    }
    async fn prune(&self, keep: i64) -> Result<()> {
        ChatStarterCacheRepository::prune(self, keep).await
    }
    async fn type_mix(&self, allowed_ids: &HashSet<String>) -> Result<Vec<(String, i64)>> {
        ChatStarterCacheRepository::type_mix(self, allowed_ids).await
    }
    async fn recent_titles(&self, allowed_ids: &HashSet<String>) -> Result<Vec<String>> {
        ChatStarterCacheRepository::recent_titles(self, allowed_ids).await
    }
    async fn latest_indexed_at(&self, allowed_ids: &HashSet<String>) -> Result<String> {
        ChatStarterCacheRepository::latest_indexed_at(self, allowed_ids).await
    }
}

/// A stable bind order, so the same scope always produces the same statements.
fn sorted_ids(allowed_ids: &HashSet<String>) -> Vec<&String> {
    let mut ids: Vec<&String> = allowed_ids.iter().collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn fresh_repository() -> (ChatStarterCacheRepository, SqlitePool) {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let repo = ChatStarterCacheRepository::new(Database::new(pool.clone()));
        (repo, pool)
    }

    #[tokio::test]
    async fn test_starter_cache_round_trip() {
        let (repo, _pool) = fresh_repository().await;
        let json = r#"[{"question":"What did I decide about pricing?"}]"#;

        repo.put("fp-1", json, "2026-09-06T10:00:00Z")
            .await
            .unwrap();

        let found = repo.get("fp-1").await.unwrap().unwrap();
        assert_eq!(found.0, json);
        assert_eq!(found.1, "2026-09-06T10:00:00Z");

        assert!(repo.get("fp-unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_starter_cache_prune_keeps_newest() {
        let (repo, pool) = fresh_repository().await;
        for i in 0..10 {
            repo.put(
                &format!("fp-{:02}", i),
                "[]",
                &format!("2026-09-06T10:{:02}:00Z", i),
            )
            .await
            .unwrap();
        }

        repo.prune(8).await.unwrap();

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_starter_cache")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 8);

        // The two oldest went; the newest stayed.
        assert!(repo.get("fp-00").await.unwrap().is_none());
        assert!(repo.get("fp-01").await.unwrap().is_none());
        assert!(repo.get("fp-09").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_zero_documents_writes_no_cache_row() {
        // Mirrors the short-circuit: nothing touches the table.
        let (_repo, pool) = fresh_repository().await;
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_starter_cache")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }

    /// Everything the prompt sees is drawn from the space's documents, so a
    /// document filed in another space changes neither the mix, nor the newest
    /// timestamp, nor the titles the model is shown.
    #[tokio::test]
    async fn test_the_aggregate_reads_see_only_the_allowed_documents() {
        let (repo, pool) = fresh_repository().await;
        for (i, (kind, indexed)) in [
            ("pdf", "2026-09-01T00:00:00Z"),
            ("pdf", "2026-09-03T00:00:00Z"),
            ("md", "2026-09-02T00:00:00Z"),
            // Filed into another space: invisible to everything below.
            ("epub", "2026-09-09T00:00:00Z"),
        ]
        .iter()
        .enumerate()
        {
            sqlx::query(
                r#"
                INSERT INTO documents (id, file_path, file_name, file_type, mime_type,
                                       size_bytes, modified_at, indexed_at, checksum, status)
                VALUES (?, ?, ?, ?, 'application/octet-stream', 1, ?, ?, ?, 'indexed')
                "#,
            )
            .bind(format!("doc-{}", i))
            .bind(format!("/vault/doc-{}.{}", i, kind))
            .bind(format!("doc-{}.{}", i, kind))
            .bind(*kind)
            .bind(*indexed)
            .bind(*indexed)
            .bind(format!("sum-{}", i))
            .execute(&pool)
            .await
            .unwrap();
        }

        let allowed: HashSet<String> = ["doc-0", "doc-1", "doc-2"]
            .iter()
            .map(|id| id.to_string())
            .collect();

        let mix = repo.type_mix(&allowed).await.unwrap();
        assert_eq!(mix, [("pdf".to_string(), 2), ("md".to_string(), 1)]);

        assert_eq!(
            repo.latest_indexed_at(&allowed).await.unwrap(),
            "2026-09-03T00:00:00Z"
        );

        // Newest first, and the out-of-space document is not among them.
        assert_eq!(
            repo.recent_titles(&allowed).await.unwrap(),
            ["doc-1.pdf", "doc-2.md", "doc-0.pdf"]
        );

        // And an empty scope reads nothing at all.
        let empty = HashSet::new();
        assert!(repo.type_mix(&empty).await.unwrap().is_empty());
        assert!(repo.latest_indexed_at(&empty).await.unwrap().is_empty());
        assert!(repo.recent_titles(&empty).await.unwrap().is_empty());
    }
}
