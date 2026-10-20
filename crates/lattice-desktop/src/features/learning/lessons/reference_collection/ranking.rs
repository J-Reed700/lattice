//! Ranking a collection's passages: the course index's hybrid search, or a
//! store's full text, then each hit widened to paragraph edges.
//!
//! A course whose passages are embedded is ranked by the one hybrid search
//! (`HybridSearchTrait`): each query's dense and full-text branches, all of
//! them fused once, at the library's weights and with its per-source cap. A
//! course without an embedding model, and a collection built in memory, are
//! ranked by FTS5's `bm25` over all the queries' words at once, which needs
//! no fusion: one list. Either way a query's words match whole, as the
//! library's text search matches them, so a source never enters a claim's
//! evidence because one of its words merely begins with one of the claim's. Either way a hit is a stored passage, a byte span of
//! a source's text, so it maps onto the source with no relocation.

use super::*;
use crate::features::learning::references::course_index::write_version;
use crate::infrastructure::passage_index::text_search::{match_words, WordMatch};
use lattice_core::shared::constants::{
    DEFAULT_KEYWORD_FUSION_WEIGHT, DEFAULT_VECTOR_FUSION_WEIGHT,
};
use std::collections::HashMap;

/// One passage as a search ranked it: its id, its score, and its best rank
/// and score in a dense and in a full-text list.
type Scored = (i64, f64, Option<(usize, f32)>, Option<(usize, f32)>);

/// One ranked span of a source, before widening.
struct Hit {
    source: usize,
    start: usize,
    end: usize,
    score: f64,
    dense: Option<(usize, f32)>,
    lexical: Option<(usize, f32)>,
}

impl<'a> ReferenceCollection<'a> {
    /// The store this collection reads: the course index's, or its own in
    /// memory, built from its sources the first time it is needed.
    pub(super) async fn store(&self) -> Result<Arc<PassageStore>> {
        match &self.index {
            Index::Course { view, .. } => Ok(Arc::clone(&view.store)),
            Index::Memory(store) => store
                .get_or_try_init(|| async {
                    let store = PassageStore::memory().await?;
                    for source in &self.sources {
                        write_version(&store, &source.id, "", &source.excerpt).await?;
                    }
                    Ok::<_, AppError>(Arc::new(store))
                })
                .await
                .map(Arc::clone),
        }
    }

    /// The best passages for `queries` together, at most `depth`, over the
    /// sources `scope` keeps: each hit widened to paragraph edges, a source's
    /// bytes shown at most once. Searches deeper while overlaps keep the list
    /// short of `depth` and more passages exist.
    pub(in crate::features::learning) async fn rank(
        &self,
        queries: &[&str],
        scope: &Scope,
        depth: usize,
    ) -> Result<Vec<Ranked>> {
        let queries: Vec<&str> = queries
            .iter()
            .map(|query| query.trim())
            .filter(|query| !query.is_empty())
            .collect();
        if queries.is_empty() || depth == 0 {
            return Ok(Vec::new());
        }
        let allowed: Option<HashSet<String>> = match scope {
            Scope::All => None,
            Scope::Keys(keys) => Some(
                keys.iter()
                    .filter(|key| self.sources.iter().any(|source| &source.id == *key))
                    .cloned()
                    .collect(),
            ),
        };
        if allowed.as_ref().is_some_and(HashSet::is_empty) {
            return Ok(Vec::new());
        }
        let mut fetch = depth;
        loop {
            let (hits, found) = self.hits(&queries, allowed.as_ref(), fetch).await?;
            let ranked = self.widened(hits, depth)?;
            if ranked.len() >= depth || found < fetch {
                // Which branch found what, and the fused scores at either
                // end: diagnostics for the log, never a cut.
                tracing::debug!(
                    mode = self.mode().label(),
                    queries = queries.len(),
                    passages = ranked.len(),
                    by_meaning = ranked.iter().filter(|hit| hit.dense.is_some()).count(),
                    by_words = ranked.iter().filter(|hit| hit.lexical.is_some()).count(),
                    first = ?ranked.first().map(|hit| hit.passage.score),
                    last = ?ranked.last().map(|hit| hit.passage.score),
                    "Ranked references"
                );
                return Ok(ranked);
            }
            fetch = fetch.saturating_mul(2);
        }
    }

