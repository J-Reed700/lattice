//! SQLite FTS5 text search behind [`TextSearchPort`].
//!
//! The scoped sibling of [`BM25Search`]: same indexes, same query construction
//! (both go through [`fts_query`]), but this one applies workspace and
//! document scope inside the SQL and reports the port's DTO. Chat retrieval
//! needs the scoping; direct search needs the richer document metadata.
//!
//! [`BM25Search`]: crate::features::search::engine::bm25::BM25Search

use crate::application::ports::TextSearchPort;
use crate::features::search::dto::SearchResultPortDto;
use crate::features::search::engine::fts_query::{self, FtsIndex, FtsQuery};
use crate::shared::result::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::collections::HashSet;
use tracing::warn;

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[tokio::test]
    async fn selected_document_filter_runs_before_limit() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT);
            CREATE TABLE document_space_memberships(document_id TEXT, space_id TEXT);
            CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED, content);
            CREATE VIRTUAL TABLE chunks_trigram USING fts5(chunk_id UNINDEXED, content, tokenize='trigram');
            INSERT INTO text_chunks VALUES ('a','other','patent'), ('b','selected','patent manual introduction');
            INSERT INTO chunks_fts SELECT id,content FROM text_chunks;
            INSERT INTO chunks_trigram SELECT id,content FROM text_chunks;
            INSERT INTO document_space_memberships VALUES ('other','space'),('selected','space');")
            .execute(&pool).await.unwrap();
        let search = SqliteTextSearch::new(pool);
        let allowed = HashSet::from(["selected".into()]);
        for space in [None, Some("space")] {
            let hits = search
                .search_scoped("patent", 1, space, Some(&allowed))
                .await
                .unwrap();
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].doc_id, "selected");
        }
        assert!(search
            .search_scoped("patent", 1, Some("other_space"), Some(&allowed))
            .await
            .unwrap()
            .is_empty());
        assert!(search
            .search_scoped("patent", 1, None, Some(&HashSet::new()))
            .await
            .unwrap()
            .is_empty());
    }

    /// A space's whole document list arrives here as the allow-list, so it
    /// must not cost one SQL variable per document: SQLite allows 32,766.
    #[tokio::test]
    async fn an_allow_list_larger_than_sqlites_variable_limit_still_searches() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT);
            CREATE TABLE document_space_memberships(document_id TEXT, space_id TEXT);
            CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED, content);
            CREATE VIRTUAL TABLE chunks_trigram USING fts5(chunk_id UNINDEXED, content, tokenize='trigram');
            INSERT INTO text_chunks VALUES ('a','other','patent'), ('b','doc-7','patent manual');
            INSERT INTO chunks_fts SELECT id,content FROM text_chunks;
            INSERT INTO chunks_trigram SELECT id,content FROM text_chunks;")
            .execute(&pool).await.unwrap();
        let allowed: HashSet<String> = (0..40_000).map(|n| format!("doc-{n}")).collect();

        let hits = SqliteTextSearch::new(pool)
            .search_scoped("patent", 5, None, Some(&allowed))
            .await
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, "doc-7");
    }

    /// From a real turn: markdown emphasis made the query look like explicit
    /// FTS syntax, FTS5 read `- The` as a column filter, and the keyword branch
    /// failed with "no such column: The" instead of falling back.
    #[tokio::test]
    async fn prose_that_fts5_reads_as_a_column_filter_still_searches() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT);
            CREATE TABLE document_space_memberships(document_id TEXT, space_id TEXT);
            CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id UNINDEXED, content);
            INSERT INTO text_chunks VALUES ('a','silo','a giant underground structure of many levels');
            INSERT INTO chunks_fts SELECT id,content FROM text_chunks;")
            .execute(&pool).await.unwrap();
        let search = SqliteTextSearch::new(pool);

        let hits = search
            .search_scoped(
                "The world and its rules - The Silo is a giant underground structure of **144 levels**",
                5,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc_id, "silo");
    }
}

