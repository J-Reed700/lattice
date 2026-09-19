//! Late chunking: embed a whole structure span once, then pool per chunk.
//!
//! The default (`EmbeddingStrategy::ChunkFirst`) path embeds every chunk on its
//! own, so a chunk only ever sees its own text plus a short context prefix.
//! Late chunking (Günther et al., 2024) inverts the order: the entire span —
//! prefix included — goes through one forward pass, and each chunk's vector is
//! the mean of the *token states* that fall inside that chunk. Every chunk is
//! therefore conditioned on the rest of the span (pronouns, a heading two
//! paragraphs up, a table caption) while the stored chunk text, its offsets and
//! its citations stay exactly what they were.
//!
//! Everything in this module is pure: token offsets in, index groups out. The
//! tensor work lives in `candle_service.rs`, which means the mapping rules —
//! special-token exclusion, prefix exclusion, chunk assignment — are unit
//! testable without model weights.

use std::ops::Range;

use crate::shared::error::AppError;

/// Marker mixed into `model_identity()` when late chunking is active. Vectors
/// pooled from a shared forward pass are NOT interchangeable with chunk-first
/// vectors, so they must live in a different embedding generation.
pub const LATE_CHUNKING_IDENTITY_MARKER: &str = "late-chunking-v1";

/// How chunk vectors are produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbeddingStrategy {
    /// Embed each chunk separately with its context prefix prepended.
    #[default]
    ChunkFirst,
    /// Embed the whole span once and mean-pool each chunk's token states.
    LateChunking,
}

impl EmbeddingStrategy {
    pub fn is_late_chunking(self) -> bool {
        matches!(self, Self::LateChunking)
    }
}

/// Reasons a span cannot be late chunked. Everything except
/// [`LateChunkingError::Embedding`] means "this span is not eligible"; the
/// caller answers by embedding the chunks the ordinary way.
#[derive(Debug, thiserror::Error)]
pub enum LateChunkingError {
    #[error("span is {tokens} tokens; the model window holds {limit}")]
    SpanTooLong { tokens: usize, limit: usize },
    #[error("chunk {chunk} of the span contains no poolable token")]
    EmptyChunkSpan { chunk: usize },
    #[error("chunk {chunk} is not a character-aligned range inside the span text")]
    InvalidRange { chunk: usize },
    #[error(transparent)]
    Embedding(#[from] AppError),
}

impl LateChunkingError {
    /// True when the caller should retry the span on the chunk-first path.
    /// Inference failures are real errors and propagate instead.
    pub fn allows_fallback(&self) -> bool {
        !matches!(self, Self::Embedding(_))
    }
}

impl From<LateChunkingError> for AppError {
    fn from(error: LateChunkingError) -> Self {
        match error {
            LateChunkingError::Embedding(inner) => inner,
            other => AppError::EmbeddingFailed {
                reason: other.to_string(),
            },
        }
    }
}

/// The identity of the vector space for `strategy`. Chunk-first keeps the bare
/// artifact digest so existing vectors stay valid; late chunking gets its own
/// suffix so the two generations can never be compared or merged.
pub fn strategy_identity(artifact_identity: &str, strategy: EmbeddingStrategy) -> String {
    match strategy {
        EmbeddingStrategy::ChunkFirst => artifact_identity.to_owned(),
        EmbeddingStrategy::LateChunking => {
            format!("{artifact_identity}+{LATE_CHUNKING_IDENTITY_MARKER}")
        }
    }
}

/// Map token offsets onto chunk ranges, producing the token indices to pool for
/// each chunk.
///
/// Rules, in order:
/// - Special tokens ([CLS]/[SEP]/BOS/EOS, padding) never pool into a chunk.
///   The tokenizer reports them in `special_tokens_mask`, and they also carry
///   an empty `(0, 0)` offset, so both signals are honoured.
/// - A token belongs to the chunk whose range contains the token's *start*
///   offset. Chunk ranges produced by the input policy are contiguous and
///   character-aligned, but a single token can still straddle a boundary; tying
///   assignment to the start offset gives that token to exactly one chunk and
///   never drops or double-counts it. Overlapping ranges (a sliding-window
///   chunker) legitimately share tokens, which this also allows.
/// - Bytes outside every range — in particular the leading
///   "[Document: title | Page | Section]" prefix, which precedes the first
///   range — condition the forward pass but are pooled into nothing.
pub fn pooling_token_indices(
    offsets: &[(usize, usize)],
    special_tokens_mask: &[u32],
    chunk_ranges: &[Range<usize>],
) -> Result<Vec<Vec<usize>>, LateChunkingError> {
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); chunk_ranges.len()];
    for (token, &(start, end)) in offsets.iter().enumerate() {
        if special_tokens_mask.get(token).copied().unwrap_or(0) == 1 {
            continue;
        }
        if end <= start {
            continue;
        }
        for (index, range) in chunk_ranges.iter().enumerate() {
            if start >= range.start && start < range.end {
                if let Some(group) = groups.get_mut(index) {
                    group.push(token);
                }
            }
        }
    }
    for (chunk, group) in groups.iter().enumerate() {
        if group.is_empty() {
            return Err(LateChunkingError::EmptyChunkSpan { chunk });
        }
    }
    Ok(groups)
}

