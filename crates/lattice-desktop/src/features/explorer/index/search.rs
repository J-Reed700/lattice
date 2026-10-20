//! Searching the open folder's index through the one hybrid search.
//!
//! The dense and `bm25` halves miss different things: dense retrieval finds
//! "where do we handle retries?" in code that only says `backoff`; `bm25`
//! finds the exact identifier a vector blurs. Neither is fused here. The
//! search is the library's orchestrator (`HybridSearchTrait`), built over
//! this folder's vectors file and full-text index by the composition root's
//! `HybridSearchFactory`, so a folder is ranked exactly as the library is:
//! its branch depth, vector floor, weights, reciprocal rank constant and
//! per-document cap (here, per file).
//!
//! SQLite is authoritative: a vector whose passage row is gone (a file
//! deleted since the last save of the vectors file) is not a hit.

use super::dto::FolderIndexStatusDto;
use super::run::StatusCell;
use crate::infrastructure::passage_index::chunking::lines as chunker;
use crate::infrastructure::passage_index::{Passage, PassageStore};
use lattice_core::application::contracts::search::SearchRequestDto;
use lattice_core::application::ports::hybrid_search::HybridSearchTrait;
use lattice_core::shared::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    store: Arc<PassageStore>,
    hybrid: Arc<dyn HybridSearchTrait>,
    /// Where the picked folder sits inside the index: `""`, or the sub-folder
    /// path when a parent's index is reused.
    offset: String,
    status: Arc<StatusCell>,
}

impl FolderSearch {
    pub fn new(
        store: Arc<PassageStore>,
        hybrid: Arc<dyn HybridSearchTrait>,
        offset: String,
        status: Arc<StatusCell>,
    ) -> Self {
        Self {
            store,
            hybrid,
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
        self.store.has_passages(&self.offset).await.unwrap_or(false)
    }

    /// The best `top_k` passages for `query`, optionally only under
    /// `path_prefix` (relative to the picked folder). Under a prefix, both
    /// halves keep to the files under it, inside USearch and inside FTS5.
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
        let scope: Option<HashSet<String>> = if prefix.is_empty() {
            None
        } else {
            Some(self.store.keys_under(&prefix).await?.into_iter().collect())
        };
        let response = self
            .hybrid
            .execute_scoped(SearchRequestDto::hybrid(query, top_k), None, scope.as_ref())
            .await?;
        let ranked: Vec<(i64, f32)> = response
            .results
            .iter()
            .filter_map(|result| Some((result.id.parse().ok()?, result.score)))
            .collect();
        let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
        let rows: HashMap<i64, Passage> = self
            .store
            .by_ids(&ids)
            .await?
            .into_iter()
            .map(|passage| (passage.id, passage))
            .collect();
        let mut hits = Vec::new();
        for (id, score) in ranked {
            let Some(row) = rows.get(&id) else {
                continue;
            };
            let Some(path) = self.picked_relative(&row.document) else {
                continue;
            };
            hits.push(FolderHit {
                path,
                start_line: u32::try_from(row.start).unwrap_or(u32::MAX),
                end_line: u32::try_from(row.end).unwrap_or(u32::MAX),
                text: chunker::body_of(&row.text).to_string(),
                score,
            });
            if hits.len() >= top_k {
                break;
            }
        }
        Ok(hits)
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