/// SQLite-based text search implementation using FTS5
///
/// Uses the chunks_fts and chunks_trigram virtual tables, which are kept in
/// sync with text_chunks by triggers (see the schema migration).
pub struct SqliteTextSearch {
    pool: SqlitePool,
}

impl SqliteTextSearch {
    /// Create a new SQLite text search instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TextSearchPort for SqliteTextSearch {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResultPortDto>> {
        self.search_scoped(query, top_k, None, None).await
    }

    async fn search_scoped(
        &self,
        query: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        let Some(normalized) = fts_query::normalize(query) else {
            return Ok(Vec::new());
        };

        if allowed_document_ids.is_some_and(HashSet::is_empty) || top_k == 0 {
            return Ok(Vec::new());
        }
        let rows_result = Self::execute_fts_query_scoped(
            &self.pool,
            &normalized,
            top_k,
            space_id,
            allowed_document_ids,
        )
        .await;

        let rows = match rows_result {
            Ok(rows) => rows,
            Err(error) if Self::is_fts_syntax_error(&error) => {
                let fallback = fts_query::strict_tokenize(query);
                let Some(fallback) = fallback.filter(|fallback| *fallback != normalized) else {
                    return Err(error.into());
                };

                warn!(
                    original_query = %query,
                    normalized_query = %normalized.match_expression,
                    fallback_query = %fallback.match_expression,
                    error = %error,
                    "SQLite text search FTS query failed with syntax error, retrying with strict tokenized fallback"
                );

                Self::execute_fts_query_scoped(
                    &self.pool,
                    &fallback,
                    top_k,
                    space_id,
                    allowed_document_ids,
                )
                .await?
            }
            Err(error) => return Err(error.into()),
        };

        let mut results = Self::rows_to_results(rows);
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
        }
        if results.len() > top_k {
            results.truncate(top_k);
        }
        Ok(results)
    }

    async fn index_document(&self, _id: &str, _content: &str) -> Result<()> {
        // No-op: both FTS5 tables are synced by the text_chunks triggers.
        Ok(())
    }

    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        // No-op: both FTS5 tables are synced by the text_chunks triggers.
        Ok(())
    }

    async fn remove_document(&self, _id: &str) -> Result<()> {
        // No-op: both FTS5 tables are synced by the text_chunks triggers.
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        // Both chunk-level indexes, or the next query answers from a stale one.
        sqlx::query("DELETE FROM chunks_fts")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM chunks_trigram")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        // Count rows in FTS5 table (chunk-level)
        use sqlx::Row;
        let row = sqlx::query("SELECT COUNT(*) as count FROM chunks_fts")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("count") as usize)
    }
}