/// Group consecutive chunks into forward passes.
///
/// A span longer than the model window used to drop back to chunk-first
/// wholesale, which threw away the only thing late chunking buys. Packing whole
/// chunks into windows instead keeps every chunk conditioned on its neighbours;
/// only the conditioning that crosses a window edge is lost.
///
/// `chunk_tokens` are the chunks' own token counts and `budget` is what is left
/// of the window once the shared prefix is counted. Because tokenizing the
/// pieces separately can differ from tokenizing their concatenation, the packing
/// is an estimate: the caller re-tokenizes each window for real and answers an
/// overflowing one on its own. A chunk that exceeds `budget` by itself becomes a
/// window of one rather than being dropped.
pub fn window_groups(chunk_tokens: &[usize], budget: usize) -> Vec<Range<usize>> {
    let mut windows = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (index, tokens) in chunk_tokens.iter().enumerate() {
        if index > start && used + tokens > budget {
            windows.push(start..index);
            (start, used) = (index, 0);
        }
        used += tokens;
    }
    if start < chunk_tokens.len() {
        windows.push(start..chunk_tokens.len());
    }
    windows
}

/// Reject ranges that do not address whole characters inside `span_text`
/// before any tokenization work happens.
pub fn validate_chunk_ranges(
    span_text: &str,
    chunk_ranges: &[Range<usize>],
) -> Result<(), LateChunkingError> {
    for (chunk, range) in chunk_ranges.iter().enumerate() {
        if range.start > range.end
            || range.end > span_text.len()
            || !span_text.is_char_boundary(range.start)
            || !span_text.is_char_boundary(range.end)
        {
            return Err(LateChunkingError::InvalidRange { chunk });
        }
    }
    Ok(())
}

/// Mean of the selected rows of a (seq, hidden) hidden-state matrix.
pub fn mean_pool_rows(hidden: &[Vec<f32>], indices: &[usize]) -> Option<Vec<f32>> {
    let first = hidden.get(*indices.first()?)?;
    let mut pooled = vec![0f32; first.len()];
    for index in indices {
        let row = hidden.get(*index)?;
        if row.len() != pooled.len() {
            return None;
        }
        for (slot, value) in pooled.iter_mut().zip(row.iter()) {
            *slot += *value;
        }
    }
    let count = indices.len() as f32;
    for slot in &mut pooled {
        *slot /= count;
    }
    Some(pooled)
}

