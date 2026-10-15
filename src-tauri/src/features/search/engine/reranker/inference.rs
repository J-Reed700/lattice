//! Cross-encoder scoring: tokenizing pairs, batched forward passes and the sigmoid.

use std::sync::Arc;

use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::Module;
use parking_lot::Mutex;
use tokenizers::{Encoding, Tokenizer};

use lattice_core::shared::error::{AppError, Result};

use super::{Inner, RerankResult};

pub(super) fn rerank_sync(
    inner: &Arc<Mutex<Inner>>,
    tokenizer: &Tokenizer,
    query: &str,
    documents: Vec<String>,
    top_k: usize,
    max_length: usize,
) -> Result<Vec<RerankResult>> {
    let scores = score_batch(inner, tokenizer, query, &documents, max_length)?;

    let mut ranked: Vec<RerankResult> = scores
        .into_iter()
        .enumerate()
        .map(|(idx, score)| RerankResult { index: idx, score })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ranked.truncate(top_k);
    Ok(ranked)
}

/// Encode `(query, doc)` pairs and run the cross-encoder forward pass
/// in mini-batches. Returns one logit per document.
fn score_batch(
    inner: &Arc<Mutex<Inner>>,
    tokenizer: &Tokenizer,
    query: &str,
    documents: &[String],
    max_length: usize,
) -> Result<Vec<f32>> {
    if documents.is_empty() {
        return Ok(vec![]);
    }

    // Build (query, doc) pairs. The tokenizer's `encode_batch` with
    // a `(String, String)` input produces the standard
    // `[CLS] query [SEP] doc [SEP]` format with correct
    // `token_type_ids` (0 for query, 1 for doc) — this is what BERT
    // cross-encoders are trained against. Don't try to roll our own
    // `format!("{q} [SEP] {d}")` — the tokenizer handles segment IDs
    // and special tokens correctly.
    let pairs: Vec<(String, String)> = documents
        .iter()
        .map(|d| (query.to_string(), d.clone()))
        .collect();

    let encodings =
        tokenizer
            .encode_batch(pairs, true)
            .map_err(|e| AppError::TokenizationError {
                reason: format!("Tokenization failed: {e}"),
            })?;
    if encodings.len() != documents.len() {
        return Err(AppError::InvalidState(format!(
            "Reranker tokenizer returned {} encodings for {} documents",
            encodings.len(),
            documents.len()
        )));
    }

    // BERT attention cost is quadratic in padded sequence length. Sorting by
    // encoded length prevents one 512-token passage from making every short
    // passage in the same batch pay the maximum cost.
    let mut indexed_encodings: Vec<(usize, Encoding)> = encodings.into_iter().enumerate().collect();
    indexed_encodings.sort_by_key(|(_, encoding)| encoding.len());

    let batch_size = 16usize;
    let mut all_scores = vec![0.0_f32; documents.len()];
    let mut assigned = vec![false; documents.len()];
    let mut batch_start = 0usize;
    while batch_start < indexed_encodings.len() {
        let shortest = indexed_encodings
            .get(batch_start)
            .map(|(_, encoding)| encoding.len().max(1))
            .ok_or_else(|| {
                AppError::InvalidState("Reranker batch start was out of range".to_string())
            })?;
        let bucket_max = shortest.saturating_mul(2);
        let mut batch_end = batch_start + 1;
        while batch_end < indexed_encodings.len()
            && batch_end - batch_start < batch_size
            && indexed_encodings
                .get(batch_end)
                .is_some_and(|(_, encoding)| encoding.len() <= bucket_max)
        {
            batch_end += 1;
        }
        let batch = indexed_encodings
            .get(batch_start..batch_end)
            .ok_or_else(|| {
                AppError::InvalidState("Reranker batch bounds were inconsistent".to_string())
            })?;
        let batch_scores = score_encoded_batch(inner, batch, max_length)?;
        if batch_scores.len() != batch.len() {
            return Err(AppError::InvalidState(
                "Reranker produced an incomplete batch".to_string(),
            ));
        }
        for ((original_index, _), score) in batch.iter().zip(batch_scores) {
            let Some(output) = all_scores.get_mut(*original_index) else {
                return Err(AppError::InvalidState(
                    "Reranker batch index was out of range".to_string(),
                ));
            };
            let Some(was_assigned) = assigned.get_mut(*original_index) else {
                return Err(AppError::InvalidState(
                    "Reranker assignment index was out of range".to_string(),
                ));
            };
            if *was_assigned {
                return Err(AppError::InvalidState(
                    "Reranker assigned a candidate twice".to_string(),
                ));
            }
            *output = score;
            *was_assigned = true;
        }
        batch_start = batch_end;
    }
    if assigned.iter().any(|assigned| !assigned) {
        return Err(AppError::InvalidState(
            "Reranker did not score every candidate".to_string(),
        ));
    }

    Ok(all_scores)
}

