//! Searching the open folder's index: dense and full-text, fused.
//!
//! The two halves miss different things. Dense retrieval finds "where do we
//! handle retries?" in code that only says `backoff`; `bm25` finds the exact
//! identifier a vector blurs. Reciprocal rank fusion (k = 60) merges them on
//! rank alone, so neither score scale has to be trusted against the other.
//!
//! SQLite is authoritative: a vector whose chunk row is gone (a file deleted
//! since the last save of the vectors file) is not a hit.

use super::chunker;
use super::dto::FolderIndexStatusDto;
use super::run::StatusCell;
use super::store::FolderStore;
use crate::application::ports::vector_search_port::VectorSearchPort;
use crate::application::ports::EmbeddingPort;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::shared::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::warn;

/// Candidates each half contributes before fusion.
pub const DENSE_CANDIDATES: usize = 40;
pub const LEXICAL_CANDIDATES: usize = 40;
/// The usual constant: it keeps one list's first place from drowning out a
/// passage both lists rank well.
pub const RRF_K: f32 = 60.0;
/// Terms a full-text query keeps.
const MAX_TERMS: usize = 16;

/// One passage found in the folder. `path` is relative to the folder the user
/// picked, the same form line references use.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderHit {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    /// The passage's lines, without the line-reference heading.
    pub text: String,
    pub score: f32,
}

/// Reciprocal rank fusion of ranked id lists, best first. Ties go to the
/// lower id so the order is stable.
pub fn fuse(lists: &[&[i64]], k: f32) -> Vec<(i64, f32)> {
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for list in lists {
        for (rank, id) in list.iter().enumerate() {
            *scores.entry(*id).or_default() += 1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(i64, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    fused
}

/// An FTS5 `MATCH` expression for free text: every word as a quoted literal
/// (so nothing in it can act as an operator), joined with `OR`. Words of four
/// characters or more also match as a prefix, so `retry` finds `retry_policy`.
pub fn match_expression(query: &str) -> Option<String> {
    let mut seen = HashSet::new();
    let terms: Vec<String> = query
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|term| term.chars().count() >= 2)
        .map(str::to_lowercase)
        .filter(|term| seen.insert(term.clone()))
        .take(MAX_TERMS)
        .map(|term| {
            let quoted = format!("\"{}\"", term.replace('"', "\"\""));
            if term.chars().count() >= 4 {
                format!("{quoted}*")
            } else {
                quoted
            }
        })
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

/// `a` and `b` as one relative path; either may be empty.
pub fn join_relative(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}/{b}"),
    }
}

/// A path the model or the user gave, in the index's form: no leading `./`
/// or `/`, no trailing `/`. `"/"` and `"."` mean the whole folder.
fn tidy_prefix(path: &str) -> String {
    let mut path = path.trim();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    match path.trim_matches('/') {
        "." => String::new(),
        tidy => tidy.to_string(),
    }
}

pub struct FolderSearch {
    store: Arc<FolderStore>,
    vectors: Arc<USearchVectorIndex>,
    embedder: Arc<dyn EmbeddingPort>,
    /// Where the picked folder sits inside the index: `""`, or the sub-folder
    /// path when a parent's index is reused.
    offset: String,
    status: Arc<StatusCell>,
}

impl FolderSearch {
    pub fn new(
        store: Arc<FolderStore>,
        vectors: Arc<USearchVectorIndex>,
        embedder: Arc<dyn EmbeddingPort>,
        offset: String,
        status: Arc<StatusCell>,
    ) -> Self {
        Self {
            store,
            vectors,
            embedder,
            offset,
            status,
        }
    }

    /// How far along the index is, for "searched what is indexed so far".
    pub fn status(&self) -> FolderIndexStatusDto {
        self.status.get()
    }

    /// Whether the folder has anything to search yet.
    pub async fn has_chunks(&self) -> bool {
        self.store.has_chunks(&self.offset).await.unwrap_or(false)
    }

    /// The best `top_k` passages for `query`, optionally only under
    /// `path_prefix` (relative to the picked folder).
    pub async fn search(
        &self,
        query: &str,
        path_prefix: Option<&str>,
        top_k: usize,
    ) -> Result<Vec<FolderHit>> {
        if top_k == 0 || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let prefix = join_relative(&self.offset, &tidy_prefix(path_prefix.unwrap_or("")));
        // Either half can fail on its own (no embedder warmed up, an odd
        // query); the other still answers.
        let dense = self.dense(query, &prefix).await.unwrap_or_else(|error| {
            warn!(%error, "Folder index: dense search failed; using full text only");
            Vec::new()
        });
        let lexical = match match_expression(query) {
            Some(expression) => self
                .store
                .full_text(&expression, &prefix, LEXICAL_CANDIDATES)
                .await
                .unwrap_or_else(|error| {
                    warn!(%error, "Folder index: full-text search failed");
                    Vec::new()
                }),
            None => Vec::new(),
        };
        let fused = fuse(&[&dense, &lexical], RRF_K);
        let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        let rows: HashMap<i64, _> = self
            .store
            .chunks_by_ids(&ids)
            .await?
            .into_iter()
            .map(|row| (row.id, row))
            .collect();
        let mut hits = Vec::new();
        for (id, score) in fused {
            let Some(row) = rows.get(&id) else {
                continue;
            };
            let Some(path) = self.picked_relative(&row.path) else {
                continue;
            };
            hits.push(FolderHit {
                path,
                start_line: row.start_line,
                end_line: row.end_line,
                text: chunker::body_of(&row.text).to_string(),
                score,
            });
            if hits.len() >= top_k {
                break;
            }
        }
        Ok(hits)
    }

    /// Chunk ids nearest the query, confined to `prefix` inside USearch.
    async fn dense(&self, query: &str, prefix: &str) -> Result<Vec<i64>> {
        if self.vectors.count() == 0 {
            return Ok(Vec::new());
        }
        let embedding = self.embedder.embed_query(query).await?;
        let scope: Option<HashSet<String>> = if prefix.is_empty() {
            None
        } else {
            Some(self.store.paths_under(prefix).await?.into_iter().collect())
        };
        let results =
            self.vectors
                .search_scoped(&embedding, DENSE_CANDIDATES, 0.0, scope.as_ref())?;
        Ok(results
            .into_iter()
            .filter_map(|result| result.chunk_id.parse().ok())
            .collect())
    }

    /// An index path in the picked folder's terms, or `None` outside it.
    fn picked_relative(&self, path: &str) -> Option<String> {
        if self.offset.is_empty() {
            return Some(path.to_string());
        }
        path.strip_prefix(&self.offset)?
            .strip_prefix('/')
            .map(str::to_string)
    }
}