    /// At most `fetch` hits in ranked order, and how many the search found
    /// before any was dropped for not mapping onto a source.
    async fn hits(
        &self,
        queries: &[&str],
        allowed: Option<&HashSet<String>>,
        fetch: usize,
    ) -> Result<(Vec<Hit>, usize)> {
        let store = self.store().await?;
        let hybrid = match &self.index {
            Index::Course { view, .. } => view.hybrid.clone(),
            Index::Memory(_) => None,
        };
        let ranked: Vec<Scored> = match hybrid {
            Some(hybrid) => {
                let mut branches = Vec::new();
                let mut answered = 0;
                for query in queries {
                    let found = hybrid
                        .search_branches(
                            query,
                            None,
                            allowed,
                            fetch,
                            DEFAULT_VECTOR_FUSION_WEIGHT,
                            DEFAULT_KEYWORD_FUSION_WEIGHT,
                        )
                        .await;
                    answered += found.answered;
                    branches.extend(found.branches);
                }
                if answered == 0 {
                    return Err(AppError::ServiceNotAvailable(
                        "The course's reference search failed by meaning and by words".into(),
                    ));
                }
                hybrid
                    .fuse(branches, fetch)
                    .into_iter()
                    .filter_map(|result| {
                        Some((
                            result.id.parse().ok()?,
                            f64::from(result.score),
                            result.vector_rank.zip(result.vector_score),
                            result.bm25_rank.zip(result.bm25_score),
                        ))
                    })
                    .collect()
            }
            None => {
                let Some(expression) = match_words(&queries.join(" "), WordMatch::Whole) else {
                    return Ok((Vec::new(), 0));
                };
                store
                    .full_text(&expression, allowed, fetch)
                    .await?
                    .into_iter()
                    .enumerate()
                    .map(|(rank, (id, bm25))| {
                        let relevance = (-bm25).max(0.0);
                        (id, relevance, None, Some((rank, relevance as f32)))
                    })
                    .collect()
            }
        };
        let found = ranked.len();
        let ids: Vec<i64> = ranked.iter().map(|(id, ..)| *id).collect();
        let rows: HashMap<i64, crate::infrastructure::passage_index::Passage> = store
            .by_ids(&ids)
            .await?
            .into_iter()
            .map(|passage| (passage.id, passage))
            .collect();
        let sources: HashMap<&str, usize> = self
            .sources
            .iter()
            .enumerate()
            .map(|(index, source)| (source.id.as_str(), index))
            .collect();
        let hits = ranked
            .into_iter()
            .filter_map(|(id, score, dense, lexical)| {
                let row = rows.get(&id)?;
                let source = *sources.get(row.document.as_str())?;
                // A passage is the source's own bytes; one that is not (a
                // version the index holds differently) is no evidence.
                let text = self.sources.get(source)?.excerpt.get(row.start..row.end)?;
                (text == row.text).then_some(Hit {
                    source,
                    start: row.start,
                    end: row.end,
                    score,
                    dense,
                    lexical,
                })
            })
            .collect();
        Ok((hits, found))
    }

    /// Widens each hit to adjacent paragraph edges, each source byte at most
    /// once, up to `limit`.
    fn widened(&self, hits: Vec<Hit>, limit: usize) -> Result<Vec<Ranked>> {
        let retrieval_kind = self.mode().label();
        let mut selected: Vec<Ranked> = Vec::new();
        for hit in hits {
            let source = self
                .sources
                .get(hit.source)
                .ok_or_else(|| invalid("Reference index contains an unknown source."))?;
            // Expand to adjacent paragraph boundaries where practical. Evidence
            // is always an exact substring, including significant whitespace.
            let mut start = hit.start.saturating_sub(WIDEN_BYTES);
            while !source.excerpt.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (hit.end + WIDEN_BYTES).min(source.excerpt.len());
            while !source.excerpt.is_char_boundary(end) {
                end -= 1;
            }
            if let Some(i) = source.excerpt[start..hit.start].find('\n') {
                start += i + 1;
            }
            if let Some(i) = source.excerpt[hit.end..end].rfind('\n') {
                end = hit.end + i;
            }
            if selected.iter().any(|ranked| {
                ranked.passage.source_id == source.id
                    && ranked.passage.start_byte < end
                    && start < ranked.passage.end_byte
            }) {
                continue;
            }
            selected.push(Ranked {
                passage: ReferencePassage {
                    source_id: source.id.clone(),
                    text: source.excerpt[start..end].to_owned(),
                    start_byte: start,
                    end_byte: end,
                    retrieval_kind: retrieval_kind.into(),
                    score: hit.score,
                },
                dense: hit.dense,
                lexical: hit.lexical,
            });
            if selected.len() >= limit {
                break;
            }
        }
        Ok(selected)
    }
}