fn score_encoded_batch(
    inner: &Arc<Mutex<Inner>>,
    encodings: &[(usize, Encoding)],
    max_length: usize,
) -> Result<Vec<f32>> {
    let batch_size = encodings.len();
    if batch_size == 0 {
        return Ok(vec![]);
    }

    let sequence_length = encodings
        .iter()
        .map(|(_, encoding)| encoding.len())
        .max()
        .unwrap_or(1)
        .min(max_length)
        .max(1);

    let mut input_ids_flat = Vec::with_capacity(batch_size * sequence_length);
    let mut attention_mask_flat = Vec::with_capacity(batch_size * sequence_length);
    let mut token_type_ids_flat = Vec::with_capacity(batch_size * sequence_length);

    for (_, encoding) in encodings {
        let ids = encoding.get_ids();
        let mask = encoding.get_attention_mask();
        let types = encoding.get_type_ids();

        // Pair-aware truncation already preserved the tokenizer's special
        // tokens. Pad to this batch's sequence length with 0 because the
        // BERT tokenizer's pad token id is 0 (per config.json
        // `pad_token_id: 0`). The attention_mask is 0 for pads, which
        // makes BertModel's softmax mask them out.
        let mut row_ids: Vec<u32> = ids.iter().take(sequence_length).copied().collect();
        let mut row_mask: Vec<u32> = mask.iter().take(sequence_length).copied().collect();
        let mut row_types: Vec<u32> = types.iter().take(sequence_length).copied().collect();
        let pad = sequence_length.saturating_sub(row_ids.len());
        row_ids.extend(std::iter::repeat_n(0u32, pad));
        row_mask.extend(std::iter::repeat_n(0u32, pad));
        row_types.extend(std::iter::repeat_n(0u32, pad));

        input_ids_flat.extend(row_ids);
        attention_mask_flat.extend(row_mask);
        token_type_ids_flat.extend(row_types);
    }

    let guard = inner.lock();
    let device = guard.device.clone();

    // BERT expects i64 input_ids. candle_nn::Embedding handles the
    // dtype conversion if we feed u32, but matching HF's published
    // shape exactly avoids surprises.
    let input_ids = Tensor::from_vec(input_ids_flat, (batch_size, sequence_length), &device)
        .map_err(|e| AppError::Other(format!("Failed to build input_ids tensor: {e}")))?
        .to_dtype(DType::U32)
        .map_err(|e| AppError::Other(format!("input_ids dtype cast failed: {e}")))?;
    let attention_mask =
        Tensor::from_vec(attention_mask_flat, (batch_size, sequence_length), &device)
            .map_err(|e| AppError::Other(format!("Failed to build attention_mask tensor: {e}")))?
            .to_dtype(DType::U32)
            .map_err(|e| AppError::Other(format!("attention_mask dtype cast failed: {e}")))?;
    let token_type_ids =
        Tensor::from_vec(token_type_ids_flat, (batch_size, sequence_length), &device)
            .map_err(|e| AppError::Other(format!("Failed to build token_type_ids tensor: {e}")))?
            .to_dtype(DType::U32)
            .map_err(|e| AppError::Other(format!("token_type_ids dtype cast failed: {e}")))?;

    // Forward pass: [batch, seq, hidden]. We need the [CLS] hidden
    // state — index 0 along seq.
    let hidden_states = guard
        .model
        .forward(&input_ids, &token_type_ids, Some(&attention_mask))
        .map_err(|e| AppError::Other(format!("BertModel forward failed: {e}")))?;

    // [batch, hidden]
    let cls_states = hidden_states
        .i((.., 0, ..))
        .map_err(|e| AppError::Other(format!("Failed to slice CLS token: {e}")))?
        .to_device(&Device::Cpu)
        .map_err(|e| AppError::Other(format!("Failed to transfer CLS state to CPU: {e}")))?;

    // [batch, 1] — single logit per pair
    let logits = guard
        .classifier
        .forward(&cls_states)
        .map_err(|e| AppError::Other(format!("Classifier forward failed: {e}")))?;

    drop(guard);

    let logits_shape = logits.dims();
    if logits_shape.len() != 2 || logits_shape.get(1) != Some(&1) {
        return Err(AppError::Other(format!(
            "Reranker output has unexpected shape {:?}; expected [batch, 1] for a \
             single-label cross-encoder",
            logits_shape
        )));
    }

    let raw_logits: Vec<f32> = logits
        .squeeze(1)
        .map_err(|e| AppError::Other(format!("Failed to squeeze logits: {e}")))?
        .to_vec1()
        .map_err(|e| AppError::Other(format!("Failed to materialize logits: {e}")))?;

    // Sigmoid into [0, 1] so the downstream blend (which clamps and
    // mixes with BM25/vector scores) preserves rank ordering. See
    // module-level "Output semantics" docs.
    let scores = raw_logits.into_iter().map(sigmoid).collect();
    Ok(scores)
}

#[inline]
pub(super) fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