impl SqliteTextSearch {
    async fn execute_fts_query_scoped(
        pool: &SqlitePool,
        query: &FtsQuery,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> std::result::Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        // Apply ALL scope restrictions before ranking/LIMIT. Filtering the top
        // global hits afterwards can hide every hit from a selected chapter.
        //
        // The table name is chosen from a closed set, never from user input.
        let mut sql = match query.index {
            FtsIndex::Words => sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT c.id as chunk_id, c.document_id, c.content, bm25(chunks_fts) as score \
                 FROM chunks_fts JOIN text_chunks c ON chunks_fts.chunk_id = c.id \
                 WHERE chunks_fts MATCH ",
            ),
            FtsIndex::Trigram => sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT c.id as chunk_id, c.document_id, c.content, bm25(chunks_trigram) as score \
                 FROM chunks_trigram JOIN text_chunks c ON chunks_trigram.chunk_id = c.id \
                 WHERE chunks_trigram MATCH ",
            ),
        };
        let order = match query.index {
            FtsIndex::Words => " ORDER BY bm25(chunks_fts), c.id LIMIT ",
            FtsIndex::Trigram => " ORDER BY bm25(chunks_trigram), c.id LIMIT ",
        };
        sql.push_bind(query.match_expression.clone());
        if let Some(space) = space_id {
            sql.push(" AND EXISTS (SELECT 1 FROM document_space_memberships m WHERE m.document_id = c.document_id AND m.space_id = ");
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
        sql.push(order).push_bind(top_k as i64);
        sql.build().fetch_all(pool).await
    }

    fn rows_to_results(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<SearchResultPortDto> {
        use sqlx::Row;

        let mut raw_results = Vec::with_capacity(rows.len());
        let mut max_score = 0.0_f32;
        for row in rows {
            let chunk_id: String = row.get("chunk_id");
            let document_id: String = row.get("document_id");
            let content: String = row.get("content");
            let score: f64 = row.get("score");
            let raw_score = (-score as f32).max(0.0); // BM25 returns negative scores, invert
            max_score = max_score.max(raw_score);
            raw_results.push((chunk_id, document_id, content, raw_score));
        }

        let normalize = |score: f32| -> f32 {
            if max_score > 0.0 {
                (score / max_score).clamp(0.0, 1.0)
            } else {
                0.0
            }
        };

        raw_results
            .into_iter()
            .map(
                |(chunk_id, document_id, content, raw_score)| SearchResultPortDto {
                    doc_id: document_id,
                    chunk_id,
                    content,
                    score: normalize(raw_score),
                },
            )
            .collect()
    }

    fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
        fts_query::is_syntax_error_message(&error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn corpus() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE text_chunks(id TEXT, document_id TEXT, content TEXT);
             CREATE TABLE document_space_memberships(document_id TEXT, space_id TEXT);
             CREATE VIRTUAL TABLE chunks_fts USING fts5(
                chunk_id UNINDEXED, content,
                tokenize='porter unicode61 remove_diacritics 2');
             CREATE VIRTUAL TABLE chunks_trigram USING fts5(
                chunk_id UNINDEXED, content, tokenize='trigram');
             INSERT INTO text_chunks VALUES
                ('a','doc-a','Incident ERR-4012 raised in 2026 after a 429 response'),
                ('b','doc-b','日本語サポート時間。担当チームは月曜日から金曜日まで対応します。'),
                ('c','doc-c','Политика хранения резервных копий'),
                ('d','doc-d','El horario de atención en español incluye los sábados');
             INSERT INTO chunks_fts SELECT id, content FROM text_chunks;
             INSERT INTO chunks_trigram SELECT id, content FROM text_chunks;",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn top(query: &str) -> Option<String> {
        let search = SqliteTextSearch::new(corpus().await);
        search
            .search_scoped(query, 5, None, None)
            .await
            .unwrap()
            .first()
            .map(|hit| hit.doc_id.clone())
    }

    #[tokio::test]
    async fn numeric_cyrillic_and_accented_queries_all_reach_the_index() {
        assert_eq!(top("4012").await.as_deref(), Some("doc-a"));
        assert_eq!(top("429").await.as_deref(), Some("doc-a"));
        assert_eq!(top("политика хранения").await.as_deref(), Some("doc-c"));
        assert_eq!(top("horario español").await.as_deref(), Some("doc-d"));
    }

    #[tokio::test]
    async fn a_japanese_substring_query_uses_the_trigram_index() {
        assert_eq!(top("日本語サポート").await.as_deref(), Some("doc-b"));
        assert_eq!(top("金曜日").await.as_deref(), Some("doc-b"));
    }

    #[tokio::test]
    async fn hostile_and_empty_queries_do_not_error() {
        let search = SqliteTextSearch::new(corpus().await);
        for query in [
            "",
            "  ",
            "!!!",
            "incident\" OR chunks_fts MATCH \"response",
            "alpha NEAR(beta gamma) AND NOT delta",
            "col:value -minus (paren)",
        ] {
            let hits = search.search_scoped(query, 5, None, None).await;
            assert!(hits.is_ok(), "query {query:?} errored: {hits:?}");
        }
    }

    #[tokio::test]
    async fn clearing_empties_both_indexes() {
        let pool = corpus().await;
        let search = SqliteTextSearch::new(pool.clone());
        search.clear().await.unwrap();
        assert!(search
            .search_scoped("incident", 5, None, None)
            .await
            .unwrap()
            .is_empty());
        assert!(search
            .search_scoped("日本語サポート", 5, None, None)
            .await
            .unwrap()
            .is_empty());
    }
}
