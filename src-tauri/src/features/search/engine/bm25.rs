use crate::features::search::engine::fts_query::{self, FtsIndex, FtsQuery};
use crate::features::search::BM25SearchTrait;
use crate::infrastructure::persistence::database::connection::{
    query_with_heavy_timeout, query_with_timeout,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct BM25Result {
    pub chunk_id: String,
    pub document_id: String,
    pub score: f32,
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,
}

impl From<BM25Result> for crate::features::search::engine::service::SearchResult {
    fn from(result: BM25Result) -> Self {
        Self {
            id: result.chunk_id.clone(),
            score: result.score,
            index: 0, // BM25 doesn't use index
            filename: result.filename.clone(),
            mime_type: result.mime_type.clone(),
            size_bytes: result.size_bytes,
            created_at: result.created_at.clone(),
            content: result.content.clone(),
            file_id: Some(result.document_id.clone()),
            file_path: None,
            file_name: result.filename,
            file_extension: None,
            file_category: None,
            is_indexed: None,
            document_id: Some(result.document_id),
            snippet: None,
            chunk_index: None,
            updated_at: None,
        }
    }
}

/// Ranked lexical retrieval over the chunk FTS5 indexes.
///
/// Query construction lives in [`fts_query`], which also decides which of the
/// two indexes a query can be answered from.
#[derive(Debug)]
pub struct BM25Search {
    pool: SqlitePool,
}

/// One statement per index. Written out rather than built by string
/// concatenation so `scripts/check-sql-contracts.py` can prepare both against
/// the migration.
const WORDS_SQL: &str = "SELECT
        tc.id as chunk_id,
        tc.document_id as document_id,
        bm25(chunks_fts) as score,
        d.file_name as filename,
        d.mime_type,
        d.size_bytes,
        d.created_at,
        tc.content
     FROM chunks_fts
     JOIN text_chunks tc ON chunks_fts.chunk_id = tc.id
     LEFT JOIN documents d ON tc.document_id = d.id
     WHERE chunks_fts MATCH ?
     ORDER BY score
     LIMIT ?";

const TRIGRAM_SQL: &str = "SELECT
        tc.id as chunk_id,
        tc.document_id as document_id,
        bm25(chunks_trigram) as score,
        d.file_name as filename,
        d.mime_type,
        d.size_bytes,
        d.created_at,
        tc.content
     FROM chunks_trigram
     JOIN text_chunks tc ON chunks_trigram.chunk_id = tc.id
     LEFT JOIN documents d ON tc.document_id = d.id
     WHERE chunks_trigram MATCH ?
     ORDER BY score
     LIMIT ?";

impl BM25Search {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>> {
        let Some(normalized) = fts_query::normalize(query) else {
            return Ok(Vec::new());
        };

        let results: Vec<SqliteRow> = match self.execute_bm25_query(&normalized, top_k).await {
            Ok(rows) => rows,
            Err(error) if Self::is_fts_syntax_error(&error) => {
                let fallback = fts_query::strict_tokenize(query);
                let Some(fallback) = fallback.filter(|fallback| *fallback != normalized) else {
                    return Err(error);
                };

                warn!(
                    original_query = %query,
                    normalized_query = %normalized.match_expression,
                    fallback_query = %fallback.match_expression,
                    error = %error,
                    "BM25 primary FTS query failed with syntax error, retrying with strict tokenized fallback"
                );

                self.execute_bm25_query(&fallback, top_k).await?
            }
            Err(error) => return Err(error),
        };

        let bm25_results = results
            .into_iter()
            .map(|row| {
                let chunk_id: String = row.get("chunk_id");
                let document_id: String = row.get("document_id");
                let score: f32 = row.get("score");
                let filename: Option<String> = row.try_get("filename").ok();
                let mime_type: Option<String> = row.try_get("mime_type").ok();
                let size_bytes: Option<i64> = row.try_get("size_bytes").ok();
                let created_at: Option<String> = row.try_get("created_at").ok();
                let content: Option<String> = row.try_get("content").ok();

                BM25Result {
                    chunk_id,
                    document_id,
                    score: -score,
                    filename,
                    mime_type,
                    size_bytes,
                    created_at,
                    content,
                }
            })
            .collect();

        Ok(bm25_results)
    }

    async fn execute_bm25_query(&self, query: &FtsQuery, top_k: usize) -> Result<Vec<SqliteRow>> {
        let pool = self.pool.clone();
        let sql = match query.index {
            FtsIndex::Words => WORDS_SQL,
            FtsIndex::Trigram => TRIGRAM_SQL,
        };
        let match_expression = query.match_expression.clone();

        query_with_timeout(move || async move {
            sqlx::query(sql)
                .bind(match_expression)
                .bind(top_k as i64)
                .fetch_all(&pool)
                .await
        })
        .await
    }

    fn is_fts_syntax_error(error: &AppError) -> bool {
        matches!(
            error,
            AppError::Database(message) if fts_query::is_syntax_error_message(message)
        )
    }

    pub async fn search_with_filter(
        &self,
        query: &str,
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<BM25Result>> {
        let all_results = self.search(query, top_k * 2).await?;

        Ok(all_results
            .into_iter()
            .filter(|r| r.score >= min_score)
            .take(top_k)
            .collect())
    }

    pub async fn optimize_index(&self) -> Result<()> {
        let pool = self.pool.clone();

        query_with_heavy_timeout(|| async {
            sqlx::query("INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')")
                .execute(&pool)
                .await?;
            sqlx::query("INSERT INTO chunks_trigram(chunks_trigram) VALUES('optimize')")
                .execute(&pool)
                .await
        })
        .await?;
        Ok(())
    }

    pub async fn rebuild_index(&self) -> Result<()> {
        let pool = self.pool.clone();

        query_with_heavy_timeout(|| async {
            sqlx::query("INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')")
                .execute(&pool)
                .await?;
            sqlx::query("INSERT INTO chunks_trigram(chunks_trigram) VALUES('rebuild')")
                .execute(&pool)
                .await
        })
        .await?;
        Ok(())
    }

    pub fn supports_query(query: &str) -> bool {
        !query.trim().is_empty()
    }
}

#[async_trait]
impl BM25SearchTrait for BM25Search {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>> {
        self.search(query, top_k).await
    }

    async fn search_with_filter(
        &self,
        query: &str,
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<BM25Result>> {
        self.search_with_filter(query, top_k, min_score).await
    }

    async fn optimize_index(&self) -> Result<()> {
        self.optimize_index().await
    }

    async fn rebuild_index(&self) -> Result<()> {
        self.rebuild_index().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// The two chunk indexes and the triggers that keep them in step, copied
    /// from `migrations/20260916000000_init_schema.sql`.
    const FTS_SCHEMA: &str = "
        CREATE VIRTUAL TABLE chunks_fts USING fts5(
            chunk_id UNINDEXED,
            content,
            tokenize='porter unicode61 remove_diacritics 2'
        );
        CREATE VIRTUAL TABLE chunks_trigram USING fts5(
            chunk_id UNINDEXED,
            content,
            tokenize='trigram'
        );
        CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
          INSERT INTO chunks_fts(chunk_id, content)
          VALUES(new.id, COALESCE(new.contextualized_content, new.content));
          INSERT INTO chunks_trigram(chunk_id, content)
          VALUES(new.id, COALESCE(new.contextualized_content, new.content));
        END;
    ";

    async fn setup_test_db() -> Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await?;

        sqlx::raw_sql(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                file_name TEXT NOT NULL,
                file_path TEXT,
                mime_type TEXT,
                size_bytes INTEGER,
                created_at TEXT NOT NULL
            );
            CREATE TABLE text_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                contextualized_content TEXT,
                chunk_index INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            );",
        )
        .execute(&pool)
        .await?;
        sqlx::raw_sql(FTS_SCHEMA).execute(&pool).await?;

        sqlx::query(
            "INSERT INTO documents (id, file_name, mime_type, size_bytes, created_at) VALUES
             ('doc1', 'rust.txt', 'text/plain', 1024, '2024-01-01'),
             ('doc2', 'python.txt', 'text/plain', 2048, '2024-01-02'),
             ('doc3', 'ml.txt', 'text/plain', 3072, '2024-01-03'),
             ('doc4', 'incidents.txt', 'text/plain', 512, '2024-01-04'),
             ('doc5', 'support-ja.txt', 'text/plain', 512, '2024-01-05'),
             ('doc6', 'policy-ru.txt', 'text/plain', 512, '2024-01-06')",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES
             ('chunk1', 'doc1', 'Rust is a systems programming language', 0),
             ('chunk2', 'doc2', 'Python is a high-level programming language', 0),
             ('chunk3', 'doc3', 'Machine learning with Python and neural networks', 0),
             ('chunk4', 'doc4', 'Incident ERR-4012 was raised in 2026 after a 429 response', 0),
             ('chunk5', 'doc5', '日本語サポート時間。担当チームは日本標準時の月曜日から金曜日まで対応します。', 0),
             ('chunk6', 'doc6', 'Политика хранения резервных копий рабочего пространства', 0)",
        )
        .execute(&pool)
        .await?;

        Ok(pool)
    }

    #[tokio::test]
    async fn test_bm25_search() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);

        let results = search.search("programming", 10).await.unwrap();

        assert!(results.len() >= 2);
        assert!(results[0].score > 0.0);
        assert!(results[0].filename.is_some());
    }

    #[tokio::test]
    async fn test_bm25_exact_keyword() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);

        let results = search.search("Rust", 10).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "chunk1");
        assert_eq!(results[0].document_id, "doc1");
        assert_eq!(results[0].filename.as_deref(), Some("rust.txt"));
    }

    #[tokio::test]
    async fn test_bm25_multiple_terms() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);

        let results = search.search("Python neural", 10).await.unwrap();

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_supports_query() {
        assert!(BM25Search::supports_query("test query"));
        assert!(!BM25Search::supports_query(""));
        assert!(!BM25Search::supports_query("   "));
    }

    /// The regression this file exists for: a query with no ASCII letter used
    /// to produce an empty `MATCH` expression and no lexical branch at all.
    #[tokio::test]
    async fn a_digits_only_query_finds_its_chunk() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);

        for query in ["4012", "429", "2026"] {
            let results = search.search(query, 10).await.unwrap();
            assert_eq!(
                results.first().map(|r| r.chunk_id.as_str()),
                Some("chunk4"),
                "query {query} found {results:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_identifier_keeps_both_halves() {
        let pool = setup_test_db().await.unwrap();
        let results = BM25Search::new(pool).search("ERR-4012", 10).await.unwrap();
        assert_eq!(results.first().map(|r| r.chunk_id.as_str()), Some("chunk4"));
    }

    #[tokio::test]
    async fn a_cyrillic_query_finds_its_chunk() {
        let pool = setup_test_db().await.unwrap();
        let results = BM25Search::new(pool)
            .search("политика хранения", 10)
            .await
            .unwrap();
        assert_eq!(results.first().map(|r| r.chunk_id.as_str()), Some("chunk6"));
    }

    /// `unicode61` indexes the whole Han/Kana run as one token, so this can
    /// only be answered from the trigram index.
    #[tokio::test]
    async fn a_japanese_substring_query_finds_its_chunk() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);
        for query in ["日本語サポート", "担当チーム", "金曜日"] {
            let results = search.search(query, 10).await.unwrap();
            assert_eq!(
                results.first().map(|r| r.chunk_id.as_str()),
                Some("chunk5"),
                "query {query} found {results:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_two_character_cjk_query_returns_nothing_rather_than_failing() {
        let pool = setup_test_db().await.unwrap();
        let results = BM25Search::new(pool).search("設定", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn hostile_and_empty_queries_do_not_error() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);
        for query in [
            "",
            "   ",
            "???",
            "rust\" OR chunks_fts MATCH \"python",
            "alpha NEAR(beta gamma) AND NOT delta",
            "programming*",
            "col:value",
        ] {
            let results = search.search(query, 10).await;
            assert!(results.is_ok(), "query {query:?} errored: {results:?}");
        }
    }

    #[tokio::test]
    async fn maintenance_commands_cover_both_indexes() {
        let pool = setup_test_db().await.unwrap();
        let search = BM25Search::new(pool);
        search.rebuild_index().await.unwrap();
        search.optimize_index().await.unwrap();
        assert!(!search
            .search("日本語サポート", 10)
            .await
            .unwrap()
            .is_empty());
        assert!(!search.search("programming", 10).await.unwrap().is_empty());
    }
}
