//! SQLite Text Search Implementation
//!
//! Stub implementation of text search using SQLite FTS5.

use crate::application::ports::TextSearchPort;
use crate::features::search::dto::SearchResultPortDto;
use crate::shared::result::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
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
            INSERT INTO text_chunks VALUES ('a','other','patent'), ('b','selected','patent manual introduction');
            INSERT INTO chunks_fts SELECT id,content FROM text_chunks;
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
}

/// SQLite-based text search implementation using FTS5
///
/// Uses the chunks_fts virtual table which is automatically synced
/// with the chunks table via triggers (see schema migration).
pub struct SqliteTextSearch {
    pool: SqlitePool,
}

impl SqliteTextSearch {
    const MIN_TERM_LEN: usize = 3;
    const MAX_TERMS: usize = 16;

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
        let normalized_query = Self::normalize_fts_query(query);
        if normalized_query.is_empty() {
            return Ok(Vec::new());
        }

        if allowed_document_ids.is_some_and(HashSet::is_empty) || top_k == 0 {
            return Ok(Vec::new());
        }
        let rows_result = Self::execute_fts_query_scoped(
            &self.pool,
            &normalized_query,
            top_k,
            space_id,
            allowed_document_ids,
        )
        .await;

        let rows = match rows_result {
            Ok(rows) => rows,
            Err(error) if Self::is_fts_syntax_error(&error) => {
                let fallback_query = Self::strict_tokenize_fts_query(query);
                if fallback_query.is_empty() || fallback_query == normalized_query {
                    return Err(error.into());
                }

                warn!(
                    original_query = %query,
                    normalized_query = %normalized_query,
                    fallback_query = %fallback_query,
                    error = %error,
                    "SQLite text search FTS query failed with syntax error, retrying with strict tokenized fallback"
                );

                Self::execute_fts_query_scoped(
                    &self.pool,
                    &fallback_query,
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
        // No-op: FTS5 table is automatically synced via triggers
        // When chunks are inserted, the chunks_fts_insert trigger
        // adds content to chunks_fts automatically
        Ok(())
    }

    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        // No-op: FTS5 table is automatically synced via triggers
        Ok(())
    }

    async fn remove_document(&self, _id: &str) -> Result<()> {
        // No-op: FTS5 table is automatically synced via triggers
        // When chunks are deleted, the chunks_fts_delete trigger
        // removes content from chunks_fts automatically
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        // Clear all FTS5 data (chunk-level index)
        sqlx::query("DELETE FROM chunks_fts")
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
        query: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> std::result::Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        // Apply ALL scope restrictions before ranking/LIMIT. Filtering the top
        // global hits afterwards can hide every hit from a selected chapter.
        let mut sql = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT c.id as chunk_id, c.document_id, c.content, bm25(chunks_fts) as score FROM chunks_fts JOIN text_chunks c ON chunks_fts.chunk_id = c.id WHERE chunks_fts MATCH ",
        );
        sql.push_bind(query);
        if let Some(space) = space_id {
            sql.push(" AND EXISTS (SELECT 1 FROM document_space_memberships m WHERE m.document_id = c.document_id AND m.space_id = ");
            sql.push_bind(space).push(")");
        }
        if let Some(ids) = allowed_document_ids {
            sql.push(" AND c.document_id IN (");
            let mut sorted: Vec<_> = ids.iter().collect();
            sorted.sort();
            let mut values = sql.separated(", ");
            for id in sorted {
                values.push_bind(id);
            }
            sql.push(")");
        }
        sql.push(" ORDER BY bm25(chunks_fts), c.id LIMIT ")
            .push_bind(top_k as i64);
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

    fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
        let message = error.to_string();
        message.contains("fts5: syntax error")
            || message.contains("malformed MATCH expression")
            || message.contains("unterminated string")
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
            .map(|term| (term.clone(), Self::term_salience(&term)))
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
}

#[cfg(test)]
mod tests {
    use super::SqliteTextSearch;

    #[test]
    fn test_normalize_fts_query_tokenizes_natural_language() {
        let normalized = SqliteTextSearch::normalize_fts_query("What has ICE been doing??");
        let terms: Vec<&str> = normalized.split(" OR ").collect();
        assert!(terms
            .iter()
            .all(|term| term.len() >= SqliteTextSearch::MIN_TERM_LEN));
    }

    #[test]
    fn test_strict_tokenize_fts_query_strips_fts_punctuation() {
        let strict = SqliteTextSearch::strict_tokenize_fts_query(
            "\"Immigration and Customs Enforcement (ICE): Operations\"",
        );
        let terms: Vec<&str> = strict.split(" OR ").collect();
        assert!(terms.contains(&"immigration"));
        assert!(terms.contains(&"customs"));
        assert!(terms.contains(&"enforcement"));
        assert!(terms.contains(&"operations"));
    }

    #[test]
    fn test_normalize_fts_query_does_not_treat_plain_punctuation_as_fts_syntax() {
        let sentence = "Immigration and Customs Enforcement (ICE): Operations expanded.";
        let normalized = SqliteTextSearch::normalize_fts_query(sentence);
        assert_ne!(normalized, sentence);
        let terms: Vec<&str> = normalized.split(" OR ").collect();
        assert!(terms.contains(&"immigration"));
        assert!(terms.contains(&"customs"));
        assert!(terms.contains(&"enforcement"));
        assert!(terms
            .iter()
            .all(|term| term.len() >= SqliteTextSearch::MIN_TERM_LEN));
    }

    #[test]
    fn test_normalize_fts_query_preserves_phrase_syntax() {
        let phrase_query = "\"blueberry anthocyanin\"";
        let normalized = SqliteTextSearch::normalize_fts_query(phrase_query);
        assert_eq!(normalized, phrase_query);
    }
}
