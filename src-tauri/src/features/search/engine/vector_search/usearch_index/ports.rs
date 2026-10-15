//! The index as a `VectorSearchPort` and a `SearchServiceTrait`.
use super::{USearchVectorIndex, MAX_SCOPED_CANDIDATES};
use crate::features::search::dto::SearchResultPortDto;
use crate::features::search::engine::service::SearchResult;
use crate::features::search::SearchServiceTrait;
use async_trait::async_trait;
use lattice_core::application::ports::vector_search_port::VectorIndexEntry;
use lattice_core::application::ports::VectorSearchPort;
use lattice_core::shared::error::{AppError, Result};
use std::collections::HashSet;
use std::sync::atomic::Ordering;

/// How far a compressed vector's approximate rank may misrepresent its true
/// cosine. Quantization perturbs each component slightly, so a candidate can
/// rescore a little above or below its neighbours; the scoped search will not
/// abandon a widening pass over a gap smaller than this.
const RESCORE_ORDER_MARGIN: f32 = 0.05;

impl VectorSearchPort for USearchVectorIndex {
    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResultPortDto>> {
        self.search_scoped(query_embedding, top_k, threshold, None)
    }

    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        if query_embedding.len() != self.dimension {
            return Err(AppError::InvalidInput(format!(
                "Query embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                query_embedding.len()
            )));
        }

        if top_k == 0 {
            return Err(AppError::InvalidInput(
                "top_k must be greater than 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&threshold) {
            return Err(AppError::InvalidInput(format!(
                "Threshold must be in [0.0, 1.0], got {}",
                threshold
            )));
        }

        let state = self.state.read();
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }
        if matches!(allowed_document_ids, Some(scope) if scope.is_empty()) {
            return Ok(Vec::new());
        }

        let index_size = self.index.size();
        let eligible_count = allowed_document_ids.map_or(index_size, |scope| {
            scope
                .iter()
                .filter_map(|document_id| state.document_to_keys.get(document_id))
                .map(HashSet::len)
                .sum::<usize>()
        });
        let target_k = top_k.min(eligible_count);
        if target_k == 0 {
            return Ok(Vec::new());
        }
        let rescoring = self.compression.is_active();
        let mut candidate_k = self
            .candidate_window(target_k, index_size)
            .min(eligible_count);
        if allowed_document_ids.is_some() {
            // A predicate keeps out-of-scope neighbours out of the result set
            // but cannot conjure in-scope ones the traversal never reached, so
            // a scoped search still asks for a wider window than an open one.
            let widened_start = target_k.saturating_mul(4).max(32).min(eligible_count);
            candidate_k = candidate_k.max(widened_start);
        }
        let maximum_candidate_k = if allowed_document_ids.is_some() {
            target_k.max(MAX_SCOPED_CANDIDATES).min(eligible_count)
        } else {
            candidate_k
        };
        candidate_k = candidate_k.min(maximum_candidate_k);

        // The stored vectors are truncated/quantized, so the query has to be
        // projected into the same space before USearch can compare them. The
        // untouched `query_embedding` stays the reference for rescoring.
        let projected_query = self.compression.project(query_embedding)?;

        // At most two bounded filtered traversals. Small scopes have already
        // taken the exact path; large scopes do not turn an underfill into a
        // full-index scan.
        let mut passes = 0;
        loop {
            passes += 1;
            let (keys, distances, exact_scope) = self.nearest(
                &state,
                projected_query.as_ref(),
                candidate_k,
                allowed_document_ids,
                query_embedding,
            )?;

            let mut results = Vec::new();
            // The best similarity any candidate reached. A second pass is
            // worth doing when the scope is what emptied the window, and
            // pointless when the threshold is: USearch returns neighbours
            // nearest first, so a wider window can only add vectors that score
            // lower than the ones already rejected.
            let mut best_seen: Option<f32> = None;
            for (&key, &distance) in keys.iter().zip(distances.iter()) {
                let Some(id) = state.key_to_id.get(&key) else {
                    continue;
                };
                let (doc_id, chunk_id) = if let Some(m) = state.metadata.get(id) {
                    (m.document_id.as_str(), m.chunk_id.as_str())
                } else {
                    (id.as_str(), id.as_str())
                };

                // USearch cosine distance = 1.0 - cosine_similarity; rescoring
                // replaces it with the exact full-precision cosine. Threshold
                // is applied to the score we report, not to the lossy one.
                let similarity = if exact_scope {
                    1.0 - distance
                } else {
                    self.candidate_similarity(key, query_embedding, distance)
                };
                best_seen = Some(best_seen.map_or(similarity, |best: f32| best.max(similarity)));
                if similarity < threshold {
                    continue;
                }

                results.push(SearchResultPortDto {
                    doc_id: doc_id.to_string(),
                    chunk_id: chunk_id.to_string(),
                    score: similarity,
                    content: String::new(),
                });
                // Rescoring reorders the window, so the first `top_k` matches
                // USearch handed back are not necessarily the best `top_k`.
                if !rescoring && results.len() >= target_k {
                    break;
                }
            }

            if rescoring {
                Self::sort_by_score_desc(&mut results, |r| r.score);
            }

            if results.len() >= target_k || candidate_k >= eligible_count || passes >= 2 {
                if results.len() > top_k {
                    results.truncate(top_k);
                }
                return Ok(results);
            }

            // When rescoring is active the approximate order USearch returns
            // is not exactly the order of true cosine, so a vector further out
            // can still rescore slightly higher. Give that reordering room:
            // only stop when the nearest candidate misses by more than
            // quantization could explain.
            if let Some(best) = best_seen {
                let margin = if rescoring { RESCORE_ORDER_MARGIN } else { 0.0 };
                if best + margin < threshold {
                    tracing::debug!(
                        candidate_k,
                        best_similarity = best,
                        threshold,
                        "USearch search stopping: nearest candidate is below threshold"
                    );
                    return Ok(results);
                }
            }

            tracing::debug!(
                candidate_k,
                target_k,
                "USearch filtered search widening its bounded candidate window"
            );
            candidate_k = candidate_k
                .saturating_mul(2)
                .max(candidate_k.saturating_add(1))
                .min(maximum_candidate_k);
        }
    }

    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()> {
        self.add_internal(id, embedding, None, None, None)?;
        self.note_mutation()
    }

