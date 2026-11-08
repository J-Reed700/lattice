//! A passage store's full-text index behind the core `TextSearchPort`, so
//! the one hybrid search ranks a passage index's `bm25` branch exactly as it
//! ranks the library's.
//!
//! No orchestration lives here: dense and lexical fusion is the hybrid
//! search's, built over this and the vectors file by `HybridSearchFactory`.
//! The scope is the port's `allowed_document_ids`, the document keys a search
//! keeps to; a passage index has no spaces, so `space_id` is ignored.
//!
//! The port's write methods belong to the library's indexer, which the
//! hybrid search never calls; a passage index is written by its own build, so
//! they refuse with an error naming the store.

use super::store::{Passage, PassageStore};
use async_trait::async_trait;
use lattice_core::application::contracts::search::SearchResultRecord;
use lattice_core::application::ports::TextSearchPort;
use lattice_core::shared::{AppError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Terms a full-text query keeps.
const MAX_TERMS: usize = 16;

/// How a query's words match the words a store indexed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WordMatch {
    /// Words of four characters or more also match as a prefix, so `retry`
    /// finds `retry_policy`: for code, where one identifier is part of
    /// another.
    #[default]
    Prefix,
    /// A word matches only itself, as the library's full-text search matches
    /// it: for prose, where `mineral1` is not `mineral169` and `test` is not
    /// `testament`.
    Whole,
}

/// An FTS5 `MATCH` expression for free text: every word as a quoted literal
/// (so nothing in it can act as an operator), joined with `OR`. Words of four
/// characters or more also match as a prefix, so `retry` finds `retry_policy`.
pub fn match_expression(query: &str) -> Option<String> {
    match_words(query, WordMatch::Prefix)
}

/// [`match_expression`], with `words` deciding whether a word also matches
/// as a prefix.
pub fn match_words(query: &str, words: WordMatch) -> Option<String> {
    let mut seen = HashSet::new();
    let terms: Vec<String> = query
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .filter(|term| seen.insert(term.clone()))
        .take(MAX_TERMS)
        .map(|term| {
            let quoted = format!("\"{}\"", term.replace('"', "\"\""));
            if words == WordMatch::Prefix && term.chars().count() >= 4 {
                format!("{quoted}*")
            } else {
                quoted
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

/// A word as the store's tokenizer keeps it in the vocabulary: lower case,
/// Latin diacritics removed (`unicode61`'s default). A term looked up in
/// [`PassageStore::term_counts`] has to be in this form to be found.
pub fn indexed_term(word: &str) -> String {
    lattice_core::shared::text::fold_diacritics(&word.to_lowercase())
}

/// The store's FTS5 index as a text search.
pub struct PassageTextSearch {
    store: Arc<PassageStore>,
    words: WordMatch,
}

impl PassageTextSearch {
    /// Words of four characters or more also match as a prefix.
    pub fn new(store: Arc<PassageStore>) -> Self {
        Self {
            store,
            words: WordMatch::Prefix,
        }
    }

    /// The same search, matching a query's words as `words` says.
    pub fn matching(mut self, words: WordMatch) -> Self {
        self.words = words;
        self
    }

    fn read_only(&self) -> AppError {
        AppError::InvalidInput(format!(
            "The passage index in {} is written by its own build, not through text search",
            self.store.dir().display()
        ))
    }
}

#[async_trait]
impl TextSearchPort for PassageTextSearch {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResultRecord>> {
        self.search_scoped(query, top_k, None, None).await
    }

    /// Passages best by `bm25` first. A record's document is its passage's
    /// document key; its score is `-bm25` scaled so the best is 1, as the
    /// library's text search reports it.
    async fn search_scoped(
        &self,
        query: &str,
        top_k: usize,
        _space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultRecord>> {
        if top_k == 0 || allowed_document_ids.is_some_and(HashSet::is_empty) {
            return Ok(Vec::new());
        }
        let Some(expression) = match_words(query, self.words) else {
            return Ok(Vec::new());
        };
        let ranked = self
            .store
            .full_text(&expression, allowed_document_ids, top_k)
            .await?;
        let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
        let mut rows: HashMap<i64, Passage> = self
            .store
            .by_ids(&ids)
            .await?
            .into_iter()
            .map(|passage| (passage.id, passage))
            .collect();
        let best = ranked
            .iter()
            .map(|(_, bm25)| (-bm25).max(0.0))
            .fold(0.0, f64::max);
        Ok(ranked
            .into_iter()
            .filter_map(|(id, bm25)| {
                let passage = rows.remove(&id)?;
                let raw = (-bm25).max(0.0);
                let score = if best > 0.0 { raw / best } else { 0.0 };
                Some(SearchResultRecord {
                    doc_id: passage.document,
                    chunk_id: id.to_string(),
                    score: score as f32,
                    content: passage.text,
                })
            })
            .collect())
    }

    async fn index_document(&self, _id: &str, _content: &str) -> Result<()> {
        Err(self.read_only())
    }

    async fn index_batch(&self, _documents: &[(&str, &str)]) -> Result<()> {
        Err(self.read_only())
    }

    async fn remove_document(&self, _id: &str) -> Result<()> {
        Err(self.read_only())
    }

    async fn clear(&self) -> Result<()> {
        Err(self.read_only())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.store.counts("").await?.passages_total as usize)
    }
}
