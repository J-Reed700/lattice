//! Nearest-neighbour lookup: the HNSW graph walk, exact scoped scans and candidate rescoring.
use super::{KeyState, USearchVectorIndex};
use crate::features::search::engine::vector_ops::cosine_similarity_simd;
use crate::shared::error::{AppError, Result};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashSet};
#[cfg(test)]
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy)]
struct ScopedCandidate {
    key: u64,
    score: f32,
}

// Reverse score ordering so the heap head is the worst retained candidate.
impl PartialEq for ScopedCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.score.to_bits() == other.score.to_bits()
    }
}
impl Eq for ScopedCandidate {}
impl PartialOrd for ScopedCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScopedCandidate {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.key.cmp(&other.key))
    }
}

impl USearchVectorIndex {
    /// The `count` stored vectors nearest to `query`, nearest first, as
    /// parallel key and distance lists.
    ///
    /// A small index is scanned exactly; see [`EXACT_SEARCH_MAX_VECTORS`]. The
    /// scan has no predicate, so a scoped query ranks every vector and keeps the
    /// in-scope ones. Past that size the graph answers, and the scope runs as a
    /// predicate inside the traversal: the filter costs a hash lookup per
    /// visited node instead of a discarded candidate per hit. The predicate
    /// sees only the USearch key, and it must not panic, because USearch calls
    /// it back through a plain `extern "C"` trampoline that unwinding would
    /// cross.
    pub(super) fn nearest(
        &self,
        state: &KeyState,
        query: &[f32],
        count: usize,
        scope: Option<&HashSet<String>>,
        full_query: &[f32],
    ) -> Result<(Vec<u64>, Vec<f32>, bool)> {
        let in_scope = |key: u64, scope: &HashSet<String>| {
            state
                .key_to_id
                .get(&key)
                .map(|id| {
                    let doc_id = state
                        .metadata
                        .get(id)
                        .map_or(id.as_str(), |m| m.document_id.as_str());
                    scope.contains(doc_id)
                })
                .unwrap_or(false)
        };
        let failed = |e| AppError::InternalError(format!("USearch search failed: {}", e));

        let index_size = self.index.size();
        if let Some(scope) = scope {
            let eligible = scope
                .iter()
                .filter_map(|document_id| state.document_to_keys.get(document_id))
                .map(HashSet::len)
                .sum::<usize>();
            if eligible <= self.exact_search_max {
                let (keys, distances) =
                    self.exact_scoped(state, query, full_query, count.min(eligible), scope)?;
                return Ok((keys, distances, true));
            }
        }
        if index_size <= self.exact_search_max {
            let Some(scope) = scope else {
                let matches = self.index.exact_search(query, count).map_err(failed)?;
                return Ok((matches.keys, matches.distances, false));
            };
            let (keys, distances) = self.exact_scoped(state, query, full_query, count, scope)?;
            return Ok((keys, distances, true));
        }

        let matches = match scope {
            None => self.index.search(query, count),
            Some(scope) => self
                .index
                .filtered_search(query, count, |key| in_scope(key, scope)),
        }
        .map_err(failed)?;
        Ok((matches.keys, matches.distances, false))
    }

    /// Exact-score only vectors in a small scope and retain at most `count`.
    fn exact_scoped(
        &self,
        state: &KeyState,
        query: &[f32],
        full_query: &[f32],
        count: usize,
        scope: &HashSet<String>,
    ) -> Result<(Vec<u64>, Vec<f32>)> {
        let failed = |e| AppError::InternalError(format!("USearch vector lookup failed: {e}"));
        if count == 0 {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut best = BinaryHeap::with_capacity(count);
        let mut projected_vector = vec![0.0f32; query.len()];
        let mut full_vector = vec![0.0f32; full_query.len()];
        let mut full_vector_bytes = vec![0u8; full_query.len().saturating_mul(4)];
        for document_id in scope {
            let Some(keys) = state.document_to_keys.get(document_id) else {
                continue;
            };
            for &key in keys {
                #[cfg(test)]
                self.exact_scope_scored.fetch_add(1, Ordering::Relaxed);
                let has_full_vector = self.rescore.as_ref().is_some_and(|full_vectors| {
                    full_vectors.get_into(key, &mut full_vector, &mut full_vector_bytes)
                });
                let similarity = if has_full_vector {
                    cosine_similarity_simd(full_query, &full_vector)
                } else {
                    let found = self.index.get(key, &mut projected_vector).map_err(failed)?;
                    if found == 0 {
                        continue;
                    }
                    cosine_similarity_simd(query, &projected_vector)
                };
                if !similarity.is_finite() {
                    continue;
                }
                let candidate = ScopedCandidate {
                    key,
                    score: similarity,
                };
                if best.len() < count {
                    best.push(candidate);
                } else if best.peek().is_some_and(|worst| candidate < *worst) {
                    best.pop();
                    best.push(candidate);
                }
            }
        }
        let mut best = best.into_vec();
        best.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.key.cmp(&right.key))
        });
        Ok(best
            .into_iter()
            .map(|candidate| (candidate.key, 1.0 - candidate.score))
            .unzip())
    }

    /// Candidates to ask USearch for when `top_k` results are wanted.
    ///
    /// With compression on, the ANN ordering is computed over truncated and
    /// usually quantized vectors, so the true top-k is a *subset* of a wider
    /// window. Over-fetching by `rescore_factor` and re-ranking exactly is what
    /// buys back the accuracy the smaller vectors gave up.
    pub(super) fn candidate_window(&self, top_k: usize, index_size: usize) -> usize {
        if !self.compression.is_active() {
            return top_k.min(index_size);
        }
        top_k
            .saturating_mul(self.compression.rescore_factor())
            .min(index_size)
    }

    /// Score actually reported for a candidate.
    ///
    /// With compression off this is just USearch's cosine distance inverted.
    /// With compression on, USearch's distance was measured in the lossy space
    /// and is only a ranking hint, so the authoritative score is the exact
    /// cosine between the full-precision query and the full-precision stored
    /// vector. A candidate whose full vector is missing — an index rebuilt by
    /// an older build, a truncated side store — falls back to the approximate
    /// score rather than disappearing from the result set.
    pub(super) fn candidate_similarity(
        &self,
        key: u64,
        query: &[f32],
        approximate_distance: f32,
    ) -> f32 {
        let approximate = 1.0 - approximate_distance;
        let Some(store) = self.rescore.as_ref() else {
            return approximate;
        };
        match store.get(key) {
            // The rescore runs `top_k * rescore_factor` times per query over
            // full-width vectors, so it gets the SIMD kernel rather than the
            // scalar one. Both agree to within f32 reassociation noise; see
            // the property test in `vector_ops`.
            Some(full) => cosine_similarity_simd(query, &full),
            None => {
                tracing::debug!(key, "no full vector to rescore against; using ANN score");
                approximate
            }
        }
    }

    /// Order results by descending score. Only meaningful after rescoring —
    /// USearch already returns its own matches sorted.
    pub(super) fn sort_by_score_desc<T>(results: &mut [T], score: impl Fn(&T) -> f32) {
        results.sort_by(|a, b| {
            score(b)
                .partial_cmp(&score(a))
                .unwrap_or(CmpOrdering::Equal)
        });
    }
}
