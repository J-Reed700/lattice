//! Scoring stored sparse postings against a sparse query vector.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::application::ports::EmbeddingPort;
use crate::domain::value_objects::SparseEmbedding;
use crate::features::search::dto::SearchResultPortDto;
use crate::features::search::SparseSearchTrait;
use crate::shared::result::Result;

/// Learned sparse retrieval over `chunk_sparse_terms`.
///
/// The query is embedded with the loaded model's sparse head, and every chunk
/// that shares at least one term is scored by the dot product of the two term
/// vectors — the standard sparse-retrieval score, run in SQL so only the
/// matching postings are ever read.
pub struct SparseSearchService {
    pool: SqlitePool,
    embedder: Arc<dyn EmbeddingPort>,
}

impl SparseSearchService {
    pub fn new(pool: SqlitePool, embedder: Arc<dyn EmbeddingPort>) -> Self {
        Self { pool, embedder }
    }

    /// Score an already-embedded sparse query. Separated from `search_scoped`
    /// so the SQL can be tested without loading a model.
    pub async fn score_scoped(
        &self,
        query: &SparseEmbedding,
        model_identity: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        if query.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }
        // Matches the BM25 branch: an explicitly empty scope allows nothing,
        // which is not the same thing as "no scope given".
        if allowed_document_ids.is_some_and(HashSet::is_empty) {
            return Ok(Vec::new());
        }

        // The query's active terms become an inline relation the posting table
        // joins against, so SQLite reads only the postings for those term ids
        // via idx_chunk_sparse_terms_lookup.
        let mut sql =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new("WITH q(term_id, weight) AS (VALUES ");
        for (position, (term_id, weight)) in query.iter().enumerate() {
            if position > 0 {
                sql.push(", ");
            }
            sql.push("(");
            sql.push_bind(i64::from(term_id));
            sql.push(", ");
            sql.push_bind(f64::from(weight));
            sql.push(")");
        }
        sql.push(
            ") SELECT c.id AS chunk_id, c.document_id AS document_id, c.content AS content, \
             SUM(s.weight * q.weight) AS score \
             FROM chunk_sparse_terms s \
             JOIN q ON q.term_id = s.term_id \
             JOIN text_chunks c ON c.id = s.chunk_id \
             WHERE s.model_identity = ",
        );
        sql.push_bind(model_identity);
        // Every scope restriction is applied before ORDER BY / LIMIT, exactly
        // as the BM25 branch does: filtering the global top hits afterwards can
        // hide every hit inside a selected document.
        if let Some(space) = space_id {
            sql.push(
                " AND EXISTS (SELECT 1 FROM document_space_memberships m \
                 WHERE m.document_id = c.document_id AND m.space_id = ",
            );
            sql.push_bind(space).push(")");
        }
        if let Some(ids) = allowed_document_ids {
            sql.push(" AND c.document_id IN (SELECT value FROM json_each(");
            // One bind for the whole list. A bind per document runs into
            // SQLite's 32,766-variable limit once a space is large enough, and
            // every search in that space then fails.
            let mut sorted: Vec<_> = ids.iter().collect();
            sorted.sort();
            sql.push_bind(serde_json::json!(sorted).to_string());
            sql.push("))");
        }
        sql.push(" GROUP BY c.id, c.document_id, c.content ORDER BY score DESC, c.id LIMIT ")
            .push_bind(top_k as i64);

        let rows = sql.build().fetch_all(&self.pool).await?;
        let mut results = rows_to_results(rows);
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
        }
        results.truncate(top_k);
        Ok(results)
    }
}

#[async_trait]
impl SparseSearchTrait for SparseSearchService {
    fn is_available(&self) -> bool {
        self.embedder.supports_sparse()
    }

    async fn search_scoped(
        &self,
        query: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        if query.trim().is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }
        let sparse_query = self.embedder.embed_sparse_query(query).await?;
        let model_identity = self.embedder.model_identity();
        self.score_scoped(
            &sparse_query,
            &model_identity,
            top_k,
            space_id,
            allowed_document_ids,
        )
        .await
    }
}

