use crate::features::search::BM25SearchTrait;
use crate::infrastructure::persistence::database::connection::{
    query_with_heavy_timeout, query_with_timeout,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
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

#[derive(Debug)]
pub struct BM25Search {
    pool: SqlitePool,
}

impl BM25Search {
    const MIN_TERM_LEN: usize = 3;
    const MAX_TERMS: usize = 16;

    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>> {
        let normalized_query = Self::normalize_fts_query(query);
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }

        let results: Vec<SqliteRow> = match self.execute_bm25_query(&normalized_query, top_k).await
        {
            Ok(rows) => rows,
            Err(error) if Self::is_fts_syntax_error(&error) => {
                let fallback_query = Self::strict_tokenize_fts_query(query);
                if fallback_query.is_empty() || fallback_query == normalized_query {
                    return Err(error);
                }

                warn!(
                    original_query = %query,
                    normalized_query = %normalized_query,
                    fallback_query = %fallback_query,
                    error = %error,
                    "BM25 primary FTS query failed with syntax error, retrying with strict tokenized fallback"
                );

                self.execute_bm25_query(&fallback_query, top_k).await?
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

    async fn execute_bm25_query(&self, query: &str, top_k: usize) -> Result<Vec<SqliteRow>> {
        let pool = self.pool.clone();
        let query_string = query.to_string();

        query_with_timeout(move || async move {
            sqlx::query(
                "SELECT
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
                 LIMIT ?",
            )
            .bind(query_string)
            .bind(top_k as i64)
            .fetch_all(&pool)
            .await
        })
        .await
    }

    /// Normalize free-text input into an FTS5 query.
    ///
    /// FTS5 treats whitespace as implicit AND. For natural-language queries and
    /// expanded synonym lists we prefer OR semantics to preserve recall.
    /// If the input appears to already use FTS operators/syntax, keep it as-is.
    fn normalize_fts_query(query: &str) -> String {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        if Self::looks_like_explicit_fts_syntax(trimmed) {
            return trimmed.to_string();
        }

        Self::strict_tokenize_fts_query(trimmed)
    }

    fn strict_tokenize_fts_query(query: &str) -> String {
        let mut terms: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in query.chars() {
            if ch.is_alphanumeric() {
                for lower in ch.to_lowercase() {
                    current.push(lower);
                }
            } else if !current.is_empty() {
                terms.push(current);
                current = String::new();
            }
        }
        if !current.is_empty() {
            terms.push(current);
        }

        let mut seen = std::collections::HashSet::new();
        let deduped_terms: Vec<String> = terms
            .into_iter()
            .filter_map(|term| {
                if term.is_empty() || !seen.insert(term.clone()) {
                    None
                } else {
                    Some(term)
                }
            })
            .collect();

        if deduped_terms.is_empty() {
            return String::new();
        }

        let mut filtered_terms = Self::select_informative_terms(deduped_terms, Self::MAX_TERMS);
        if filtered_terms.is_empty() {
            return String::new();
        }

        let mut seen = filtered_terms
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let selected_base = filtered_terms.clone();
        for term in selected_base {
            let stem = Self::stem_term(term.as_str());
            if stem != term && stem.len() >= Self::MIN_TERM_LEN && seen.insert(stem.clone()) {
                filtered_terms.push(stem);
                if filtered_terms.len() >= Self::MAX_TERMS {
                    break;
                }
            }
        }

        if filtered_terms.len() <= 1 {
            filtered_terms.first().cloned().unwrap_or_default()
        } else {
            filtered_terms.join(" OR ")
        }
    }

    fn looks_like_explicit_fts_syntax(query: &str) -> bool {
        query.contains('"')
            || query.contains('*')
            || query.contains(" OR ")
            || query.contains(" AND ")
            || query.contains(" NOT ")
            || query.contains(" NEAR ")
            || query.contains(" NEAR(")
    }

    fn normalize_term(term: &str) -> Option<String> {
        let trimmed = term.trim();
        if trimmed.len() < Self::MIN_TERM_LEN {
            return None;
        }
        if !trimmed.chars().any(|ch| ch.is_ascii_alphabetic()) {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn stem_term(term: &str) -> String {
        static EN_STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));
        EN_STEMMER.stem(term).to_string()
    }

    fn term_entropy(term: &str) -> f32 {
        use std::collections::HashMap;

        let len = term.len();
        if len == 0 {
            return 0.0;
        }

        let mut counts: HashMap<char, usize> = HashMap::new();
        for ch in term.chars() {
            *counts.entry(ch).or_insert(0) += 1;
        }

        let denom = len as f32;
        counts.values().fold(0.0_f32, |acc, count| {
            let p = (*count as f32) / denom;
            if p <= f32::EPSILON {
                acc
            } else {
                acc - p * p.log2()
            }
        })
    }

    fn term_salience(term: &str) -> f32 {
        let entropy = Self::term_entropy(term);
        let length_factor = ((term.len() as f32) + 1.0).ln();
        entropy * (0.65 + 0.35 * length_factor)
    }

    fn select_informative_terms(terms: Vec<String>, max_terms: usize) -> Vec<String> {
        let mut deduped = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for term in terms {
            if let Some(normalized) = Self::normalize_term(&term) {
                if seen.insert(normalized.clone()) {
                    deduped.push(normalized);
                }
            }
        }
        if deduped.is_empty() {
            return Vec::new();
        }

        let mut ranked: Vec<(String, f32)> = deduped
            .into_iter()
            .map(|term| {
                let score = Self::term_salience(&term);
                (term, score)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let best_score = ranked.first().map(|(_, score)| *score).unwrap_or(0.0);
        if best_score <= f32::EPSILON {
            return ranked
                .into_iter()
                .take(max_terms)
                .map(|(term, _)| term)
                .collect();
        }

        let mut selected: Vec<String> = ranked
            .iter()
            .filter_map(|(term, score)| (*score >= best_score * 0.42).then_some(term.clone()))
            .take(max_terms)
            .collect();
        if selected.is_empty() {
            selected = ranked
                .into_iter()
                .take(max_terms.min(3))
                .map(|(term, _)| term)
                .collect();
        }
        selected
    }

    fn is_fts_syntax_error(error: &AppError) -> bool {
        matches!(
            error,
            AppError::Database(message)
                if message.contains("fts5: syntax error")
                    || message.contains("malformed MATCH expression")
                    || message.contains("unterminated string")
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

    async fn setup_test_db() -> Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await?;

        sqlx::query(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                file_name TEXT NOT NULL,
                file_path TEXT,
                mime_type TEXT,
                size_bytes INTEGER,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE text_chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id)
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE VIRTUAL TABLE chunks_fts USING fts5(
                chunk_id UNINDEXED,
                content,
                tokenize='porter unicode61 remove_diacritics 2'
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO documents (id, file_name, mime_type, size_bytes, created_at) VALUES
             ('doc1', 'rust.txt', 'text/plain', 1024, '2024-01-01'),
             ('doc2', 'python.txt', 'text/plain', 2048, '2024-01-02'),
             ('doc3', 'ml.txt', 'text/plain', 3072, '2024-01-03')",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES
             ('chunk1', 'doc1', 'Rust is a systems programming language', 0),
             ('chunk2', 'doc2', 'Python is a high-level programming language', 0),
             ('chunk3', 'doc3', 'Machine learning with Python and neural networks', 0)",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO chunks_fts (chunk_id, content) VALUES
             ('chunk1', 'Rust is a systems programming language'),
             ('chunk2', 'Python is a high-level programming language'),
             ('chunk3', 'Machine learning with Python and neural networks')",
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

    #[test]
    fn test_normalize_fts_query_tokenizes_natural_language() {
        let normalized = BM25Search::normalize_fts_query("What has ICE been doing??");
        let terms: Vec<&str> = normalized.split(" OR ").collect();
        assert!(terms
            .iter()
            .all(|term| term.len() >= BM25Search::MIN_TERM_LEN));
    }

    #[test]
    fn test_strict_tokenize_fts_query_strips_fts_punctuation() {
        let strict = BM25Search::strict_tokenize_fts_query(
            "\"Immigration and Customs Enforcement (ICE): Operations\"",
        );
        let terms: Vec<&str> = strict.split(" OR ").collect();
        assert!(terms.contains(&"immigration"));
        assert!(terms.contains(&"customs"));
        assert!(terms.contains(&"enforcement"));
        assert!(terms.contains(&"operations"));
    }

    #[test]
    fn test_is_fts_syntax_error_detects_database_error_string() {
        let err = AppError::Database(
            "Database query failed: error returned from database: (code: 1) fts5: syntax error near \"Operations\"".to_string(),
        );
        assert!(BM25Search::is_fts_syntax_error(&err));
    }

    #[test]
    fn test_normalize_fts_query_does_not_treat_plain_punctuation_as_fts_syntax() {
        let sentence = "Immigration and Customs Enforcement (ICE): Operations expanded.";
        let normalized = BM25Search::normalize_fts_query(sentence);
        assert_ne!(normalized, sentence);
        let terms: Vec<&str> = normalized.split(" OR ").collect();
        assert!(terms.contains(&"immigration"));
        assert!(terms.contains(&"customs"));
        assert!(terms.contains(&"enforcement"));
        assert!(terms
            .iter()
            .all(|term| term.len() >= BM25Search::MIN_TERM_LEN));
    }

    #[test]
    fn test_normalize_fts_query_preserves_phrase_syntax() {
        let phrase_query = "\"blueberry anthocyanin\"";
        let normalized = BM25Search::normalize_fts_query(phrase_query);
        assert_eq!(normalized, phrase_query);
    }
}
