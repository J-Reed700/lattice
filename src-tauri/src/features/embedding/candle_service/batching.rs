//! Length-bucketed batching, the query cache and the sparse forward pass.

use std::ops::Range;
use std::sync::Arc;

use crate::application::ports::embedding_port::{span_chunk_texts, sparse_not_supported};
use crate::domain::value_objects::SparseEmbedding;
use crate::features::embedding::late_chunking::LateChunkingError;
use crate::features::embedding::sparse_head::MAX_PASSAGE_TERMS;
use crate::shared::error::AppError;
use crate::shared::error::Result;

use super::forward::{encoder_embed, qwen3_embed, run_inference};
use super::{
    CandleEmbeddingService, ModelArchitecture, ModelVariant, ENCODER_PADDED_TOKEN_BUDGET,
    MAX_BATCH_ROWS, QWEN3_PADDED_TOKEN_BUDGET,
};

impl CandleEmbeddingService {
    /// The padded-token ceiling for one forward pass on this architecture.
    fn padded_token_budget(&self) -> usize {
        match self.architecture {
            ModelArchitecture::Qwen3 => QWEN3_PADDED_TOKEN_BUDGET,
            _ => ENCODER_PADDED_TOKEN_BUDGET,
        }
    }

    /// Group `texts` into batches of similar length, returning each batch as
    /// indices into `texts`.
    ///
    /// Padding is what a batch actually costs: every row is padded out to the
    /// longest row in it, so a 12-token query batched with a 2000-token passage
    /// pays for 2000 tokens. Sorting by length first puts similar rows
    /// together, and budgeting by `rows × longest row` rather than by a fixed
    /// row count means a batch of long passages cannot blow up memory while a
    /// batch of short ones is free to be much larger than 16.
    ///
    /// This is also the single place inputs are validated against the model
    /// window; the forward pass takes its texts as already checked.
    pub(super) fn plan_batches(&self, texts: &[String]) -> Result<Vec<Vec<usize>>> {
        let mut by_length: Vec<(usize, usize)> = Vec::with_capacity(texts.len());
        for (index, text) in texts.iter().enumerate() {
            let tokens = self.input_policy.count(text)?;
            if tokens > self.input_policy.max_tokens {
                // `count` already tokenized; `validate` is here only to raise
                // the one over-long-input message the splitter also uses.
                self.input_policy.validate(text)?;
            }
            by_length.push((tokens, index));
        }
        by_length.sort_unstable();

        let budget = self.padded_token_budget();
        let mut batches: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        for (tokens, index) in by_length {
            // Ascending order, so `tokens` is this batch's longest row once the
            // row joins it.
            let full = current.len() >= MAX_BATCH_ROWS
                || tokens.saturating_mul(current.len() + 1) > budget;
            if full && !current.is_empty() {
                batches.push(std::mem::take(&mut current));
            }
            current.push(index);
        }
        if !current.is_empty() {
            batches.push(current);
        }
        Ok(batches)
    }

    /// Embed `texts` in length-bucketed batches and hand the vectors back in
    /// the caller's order.
    ///
    /// Each batch takes and releases the model lock on its own, so a query
    /// embedding can slip between two indexing batches instead of waiting out
    /// the whole run.
    pub(super) async fn forward_batched(
        &self,
        texts: Vec<String>,
        sparse_terms: Option<usize>,
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if texts.is_empty() {
            return Ok((vec![], vec![]));
        }
        let plan = self.plan_batches(&texts)?;
        let mut dense = vec![Vec::new(); texts.len()];
        let mut sparse = vec![SparseEmbedding::empty(); texts.len()];
        // Each text is handed to exactly one batch, so moving it out of the
        // pending list is both the cheapest way to build the batch and the
        // check that the plan covered every input exactly once.
        let mut pending: Vec<Option<String>> = texts.into_iter().map(Some).collect();
        for batch in plan {
            let inputs: Vec<String> = batch
                .iter()
                .filter_map(|&index| pending.get_mut(index).and_then(Option::take))
                .collect();
            if inputs.len() != batch.len() {
                return Err(AppError::InvalidState(
                    "Embedding batch plan referenced an input twice".into(),
                ));
            }
            let (batch_dense, batch_sparse) =
                self.forward_with_sparse(inputs, sparse_terms).await?;
            if batch_dense.len() != batch.len() {
                return Err(AppError::EmbeddingFailed {
                    reason: format!(
                        "forward pass returned {} vectors for {} inputs",
                        batch_dense.len(),
                        batch.len()
                    ),
                });
            }
            for (&index, vector) in batch.iter().zip(batch_dense) {
                if let Some(slot) = dense.get_mut(index) {
                    *slot = vector;
                }
            }
            for (&index, terms) in batch.iter().zip(batch_sparse) {
                if let Some(slot) = sparse.get_mut(index) {
                    *slot = terms;
                }
            }
        }
        Ok((dense, sparse))
    }