/// Normalize to 0-1 against the batch maximum, matching the BM25 branch.
///
/// RRF only reads rank, so this changes no ordering; it keeps the `score`
/// field on the DTO comparable with the other branches for anything that
/// inspects it (debug logs, the UI's per-branch score columns).
fn rows_to_results(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<SearchResultPortDto> {
    use sqlx::Row;

    let mut raw = Vec::with_capacity(rows.len());
    let mut max_score = 0.0f32;
    for row in rows {
        let chunk_id: String = row.get("chunk_id");
        let document_id: String = row.get("document_id");
        let content: String = row.get("content");
        let score = row.get::<f64, _>("score") as f32;
        let score = if score.is_finite() {
            score.max(0.0)
        } else {
            0.0
        };
        max_score = max_score.max(score);
        raw.push((chunk_id, document_id, content, score));
    }
    raw.into_iter()
        .map(|(chunk_id, doc_id, content, score)| SearchResultPortDto {
            doc_id,
            chunk_id,
            content,
            score: if max_score > 0.0 {
                (score / max_score).clamp(0.0, 1.0)
            } else {
                0.0
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{ChunkSparseTerms, SparseTermStorePort};
    use crate::features::search::engine::sparse_search::store::SqliteSparseTermStore;

    const MODEL: &str = "sha256:model-a";
    const OTHER_MODEL: &str = "sha256:model-b";

    async fn fixture() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE text_chunks(id TEXT PRIMARY KEY, document_id TEXT, content TEXT);
             CREATE TABLE document_space_memberships(document_id TEXT, space_id TEXT);
             CREATE TABLE chunk_sparse_terms (
                chunk_id TEXT NOT NULL,
                model_identity TEXT NOT NULL,
                term_id INTEGER NOT NULL,
                weight REAL NOT NULL,
                PRIMARY KEY (chunk_id, model_identity, term_id));
             CREATE INDEX idx_chunk_sparse_terms_lookup
                ON chunk_sparse_terms(model_identity, term_id);
             INSERT INTO text_chunks VALUES
                ('c1','doc-a','alpha beta'),
                ('c2','doc-b','beta gamma'),
                ('c3','doc-c','delta');
             INSERT INTO document_space_memberships VALUES ('doc-a','space-1'),('doc-b','space-2');",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn entry(chunk: &str, pairs: &[(u32, f32)]) -> ChunkSparseTerms {
        (
            chunk.to_owned(),
            SparseEmbedding::from_pairs(pairs.iter().copied()),
        )
    }

    fn service(pool: SqlitePool) -> SparseSearchService {
        SparseSearchService::new(pool, Arc::new(NoSparseEmbedder))
    }

    /// An embedder without a sparse head — the default trait path.
    struct NoSparseEmbedder;

    #[async_trait]
    impl EmbeddingPort for NoSparseEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0]).collect())
        }
        fn dimension(&self) -> usize {
            1
        }
        fn model_identity(&self) -> String {
            MODEL.to_owned()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn scores_by_dot_product_over_shared_terms() {
        let pool = fixture().await;
        let store = SqliteSparseTermStore::new(pool.clone());
        store
            .replace_chunk_terms(
                MODEL,
                &[
                    entry("c1", &[(10, 0.5), (11, 2.0)]),
                    entry("c2", &[(11, 0.5), (12, 4.0)]),
                    entry("c3", &[(99, 9.0)]),
                ],
            )
            .await
            .unwrap();

        // query activates 10 (w=1.0) and 11 (w=1.0):
        //   c1 = 0.5*1 + 2.0*1 = 2.5   c2 = 0.5*1 = 0.5   c3 = no shared term
        let query = SparseEmbedding::from_pairs([(10, 1.0), (11, 1.0)]);
        let hits = service(pool)
            .score_scoped(&query, MODEL, 10, None, None)
            .await
            .unwrap();
        assert_eq!(
            hits.iter().map(|h| h.chunk_id.as_str()).collect::<Vec<_>>(),
            vec!["c1", "c2"]
        );
        // Normalized against the batch maximum, so the ratio is preserved.
        let top = hits.first().unwrap();
        let second = hits.get(1).unwrap();
        assert!((top.score - 1.0).abs() < 1e-6);
        assert!((second.score - 0.2).abs() < 1e-6);
        assert_eq!(top.doc_id, "doc-a");
        assert_eq!(top.content, "alpha beta");
    }

    #[tokio::test]
    async fn postings_never_cross_model_identities() {
        let pool = fixture().await;
        let store = SqliteSparseTermStore::new(pool.clone());
        store
            .replace_chunk_terms(MODEL, &[entry("c1", &[(10, 1.0)])])
            .await
            .unwrap();
        store
            .replace_chunk_terms(OTHER_MODEL, &[entry("c2", &[(10, 1.0)])])
            .await
            .unwrap();

        let query = SparseEmbedding::from_pairs([(10, 1.0)]);
        let service = service(pool);
        let mine = service
            .score_scoped(&query, MODEL, 10, None, None)
            .await
            .unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine.first().unwrap().chunk_id, "c1");

        // A model nobody indexed with finds nothing rather than scoring against
        // another tokenizer's term ids.
        assert!(service
            .score_scoped(&query, "sha256:never-used", 10, None, None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn scope_filters_match_the_bm25_branch() {
        let pool = fixture().await;
        let store = SqliteSparseTermStore::new(pool.clone());
        store
            .replace_chunk_terms(
                MODEL,
                &[entry("c1", &[(10, 1.0)]), entry("c2", &[(10, 1.0)])],
            )
            .await
            .unwrap();
        let query = SparseEmbedding::from_pairs([(10, 1.0)]);
        let service = service(pool);

        let by_space = service
            .score_scoped(&query, MODEL, 10, Some("space-1"), None)
            .await
            .unwrap();
        assert_eq!(by_space.len(), 1);
        assert_eq!(by_space.first().unwrap().doc_id, "doc-a");

        let allowed = HashSet::from(["doc-b".to_owned()]);
        let by_document = service
            .score_scoped(&query, MODEL, 10, None, Some(&allowed))
            .await
            .unwrap();
        assert_eq!(by_document.len(), 1);
        assert_eq!(by_document.first().unwrap().doc_id, "doc-b");

        // The scope is applied before the limit: asking for one result from a
        // selected document cannot return the other document's better hit.
        let single = service
            .score_scoped(&query, MODEL, 1, None, Some(&allowed))
            .await
            .unwrap();
        assert_eq!(single.first().map(|h| h.doc_id.as_str()), Some("doc-b"));

        // An empty allow-list allows nothing.
        assert!(service
            .score_scoped(&query, MODEL, 10, None, Some(&HashSet::new()))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn reindexing_a_chunk_replaces_its_terms() {
        let pool = fixture().await;
        let store = SqliteSparseTermStore::new(pool.clone());
        store
            .replace_chunk_terms(MODEL, &[entry("c1", &[(10, 1.0), (11, 1.0)])])
            .await
            .unwrap();
        store
            .replace_chunk_terms(MODEL, &[entry("c1", &[(11, 0.25)])])
            .await
            .unwrap();

        let service = service(pool.clone());
        // Term 10 is gone, not merely outweighed.
        assert!(service
            .score_scoped(
                &SparseEmbedding::from_pairs([(10, 1.0)]),
                MODEL,
                10,
                None,
                None
            )
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            service
                .score_scoped(
                    &SparseEmbedding::from_pairs([(11, 1.0)]),
                    MODEL,
                    10,
                    None,
                    None
                )
                .await
                .unwrap()
                .len(),
            1
        );

        store.delete_chunk_terms(&["c1".to_owned()]).await.unwrap();
        assert!(service
            .score_scoped(
                &SparseEmbedding::from_pairs([(11, 1.0)]),
                MODEL,
                10,
                None,
                None
            )
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn an_embedder_without_a_sparse_head_reports_unavailable_and_refuses() {
        let pool = fixture().await;
        let service = service(pool);
        assert!(!service.is_available());

        // The default port path: not a retrieval failure, a "no such capability".
        let error = SparseSearchTrait::search_scoped(&service, "anything", 5, None, None)
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("not supported"),
            "unexpected error: {error}"
        );

        // An empty query short-circuits before the embedder is ever consulted.
        assert!(
            SparseSearchTrait::search_scoped(&service, "   ", 5, None, None)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
