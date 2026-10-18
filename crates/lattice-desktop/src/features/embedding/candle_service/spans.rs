//! Contextualized and late-chunked embedding of spans within a passage.

use std::ops::Range;
use std::sync::Arc;

use candle_core::{DType, Tensor};
use tokenizers::Tokenizer;

use crate::features::embedding::late_chunking::{
    l2_normalize_in_place, mean_pool_rows, pooling_token_indices, validate_chunk_ranges,
    window_groups, LateChunkingError,
};
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;

use super::forward::{run_encoder, run_inference};
use super::{CandleEmbeddingService, LoadError, ModelVariant};

impl CandleEmbeddingService {
    /// Specialized embedding for chunker output. Same code path as
    /// `embed_batch` — we just unwrap the contextualized text first.
    pub async fn embed_contextualized_chunks(
        &self,
        chunks: &[lattice_core::domain::value_objects::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        if chunks.is_empty() {
            return Ok(vec![]);
        }
        let texts: Vec<String> = chunks
            .iter()
            .map(|c| c.contextualized_content.clone())
            .collect();
        <Self as EmbeddingPort>::embed_batch(self, &texts).await
    }

    /// Embed every chunk of one structure span from a single forward pass —
    /// "late chunking".
    ///
    /// `span_text` is the whole span *including* its leading
    /// "[Document: title | Page | Section]" context prefix, and
    /// `chunk_char_ranges` are the UTF-8 byte ranges of the individual chunks
    /// inside it. The span is tokenized once with offsets and run through the
    /// model once; each chunk vector is the mean of the final hidden states of
    /// the tokens that start inside that chunk's range, L2-normalized. Prefix
    /// tokens and the special tokens ([CLS]/[SEP], BOS/EOS) condition the pass
    /// but are pooled into nothing — the prefix informs every chunk without
    /// polluting any chunk's vector with its own literal text.
    ///
    /// Qwen3 normally pools the *last* token of a sequence, because its causal
    /// attention only lets that position see the whole input. There is no
    /// per-chunk position with that property here, so — like every other
    /// architecture — we mean-pool the chunk's token states. Mean pooling over
    /// the chunk span is the late-chunking convention (Günther et al., 2024);
    /// it produces a different vector space than last-token pooling, which is
    /// exactly why [`CandleEmbeddingService::model_identity`] carries the
    /// `late-chunking-v1` marker whenever this path is active.
    ///
    /// # Errors
    ///
    /// Returns [`LateChunkingError::SpanTooLong`] when the span does not fit
    /// the model window, and [`LateChunkingError::EmptyChunkSpan`] /
    /// [`LateChunkingError::InvalidRange`] when the ranges cannot be pooled.
    /// All three satisfy [`LateChunkingError::allows_fallback`]: the caller
    /// re-embeds that span with the ordinary per-chunk path.
    pub async fn embed_late_chunked(
        &self,
        span_text: &str,
        chunk_char_ranges: &[Range<usize>],
    ) -> std::result::Result<Vec<Vec<f32>>, LateChunkingError> {
        if chunk_char_ranges.is_empty() {
            return Ok(Vec::new());
        }
        validate_chunk_ranges(span_text, chunk_char_ranges)?;

        // Padding would add tokens with meaningless offsets; a span is a single
        // sequence, so there is nothing to pad to.
        let mut tokenizer = Tokenizer::clone(&self.tokenizer);
        tokenizer.with_padding(None);
        let encoding = tokenizer.encode(span_text, true).map_err(|e| {
            LateChunkingError::Embedding(LoadError::Tokenizer(e.to_string()).into())
        })?;
        let token_count = encoding.get_ids().len();
        if token_count == 0 {
            return Err(LateChunkingError::EmptyChunkSpan { chunk: 0 });
        }
        if token_count > self.input_policy.max_tokens {
            return Err(LateChunkingError::SpanTooLong {
                tokens: token_count,
                limit: self.input_policy.max_tokens,
            });
        }

        let groups = pooling_token_indices(
            encoding.get_offsets(),
            encoding.get_special_tokens_mask(),
            chunk_char_ranges,
        )?;

        let hidden = self.span_hidden_states(&encoding).await?;
        let mut vectors = Vec::with_capacity(groups.len());
        for (chunk, group) in groups.iter().enumerate() {
            let mut pooled = mean_pool_rows(&hidden, group)
                .ok_or(LateChunkingError::EmptyChunkSpan { chunk })?;
            l2_normalize_in_place(&mut pooled);
            vectors.push(pooled);
        }
        Ok(vectors)
    }

    /// Late chunk a span of any length, one forward pass per window of whole
    /// chunks.
    ///
    /// A span that fits the model window is a single window and behaves exactly
    /// as [`embed_late_chunked`](Self::embed_late_chunked) always did. A longer
    /// span used to fall back to chunk-first in its entirety, which threw away
    /// the whole point of late chunking; now only the conditioning that would
    /// have crossed a window edge is lost. Each window carries the span's shared
    /// context prefix — the bytes before the first chunk range — so every chunk
    /// stays conditioned on the document's identity.
    async fn embed_late_chunked_windows(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> std::result::Result<Vec<Vec<f32>>, LateChunkingError> {
        validate_chunk_ranges(span_text, chunk_ranges)?;
        let Some(prefix_end) = chunk_ranges.first().map(|range| range.start) else {
            return Ok(Vec::new());
        };
        match self.embed_late_chunked(span_text, chunk_ranges).await {
            Err(LateChunkingError::SpanTooLong { .. }) => {}
            outcome => return outcome,
        }

        // Windowing reads a window as one contiguous slice, which only holds
        // for ranges in chunk order. Anything else keeps the old whole-span
        // fallback rather than pooling the wrong bytes.
        if chunk_ranges.windows(2).any(|pair| match pair {
            [a, b] => b.start < a.start || b.end < a.end,
            _ => false,
        }) {
            return Err(LateChunkingError::InvalidRange { chunk: 0 });
        }

        let slice = |range: Range<usize>| {
            span_text
                .get(range)
                .ok_or(LateChunkingError::InvalidRange { chunk: 0 })
        };
        let prefix = slice(0..prefix_end)?;
        let count = |text: &str| {
            self.input_policy
                .count(text)
                .map_err(LateChunkingError::Embedding)
        };
        let mut chunk_tokens = Vec::with_capacity(chunk_ranges.len());
        for range in chunk_ranges {
            chunk_tokens.push(count(slice(range.clone())?)?);
        }
        let budget = self.input_policy.max_tokens.saturating_sub(count(prefix)?);

        let mut vectors = Vec::with_capacity(chunk_ranges.len());
        for window in window_groups(&chunk_tokens, budget) {
            // Ranges arrive in chunk order, so the window's text is one
            // contiguous slice behind the shared prefix.
            let Some(group) = chunk_ranges.get(window) else {
                continue;
            };
            let (start, end) = match (group.first(), group.last()) {
                (Some(first), Some(last)) => (first.start, last.end),
                _ => continue,
            };
            let text = format!("{prefix}{}", slice(start..end)?);
            let shifted: Vec<Range<usize>> = group
                .iter()
                .map(|range| prefix_end + (range.start - start)..prefix_end + (range.end - start))
                .collect();
            match self.embed_late_chunked(&text, &shifted).await {
                Ok(pooled) => vectors.extend(pooled),
                // One window that still will not fit answers for itself rather
                // than dropping the whole span back to chunk-first.
                Err(error) if error.allows_fallback() => {
                    tracing::debug!(
                        chunks = shifted.len(),
                        reason = %error,
                        "Late-chunking window is not poolable; embedding its chunks individually"
                    );
                    vectors.extend(
                        self.embed_chunks_individually(&text, &shifted)
                            .await
                            .map_err(LateChunkingError::Embedding)?,
                    );
                }
                Err(error) => return Err(error),
            }
        }
        Ok(vectors)
    }

    /// Late chunk when the strategy is on and the span is eligible; otherwise
    /// embed each chunk on its own. This is what the port and service traits
    /// expose, so callers never have to handle the fallback themselves.
    pub(super) async fn embed_span(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        // The model's document prefix conditions the span the same way the
        // chunker's context prefix does: it goes in front of everything and is
        // pooled into nothing. Shifting the ranges past it keeps them pointing
        // at the same characters, and keeps them out of the prefix, so the
        // per-chunk fallback below re-adds the prefix per chunk rather than
        // slicing a piece of it into one.
        if !self.prefixes.document.is_empty() && !chunk_ranges.is_empty() {
            let offset = self.prefixes.document.len();
            let shifted: Vec<Range<usize>> = chunk_ranges
                .iter()
                .map(|range| range.start + offset..range.end + offset)
                .collect();
            let prefixed = self.prefixes.document_input(span_text);
            return self.embed_span_inner(&prefixed, &shifted).await;
        }
        self.embed_span_inner(span_text, chunk_ranges).await
    }

    /// [`Self::embed_span`] once the document prefix has been applied.
    async fn embed_span_inner(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        if self.strategy.is_late_chunking() && !chunk_ranges.is_empty() {
            match self
                .embed_late_chunked_windows(span_text, chunk_ranges)
                .await
            {
                Ok(vectors) => return Ok(vectors),
                Err(error) if error.allows_fallback() => {
                    tracing::debug!(
                        chunks = chunk_ranges.len(),
                        reason = %error,
                        "Span is not late chunkable; embedding its chunks individually"
                    );
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.embed_chunks_individually(span_text, chunk_ranges)
            .await
    }

    /// One forward pass over one already-tokenized sequence, returned as
    /// (seq, hidden) rows. Only late chunking needs per-token states.
    async fn span_hidden_states(&self, encoding: &tokenizers::Encoding) -> Result<Vec<Vec<f32>>> {
        let guard = Arc::clone(&self.model).lock_owned().await;
        let device = self.device.clone();
        let encoding = encoding.clone();
        run_inference(move || {
            let ids = encoding.get_ids();
            let length = ids.len();
            let hidden_states = match &*guard {
                // No KV cache to clear, so no clone: the encoder is stateless
                // across passes, exactly as the batch path needs it to be.
                ModelVariant::Qwen3(model) => {
                    let input = Tensor::new(ids, &device)
                        .and_then(|t| t.unsqueeze(0))
                        .map_err(|e| LoadError::Candle(e.to_string()))?;
                    model
                        .forward(&input)
                        .map_err(|e| LoadError::Candle(e.to_string()))?
                }
                model => {
                    let to_i64 =
                        |values: &[u32]| values.iter().map(|&v| v as i64).collect::<Vec<_>>();
                    let tensor = |values: Vec<i64>| {
                        Tensor::from_vec(values, (1, length), &device).map_err(|e| {
                            AppError::EmbeddingFailed {
                                reason: format!("span tensor: {}", e),
                            }
                        })
                    };
                    let input_ids_t = tensor(to_i64(ids))?;
                    let attention_mask_t = tensor(to_i64(encoding.get_attention_mask()))?;
                    let token_type_ids_t = tensor(to_i64(encoding.get_type_ids()))?;
                    run_encoder(model, &input_ids_t, &attention_mask_t, &token_type_ids_t)?
                }
            };
            drop(guard);

            hidden_states
                .squeeze(0)
                .and_then(|t| t.to_dtype(DType::F32))
                .and_then(|t| t.contiguous())
                .and_then(|t| t.to_vec2::<f32>())
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("hidden states → rows: {}", e),
                })
        })
        .await
    }
}
