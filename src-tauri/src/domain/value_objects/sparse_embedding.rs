//! Learned sparse embeddings (BGE-M3 style "sparse head" term weights).
//!
//! A dense vector answers "what does this passage mean"; a learned sparse
//! vector answers "which vocabulary terms does this passage activate, and how
//! strongly". It is lexical like BM25 — the axes are token ids — but the
//! weights are produced by the model, so a passage can activate a term it never
//! literally contains and can down-weight a term it repeats mechanically.
//!
//! The representation is a coordinate list: `indices[i]` is a tokenizer term id
//! and `weights[i]` is its weight. Indices are unique and sorted ascending so
//! two embeddings can be compared, persisted, and merged deterministically.

use serde::{Deserialize, Serialize};

/// A sparse term-weight vector over a tokenizer's vocabulary.
///
/// Invariants (upheld by every constructor in this module):
/// - `indices.len() == weights.len()`
/// - `indices` is strictly ascending (therefore unique)
/// - every weight is finite and strictly positive
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SparseEmbedding {
    /// Tokenizer term ids, strictly ascending.
    pub indices: Vec<u32>,
    /// Weight for the term id at the same position.
    pub weights: Vec<f32>,
}

impl SparseEmbedding {
    /// An embedding with no active terms. Scores zero against everything.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from already-aggregated `(term_id, weight)` pairs.
    ///
    /// Duplicate term ids keep their maximum weight, non-positive and
    /// non-finite weights are dropped, and the result is sorted by term id.
    /// This is the same aggregation the token path uses, so callers that
    /// already hold term weights do not need to re-derive it.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (u32, f32)>) -> Self {
        let mut terms: Vec<(u32, f32)> = Vec::new();
        for (term_id, weight) in pairs {
            if !weight.is_finite() || weight <= 0.0 {
                continue;
            }
            match terms.binary_search_by_key(&term_id, |(id, _)| *id) {
                Ok(position) => {
                    if let Some(slot) = terms.get_mut(position) {
                        if weight > slot.1 {
                            slot.1 = weight;
                        }
                    }
                }
                Err(position) => terms.insert(position, (term_id, weight)),
            }
        }
        let mut indices = Vec::with_capacity(terms.len());
        let mut weights = Vec::with_capacity(terms.len());
        for (term_id, weight) in terms {
            indices.push(term_id);
            weights.push(weight);
        }
        Self { indices, weights }
    }

    /// Aggregate per-token model weights into one sparse vector.
    ///
    /// `token_ids` and `weights` are parallel, in sequence order.
    /// `special_tokens_mask` is the tokenizer's mask (`1` marks a token the
    /// template added: `[CLS]`/`<s>`, `[SEP]`/`</s>`, padding) and may be empty
    /// when the caller has no mask. `unused_token_ids` drops ids that carry no
    /// lexical content even when the mask does not flag them — `[UNK]` in
    /// particular, plus the pad id when padding was applied without a mask.
    ///
    /// A term that appears several times keeps its **largest** weight, matching
    /// the reference BGE-M3 implementation: the strongest activation is the
    /// evidence, and summing would turn repetition into relevance.
    ///
    /// Positions past the end of either slice are ignored rather than panicking,
    /// so a ragged batch row can never take the indexer down.
    pub fn from_token_weights(
        token_ids: &[u32],
        weights: &[f32],
        special_tokens_mask: &[u32],
        unused_token_ids: &[u32],
    ) -> Self {
        let pairs = token_ids
            .iter()
            .zip(weights.iter())
            .enumerate()
            .filter(|(position, (token_id, _))| {
                special_tokens_mask.get(*position).copied().unwrap_or(0) == 0
                    && !unused_token_ids.contains(token_id)
            })
            .map(|(_, (token_id, weight))| (*token_id, *weight));
        Self::from_pairs(pairs)
    }

    /// Number of active terms.
    pub fn len(&self) -> usize {
        self.indices.len().min(self.weights.len())
    }

    /// True when no term is active.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `(term_id, weight)` pairs in ascending term order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.indices
            .iter()
            .copied()
            .zip(self.weights.iter().copied())
    }

    /// Dot product over the shared term ids — the sparse relevance score.
    ///
    /// Both sides are sorted, so this is a linear merge rather than a hash
    /// lookup per term. The same arithmetic runs in SQL for the stored side;
    /// this implementation is what the unit tests check it against.
    pub fn dot(&self, other: &Self) -> f32 {
        let (mut left, mut right) = (0usize, 0usize);
        let mut score = 0.0f32;
        while left < self.len() && right < other.len() {
            let (Some(a), Some(b)) = (self.indices.get(left), other.indices.get(right)) else {
                break;
            };
            match a.cmp(b) {
                std::cmp::Ordering::Less => left += 1,
                std::cmp::Ordering::Greater => right += 1,
                std::cmp::Ordering::Equal => {
                    let a_weight = self.weights.get(left).copied().unwrap_or(0.0);
                    let b_weight = other.weights.get(right).copied().unwrap_or(0.0);
                    score += a_weight * b_weight;
                    left += 1;
                    right += 1;
                }
            }
        }
        score
    }

    /// Drop the smallest terms, keeping at most `max_terms` of them.
    ///
    /// A BGE-M3 passage activates roughly one term per token; the long tail
    /// contributes almost nothing to the score but dominates the row count of
    /// the term table. Pruning is applied at write time, never at query time.
    pub fn top_terms(mut self, max_terms: usize) -> Self {
        if self.len() <= max_terms {
            return self;
        }
        let mut terms: Vec<(u32, f32)> = self.iter().collect();
        terms.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        terms.truncate(max_terms);
        terms.sort_by_key(|(term_id, _)| *term_id);
        self.indices.clear();
        self.weights.clear();
        for (term_id, weight) in terms {
            self.indices.push(term_id);
            self.weights.push(weight);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_largest_weight_per_token_id() {
        let sparse = SparseEmbedding::from_token_weights(
            &[10, 11, 10, 11, 10],
            &[0.2, 0.9, 0.7, 0.1, 0.4],
            &[],
            &[],
        );
        assert_eq!(sparse.indices, vec![10, 11]);
        assert_eq!(sparse.weights, vec![0.7, 0.9]);
    }

    #[test]
    fn drops_special_padding_and_unused_tokens() {
        // [CLS] alpha [UNK] beta [SEP] [PAD]
        let sparse = SparseEmbedding::from_token_weights(
            &[0, 40, 3, 41, 2, 1],
            &[5.0, 0.8, 4.0, 0.6, 5.0, 5.0],
            &[1, 0, 0, 0, 1, 1],
            &[3],
        );
        assert_eq!(sparse.indices, vec![40, 41]);
        assert_eq!(sparse.weights, vec![0.8, 0.6]);
    }

    #[test]
    fn drops_non_positive_and_non_finite_weights() {
        let sparse = SparseEmbedding::from_token_weights(
            &[7, 8, 9, 10],
            &[0.0, -1.5, f32::NAN, 0.25],
            &[],
            &[],
        );
        assert_eq!(sparse.indices, vec![10]);
        assert_eq!(sparse.weights, vec![0.25]);
    }

    #[test]
    fn ragged_rows_are_truncated_not_panicked_on() {
        let sparse = SparseEmbedding::from_token_weights(&[1, 2, 3], &[0.5], &[], &[]);
        assert_eq!(sparse.indices, vec![1]);
        assert!(!sparse.is_empty());
    }

    #[test]
    fn dot_product_uses_only_shared_terms() {
        let query = SparseEmbedding::from_pairs([(1, 0.5), (4, 2.0), (9, 1.0)]);
        let passage = SparseEmbedding::from_pairs([(4, 0.25), (5, 8.0), (9, 3.0)]);
        assert!((query.dot(&passage) - (2.0 * 0.25 + 1.0 * 3.0)).abs() < 1e-6);
        assert_eq!(query.dot(&SparseEmbedding::empty()), 0.0);
    }

    #[test]
    fn pruning_keeps_the_heaviest_terms_in_term_order() {
        let sparse =
            SparseEmbedding::from_pairs([(1, 0.1), (2, 0.9), (3, 0.5), (4, 0.7)]).top_terms(2);
        assert_eq!(sparse.indices, vec![2, 4]);
        assert_eq!(sparse.weights, vec![0.9, 0.7]);
    }
}