/// L2-normalize in place, matching what the pooled chunk-first path emits.
/// A zero vector is left alone rather than turned into NaNs.
pub fn l2_normalize_in_place(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 && norm.is_finite() {
        for value in vector.iter_mut() {
            *value /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    // A one-element slice of ranges is exactly what a single-chunk span looks
    // like; it is not a mistyped `[value; len]`.
    #![allow(clippy::single_range_in_vec_init)]

    use super::*;
    use crate::application::ports::embedding_port::span_chunk_texts;

    /// Offsets for "PREFIX\n\nalpha beta gamma" style spans, hand-built so the
    /// mapping rules are exercised without a tokenizer or model weights.
    fn span_offsets() -> (Vec<(usize, usize)>, Vec<u32>) {
        // [CLS] PREFIX \n\n alpha beta gamma [SEP]
        let offsets = vec![
            (0, 0),   // [CLS]
            (0, 6),   // "PREFIX"
            (8, 13),  // "alpha"
            (14, 18), // "beta"
            (19, 24), // "gamma"
            (0, 0),   // [SEP]
        ];
        let mask = vec![1, 0, 0, 0, 0, 1];
        (offsets, mask)
    }

    #[test]
    fn pooling_excludes_special_tokens_and_the_context_prefix() {
        let (offsets, mask) = span_offsets();
        // Chunk ranges start after "PREFIX\n\n" (8 bytes), so prefix tokens and
        // both special tokens must be absent from every group.
        let groups = pooling_token_indices(&offsets, &mask, &[8..18, 18..24]).unwrap();
        assert_eq!(groups, vec![vec![2, 3], vec![4]]);
        assert!(groups
            .iter()
            .flatten()
            .all(|token| *token != 0 && *token != 1 && *token != offsets.len() - 1));
    }

    #[test]
    fn a_boundary_straddling_token_lands_in_exactly_one_chunk() {
        let (offsets, mask) = span_offsets();
        // Boundary at 16 falls inside the "beta" token (14..18).
        let groups = pooling_token_indices(&offsets, &mask, &[8..16, 16..24]).unwrap();
        assert_eq!(groups, vec![vec![2, 3], vec![4]]);
        let assigned: Vec<usize> = groups.iter().flatten().copied().collect();
        let mut unique = assigned.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(assigned.len(), unique.len(), "no token pooled twice");
    }

    #[test]
    fn overlapping_ranges_share_their_tokens() {
        let (offsets, mask) = span_offsets();
        let groups = pooling_token_indices(&offsets, &mask, &[8..24, 14..24]).unwrap();
        assert_eq!(groups, vec![vec![2, 3, 4], vec![3, 4]]);
    }

    #[test]
    fn a_chunk_without_tokens_is_not_late_chunkable() {
        let (offsets, mask) = span_offsets();
        // 13..14 is the single space between "alpha" and "beta": no token starts there.
        let error = pooling_token_indices(&offsets, &mask, &[8..13, 13..14]).unwrap_err();
        assert!(matches!(
            error,
            LateChunkingError::EmptyChunkSpan { chunk: 1 }
        ));
        assert!(error.allows_fallback());
    }

    #[test]
    fn prefix_tokens_are_excluded_by_where_the_ranges_start() {
        let (offsets, mask) = span_offsets();
        // Token 1 is the prefix. It is only ever pooled if a caller points a
        // chunk range at it, which `prepare_structured_with_spans` never does —
        // its ranges begin at `prefix.len()`.
        assert_eq!(
            pooling_token_indices(&offsets, &mask, &[0..24]).unwrap(),
            vec![vec![1, 2, 3, 4]]
        );
        assert_eq!(
            pooling_token_indices(&offsets, &mask, &[8..24]).unwrap(),
            vec![vec![2, 3, 4]]
        );
        // A range past the last token pools nothing at all.
        assert!(matches!(
            pooling_token_indices(&offsets, &mask, &[24..30]).unwrap_err(),
            LateChunkingError::EmptyChunkSpan { chunk: 0 }
        ));
    }

    #[test]
    fn ranges_must_address_whole_characters_inside_the_span() {
        let span = "PREFIX\n\nαβγ";
        assert!(validate_chunk_ranges(span, &[8..span.len()]).is_ok());
        assert!(matches!(
            validate_chunk_ranges(span, &[8..span.len() + 1]),
            Err(LateChunkingError::InvalidRange { chunk: 0 })
        ));
        // 9 splits the two bytes of 'α'.
        assert!(matches!(
            validate_chunk_ranges(span, &[8..9]),
            Err(LateChunkingError::InvalidRange { chunk: 0 })
        ));
    }

    #[test]
    fn windows_hold_as_many_whole_chunks_as_fit() {
        // Four 400-token chunks in a 1000-token budget: two, then two.
        assert_eq!(window_groups(&[400, 400, 400, 400], 1000), vec![0..2, 2..4]);
        // A span that fits is one window, which is the original behaviour.
        assert_eq!(window_groups(&[400, 400], 1000), vec![0..2]);
        assert!(window_groups(&[], 1000).is_empty());
    }

    #[test]
    fn a_chunk_larger_than_the_budget_still_gets_its_own_window() {
        assert_eq!(window_groups(&[2000, 10], 1000), vec![0..1, 1..2]);
        assert_eq!(window_groups(&[10, 20, 30], 0), vec![0..1, 1..2, 2..3]);
        // Every chunk lands in exactly one window, in order.
        let windows = window_groups(&[300, 300, 300, 300, 300], 700);
        assert_eq!(windows.first().map(|w| w.start), Some(0));
        assert_eq!(windows.last().map(|w| w.end), Some(5));
        assert!(windows.windows(2).all(|pair| pair[0].end == pair[1].start));
    }

    #[test]
    fn identity_changes_only_for_late_chunking() {
        let base = "sha256:abc123";
        assert_eq!(strategy_identity(base, EmbeddingStrategy::ChunkFirst), base);
        let late = strategy_identity(base, EmbeddingStrategy::LateChunking);
        assert_ne!(late, base);
        assert!(late.starts_with(base));
        assert!(late.contains(LATE_CHUNKING_IDENTITY_MARKER));
        assert_eq!(EmbeddingStrategy::default(), EmbeddingStrategy::ChunkFirst);
    }

    #[test]
    fn fallback_texts_rebuild_the_chunk_first_inputs() {
        let span = "PREFIX\n\nalpha beta gamma";
        let texts = span_chunk_texts(span, &[8..18, 18..24]).unwrap();
        assert_eq!(texts, vec!["PREFIX\n\nalpha beta", "PREFIX\n\n gamma"]);
        assert!(span_chunk_texts(span, &[]).unwrap().is_empty());
        assert!(span_chunk_texts(span, &[8..999]).is_err());
    }

    #[test]
    fn pooling_averages_rows_and_normalizes() {
        let hidden = vec![vec![1.0, 0.0], vec![3.0, 0.0], vec![0.0, 4.0]];
        let mut pooled = mean_pool_rows(&hidden, &[0, 1]).unwrap();
        assert_eq!(pooled, vec![2.0, 0.0]);
        l2_normalize_in_place(&mut pooled);
        assert_eq!(pooled, vec![1.0, 0.0]);
        let mut zero = vec![0.0, 0.0];
        l2_normalize_in_place(&mut zero);
        assert_eq!(zero, vec![0.0, 0.0]);
        assert!(mean_pool_rows(&hidden, &[]).is_none());
        assert!(mean_pool_rows(&hidden, &[9]).is_none());
    }
}