    /// Embed one string that already carries whichever prefix its role calls
    /// for. Every public entry point funnels through here once it has decided
    /// between the query and the document prefix; `content_start` is where
    /// that role prefix ends.
    pub(super) async fn embed_prepared(
        &self,
        text: String,
        content_start: usize,
    ) -> Result<Vec<f32>> {
        if self.strategy.is_late_chunking() {
            return self.embed_content_mean(&text, content_start).await;
        }
        let mut out = self.forward_batched(vec![text], None).await?.0;
        out.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Forward pass returned no embeddings".into(),
        })
    }

    /// One vector pooled the way late chunking pools a chunk: the mean of the
    /// tokens from `content_start` on, special tokens excluded. Under late
    /// chunking every dense vector in the generation — query, lone passage,
    /// and a chunk that could not be late chunked — is made this way, because
    /// the model's own pooling (CLS for BGE, last token for Qwen3) is a
    /// different vector space from a mean and the two cannot be compared.
    pub(super) async fn embed_content_mean(
        &self,
        text: &str,
        content_start: usize,
    ) -> Result<Vec<f32>> {
        let content = content_start.min(text.len())..text.len();
        let pooled = match self
            .embed_late_chunked(text, std::slice::from_ref(&content))
            .await
        {
            // Nothing after the prefix tokenized (an empty passage behind a
            // context prefix): the prefix is all there is to pool.
            Err(LateChunkingError::EmptyChunkSpan { .. }) if content_start > 0 => {
                self.embed_late_chunked(text, std::slice::from_ref(&(0..text.len())))
                    .await
            }
            outcome => outcome,
        };
        pooled?.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Forward pass returned no embeddings".into(),
        })
    }

    /// Embed each chunk of a span on its own, for a span (or window) that
    /// could not be late chunked. Under late chunking the chunk is still pooled
    /// as a mean over its own tokens behind the span's shared prefix, so these
    /// vectors land in the same space as their late-chunked neighbours.
    /// `span_text` already carries the document prefix.
    pub(super) async fn embed_chunks_individually(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        let texts = span_chunk_texts(span_text, chunk_ranges)?;
        if !self.strategy.is_late_chunking() {
            // Not `embed_batch`: that would put the document prefix in twice.
            return Ok(self.forward_batched(texts, None).await?.0);
        }
        let content_start = chunk_ranges.first().map_or(0, |range| range.start);
        let mut vectors = Vec::with_capacity(texts.len());
        for text in &texts {
            vectors.push(self.embed_content_mean(text, content_start).await?);
        }
        Ok(vectors)
    }

    /// A previously embedded query, if this service has seen it.
    pub(super) fn cached_query(&self, prefixed: &str) -> Option<Arc<Vec<f32>>> {
        let mut cache = self
            .query_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(prefixed).map(Arc::clone)
    }

    pub(super) fn cache_query(&self, prefixed: String, vector: Arc<Vec<f32>>) {
        let mut cache = self
            .query_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.put(prefixed, vector);
    }

    /// The dense forward pass, optionally also reading the learned sparse head
    /// off the *same* hidden states.
    ///
    /// `sparse_terms` is the per-passage term budget; `None` skips the sparse
    /// head entirely. When it is `Some`, the encoder runs exactly once and
    /// both representations come out of one set of hidden states — the whole
    /// reason `embed_batch_with_sparse` exists rather than two calls.
    ///
    /// The returned sparse vector is empty (never absent) for a passage whose
    /// every token was special or scored zero, so the two returned vectors are
    /// always the same length and stay aligned with `texts`.
    ///
    /// `texts` is one padded batch and must already fit the model window;
    /// [`Self::plan_batches`] is what decides both.
    async fn forward_with_sparse(
        &self,
        texts: Vec<String>,
        sparse_terms: Option<usize>,
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if texts.is_empty() {
            return Ok((vec![], vec![]));
        }
        let sparse_head = match sparse_terms {
            Some(_) => Some(
                self.sparse_head
                    .as_ref()
                    .ok_or_else(|| sparse_not_supported(self.identity.as_str()))?,
            ),
            None => None,
        };

        let guard = Arc::clone(&self.model).lock_owned().await;
        let tokenizer = Arc::clone(&self.tokenizer);
        let device = self.device.clone();
        let pooling = self.pooling;
        let sparse_head = sparse_head.cloned();
        run_inference(move || match &*guard {
            ModelVariant::Qwen3(template) => {
                let dense = qwen3_embed(template, &tokenizer, &device, &texts)?;
                let sparse = vec![SparseEmbedding::empty(); dense.len()];
                Ok((dense, sparse))
            }
            model => encoder_embed(
                model,
                &tokenizer,
                &device,
                pooling,
                sparse_head.as_deref(),
                sparse_terms,
                &texts,
            ),
        })
        .await
    }

    /// Dense vectors and learned sparse term weights from a single forward
    /// pass per batch.
    ///
    /// This is the indexing path for a model with a sparse head: running
    /// `embed_batch` and `embed_sparse_batch` separately would double the
    /// encoder work for identical hidden states.
    ///
    /// # Errors
    ///
    /// Returns the "not supported" error when this model has no sparse head.
    /// Check [`EmbeddingPort::supports_sparse`] first.
    pub async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        if self.sparse_head.is_none() {
            return Err(sparse_not_supported(self.identity.as_str()));
        }
        self.forward_batched(self.as_documents(texts), Some(MAX_PASSAGE_TERMS))
            .await
    }

    /// `texts` with the model's document prefix in front of each, which is
    /// what a passage is actually embedded as.
    pub(super) fn as_documents(&self, texts: &[String]) -> Vec<String> {
        texts
            .iter()
            .map(|text| self.prefixes.document_input(text).into_owned())
            .collect()
    }
}