    fn add_embedding_with_content(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        chunk_id: String,
        document_id: String,
    ) -> Result<()> {
        self.add_internal(
            id,
            embedding,
            Some(content),
            Some(chunk_id),
            Some(document_id),
        )?;
        self.note_mutation()
    }

    fn publish_embeddings(&self, entries: Vec<VectorIndexEntry>) -> Result<()> {
        // Validate before changing the runtime index. IDs come from committed
        // SQLite rows; an existing ID is the same immutable chunk on a retry.
        if entries
            .iter()
            .any(|entry| entry.embedding.len() != self.dimension)
        {
            return Err(AppError::InvalidInput(
                "Embedding dimension mismatch during publication".into(),
            ));
        }
        for entry in entries {
            self.insert_internal(
                entry.id,
                entry.embedding,
                Some(entry.content),
                Some(entry.chunk_id),
                Some(entry.document_id),
                true,
            )?;
        }
        self.note_mutation()
    }

    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.remove_embeddings(&[id.to_owned()])
    }

    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        let mut state = self.state.write();
        let mut changed = false;
        for id in ids {
            let Some(&key) = state.id_to_key.get(id) else {
                continue;
            };
            self.index.remove(key).map_err(|e| {
                AppError::InternalError(format!("Failed to remove embedding from USearch: {}", e))
            })?;
            if let Some(store) = self.rescore.as_ref() {
                store.remove(key)?;
            }
            state.id_to_key.remove(id);
            state.key_to_id.remove(&key);
            let document_id = state
                .metadata
                .remove(id)
                .map_or_else(|| id.clone(), |meta| meta.document_id);
            if let Some(keys) = state.document_to_keys.get_mut(&document_id) {
                keys.remove(&key);
                if keys.is_empty() {
                    state.document_to_keys.remove(&document_id);
                }
            }
            changed = true;
        }
        drop(state);
        if changed {
            self.note_mutation()?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        let mut state = self.state.write();
        self.index.reset().map_err(|e| {
            AppError::InternalError(format!("Failed to reset USearch index: {}", e))
        })?;
        // Keys restart at 1 after a reset, so stale slots would otherwise be
        // reused with another chunk's vector still in them.
        if let Some(store) = self.rescore.as_ref() {
            store.clear()?;
        }
        state.id_to_key.clear();
        state.key_to_id.clear();
        state.metadata.clear();
        state.document_to_keys.clear();
        self.next_key.store(1, Ordering::SeqCst);
        drop(state);
        // No save here: `clear` is only ever the first half of a rebuild, and
        // writing an empty index between the two halves would hand a crash a
        // file claiming the library has no vectors.
        self.dirty.lock().mark();
        Ok(())
    }

    fn count(&self) -> usize {
        self.state.read().id_to_key.len()
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[async_trait]
impl SearchServiceTrait for USearchVectorIndex {
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>> {
        let state = self.state.read();
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }

        let start = std::time::Instant::now();

        // Over-fetch when the stored vectors are lossy; the exact top-k is a
        // subset of the wider window, not a prefix of it.
        let candidate_k = self.candidate_window(top_k, self.index.size());
        let projected_query = self.compression.project(query_embedding)?;

        let (keys, distances, _) = self.nearest(
            &state,
            projected_query.as_ref(),
            candidate_k,
            None,
            query_embedding,
        )?;

        let mut results: Vec<SearchResult> = keys
            .iter()
            .zip(distances.iter())
            .enumerate()
            .filter_map(|(idx, (&key, &distance))| {
                let similarity = self.candidate_similarity(key, query_embedding, distance);
                state.key_to_id.get(&key).map(|id| {
                    let (resolved_chunk_id, resolved_document_id) =
                        if let Some(metadata) = state.metadata.get(id) {
                            (
                                metadata.chunk_id.clone(),
                                Some(metadata.document_id.clone()),
                            )
                        } else if let Some(stripped_chunk_id) = id.strip_prefix("emb_") {
                            // Backward compatibility: legacy indexes may only store embedding IDs.
                            // The DB convention is emb_<chunk_id>, so recover the chunk ID here.
                            (stripped_chunk_id.to_string(), None)
                        } else {
                            (id.clone(), None)
                        };

                    SearchResult {
                        id: resolved_chunk_id,
                        score: similarity,
                        index: idx,
                        filename: None,
                        mime_type: None,
                        size_bytes: None,
                        created_at: None,
                        content: None,
                        file_id: resolved_document_id.clone(),
                        file_path: None,
                        file_name: None,
                        file_extension: None,
                        file_category: None,
                        is_indexed: None,
                        document_id: resolved_document_id,
                        snippet: None,
                        chunk_index: None,
                        updated_at: None,
                    }
                })
            })
            .collect();

        if self.compression.is_active() {
            Self::sort_by_score_desc(&mut results, |r| r.score);
            results.truncate(top_k);
            // `index` is the caller-visible rank, so it has to follow the
            // rescored order rather than USearch's approximate one.
            for (rank, result) in results.iter_mut().enumerate() {
                result.index = rank;
            }
        }

        let duration = start.elapsed();
        tracing::debug!(
            duration_ms = duration.as_millis(),
            results_found = results.len(),
            index_size = self.index.size(),
            "USearch vector search completed"
        );

        Ok(results)
    }

    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        // Metadata enrichment happens at higher layers (SearchEnrichmentService)
        SearchServiceTrait::search(self, query_embedding, top_k)
    }

    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>> {
        let results = SearchServiceTrait::search(self, query_embedding, top_k)?;
        Ok(results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .collect())
    }

    fn batch_search(
        &self,
        query_embeddings: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>> {
        query_embeddings
            .iter()
            .map(|query| SearchServiceTrait::search(self, query, top_k))
            .collect()
    }
}
