//! Forward passes and pooling over the loaded model variants.

use candle_core::{DType, Device, Module, Tensor};
use tokenizers::Tokenizer;
// Tensor indexing helper trait (candle uses an extension-trait pattern for `i`).
use candle_core::IndexOp;

use crate::features::embedding::qwen3_encoder::Qwen3Encoder;
use crate::features::embedding::sparse_head::SparseHead;
use lattice_core::domain::value_objects::SparseEmbedding;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::with_autorelease_pool;

use super::{LoadError, ModelVariant, PoolingStrategy};

/// Run one inference step off the async runtime.
///
/// Metal work blocks for as long as the GPU takes, so it belongs on the
/// blocking pool rather than a runtime worker. It also autoreleases
/// Objective-C objects on every dispatch, which the step's own autorelease
/// pool releases on return; without it they stayed parked on the worker
/// thread for the life of the process (see `shared::runtime::autorelease`).
pub(super) async fn run_inference<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || with_autorelease_pool(f))
        .await
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("inference task: {}", e),
        })?
}

/// Last-token embeddings for one right-padded batch, from a single pass.
///
/// Right padding is what makes a batch exact rather than merely close. Qwen3 is
/// causal: position `i` attends to `0..=i` and never to the pad tokens sitting
/// to its right, so the hidden state at each row's last real token — the one
/// last-token pooling reads — is the state that row would have produced alone.
/// The padded rows do compute values past their own end; nothing reads them.
pub(super) fn qwen3_embed(
    model: &Qwen3Encoder,
    tokenizer: &Tokenizer,
    device: &Device,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    let encodings =
        tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("tokenization: {}", e),
            })?;

    // The attention mask, not the id count, is each row's real length: the ids
    // now run out to the longest row in the batch.
    let lengths: Vec<usize> = encodings
        .iter()
        .map(|encoding| {
            encoding
                .get_attention_mask()
                .iter()
                .filter(|&&flag| flag == 1)
                .count()
        })
        .collect();
    if lengths.contains(&0) {
        return Err(AppError::InvalidInput(
            "Cannot embed an empty token sequence".into(),
        ));
    }

    let batch_size = encodings.len();
    let max_len = encodings
        .iter()
        .map(|encoding| encoding.get_ids().len())
        .max()
        .unwrap_or(0);
    let mut input_ids = Vec::with_capacity(batch_size * max_len);
    for encoding in &encodings {
        input_ids.extend_from_slice(encoding.get_ids());
    }

    let input = Tensor::from_vec(input_ids, (batch_size, max_len), device).map_err(|e| {
        AppError::EmbeddingFailed {
            reason: format!("input_ids tensor: {}", e),
        }
    })?;
    let hidden = model
        .forward(&input)
        .map_err(|e| LoadError::Candle(format!("Qwen3 forward: {}", e)))?;

    let last_tokens = lengths
        .iter()
        .enumerate()
        .map(|(row, &length)| hidden.i((row, length - 1, ..)))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| LoadError::Candle(format!("last-token pooling: {}", e)))?;
    // Back to F32 before the norm: a sum of 1024 squared hidden states
    // overflows f16 long before it overflows anything else.
    let pooled = Tensor::stack(&last_tokens, 0)
        .and_then(|t| t.to_dtype(DType::F32))
        .map_err(|e| LoadError::Candle(format!("pooling: {}", e)))?;
    let normalized = l2_normalize(&pooled).map_err(|e| LoadError::Candle(e.to_string()))?;
    tensor_to_vec_of_vec(&normalized).map_err(|e| LoadError::Candle(e.to_string()).into())
}

/// One padded batch through an encoder model: dense vectors, plus the sparse
/// head read off the same hidden states when `sparse_head` and
/// `sparse_terms` are both given.
pub(super) fn encoder_embed(
    model: &ModelVariant,
    tokenizer: &Tokenizer,
    device: &Device,
    pooling: PoolingStrategy,
    sparse_head: Option<&SparseHead>,
    sparse_terms: Option<usize>,
    texts: &[String],
) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
    let encodings =
        tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("tokenization: {}", e),
            })?;

    let batch_size = encodings.len();
    let max_len = encodings
        .iter()
        .map(|e| e.get_ids().len())
        .max()
        .unwrap_or(0);

    let mut input_ids = Vec::with_capacity(batch_size * max_len);
    let mut attention_mask = Vec::with_capacity(batch_size * max_len);
    let mut token_type_ids = Vec::with_capacity(batch_size * max_len);
    for enc in &encodings {
        input_ids.extend(enc.get_ids().iter().map(|&x| x as i64));
        attention_mask.extend(enc.get_attention_mask().iter().map(|&x| x as i64));
        token_type_ids.extend(enc.get_type_ids().iter().map(|&x| x as i64));
    }

    let input_ids_t = Tensor::from_vec(input_ids, (batch_size, max_len), device).map_err(|e| {
        AppError::EmbeddingFailed {
            reason: format!("input_ids tensor: {}", e),
        }
    })?;
    let attention_mask_t = Tensor::from_vec(attention_mask, (batch_size, max_len), device)
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("attention_mask tensor: {}", e),
        })?;
    let token_type_ids_t = Tensor::from_vec(token_type_ids, (batch_size, max_len), device)
        .map_err(|e| AppError::EmbeddingFailed {
            reason: format!("token_type_ids tensor: {}", e),
        })?;

    let hidden_states = run_encoder(model, &input_ids_t, &attention_mask_t, &token_type_ids_t)?;

    // Read the sparse head off the hidden states we already have. Padding
    // is dropped by trimming each row back to its own token count before
    // aggregation, so a short passage in a long batch cannot pick up terms
    // from the pad token.
    let sparse_vectors = match (sparse_head, sparse_terms) {
        (Some(head), Some(max_terms)) => {
            let rows = head.token_weights(&hidden_states)?;
            encodings
                .iter()
                .zip(rows.iter())
                .map(|(encoding, row)| {
                    let ids = encoding.get_ids();
                    let usable = row.len().min(ids.len());
                    head.aggregate(
                        ids.get(..usable).unwrap_or_default(),
                        row.get(..usable).unwrap_or_default(),
                        encoding.get_special_tokens_mask(),
                        max_terms,
                    )
                })
                .collect()
        }
        _ => Vec::new(),
    };

    let pooled = match pooling {
        PoolingStrategy::Cls => cls_pool(&hidden_states),
        PoolingStrategy::Mean => mean_pool(&hidden_states, &attention_mask_t),
    }
    .map_err(|e| AppError::EmbeddingFailed {
        reason: format!("pooling: {}", e),
    })?;

    let normalized = l2_normalize(&pooled).map_err(|e| AppError::EmbeddingFailed {
        reason: format!("L2 normalize: {}", e),
    })?;

    let vectors = tensor_to_vec_of_vec(&normalized).map_err(|e| AppError::EmbeddingFailed {
        reason: format!("tensor → Vec<Vec<f32>>: {}", e),
    })?;

    Ok((vectors, sparse_vectors))
}

pub(super) fn run_encoder(
    model: &ModelVariant,
    input_ids: &Tensor,
    attention_mask: &Tensor,
    token_type_ids: &Tensor,
) -> Result<Tensor> {
    let hidden_states = match model {
        ModelVariant::Bert(model) => model
            .forward(input_ids, token_type_ids, Some(attention_mask))
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("BertModel forward: {}", e),
            })?,
        ModelVariant::DistilBert(model) => {
            model
                .forward(input_ids, attention_mask)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("DistilBertModel forward: {}", e),
                })?
        }
        ModelVariant::XlmRoberta(model) => model
            .forward(input_ids, attention_mask, token_type_ids, None, None, None)
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("XLMRobertaModel forward: {}", e),
            })?,
        ModelVariant::JinaBert(model) => {
            model
                .forward(input_ids)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("JinaBertModel forward: {}", e),
                })?
        }
        ModelVariant::NomicBert(model) => model
            .forward(input_ids, Some(token_type_ids), Some(attention_mask))
            .map_err(|e| AppError::EmbeddingFailed {
                reason: format!("NomicBertModel forward: {}", e),
            })?,
        ModelVariant::ModernBert(model) => {
            model
                .forward(input_ids, attention_mask)
                .map_err(|e| AppError::EmbeddingFailed {
                    reason: format!("ModernBert forward: {}", e),
                })?
        }
        ModelVariant::Qwen3(_) => unreachable!("Qwen runs its own causal forward pass"),
    };
    Ok(hidden_states)
}

/// CLS pooling: take token at index 0 from each row of (batch, seq, hidden).
fn cls_pool(hidden_states: &Tensor) -> std::result::Result<Tensor, candle_core::Error> {
    hidden_states.i((.., 0, ..))
}

/// Mean pooling: sum hidden states over the seq dim, weighted by attention mask,
/// then divide by the per-row mask sum. Matches sentence-transformers reference.
fn mean_pool(
    hidden_states: &Tensor,
    attention_mask: &Tensor,
) -> std::result::Result<Tensor, candle_core::Error> {
    // hidden_states: (B, S, H), attention_mask: (B, S) i64
    let mask = attention_mask.to_dtype(DType::F32)?.unsqueeze(2)?; // (B, S, 1)
    let masked = hidden_states.broadcast_mul(&mask)?;
    let summed = masked.sum(1)?; // (B, H)
    let counts = mask.sum(1)?.clamp(1f32, f32::INFINITY)?; // (B, 1) avoid div-by-zero
    summed.broadcast_div(&counts)
}

/// L2-normalize each row of a (batch, hidden) tensor.
fn l2_normalize(t: &Tensor) -> std::result::Result<Tensor, candle_core::Error> {
    let norm = t.sqr()?.sum_keepdim(1)?.sqrt()?;
    t.broadcast_div(&norm)
}

/// Convert a (batch, hidden) f32 tensor into the Vec<Vec<f32>> shape callers
/// expect.
fn tensor_to_vec_of_vec(t: &Tensor) -> std::result::Result<Vec<Vec<f32>>, candle_core::Error> {
    let dims = t.dims();
    let (batch, hidden) = match dims {
        [b, h] => (*b, *h),
        other => {
            return Err(candle_core::Error::Msg(format!(
                "expected (batch, hidden) tensor, got {:?}",
                other
            )))
        }
    };
    let flat = t.to_dtype(DType::F32)?.flatten_all()?.to_vec1::<f32>()?;
    let expected_len = batch.checked_mul(hidden).ok_or_else(|| {
        candle_core::Error::Msg("embedding tensor dimensions overflowed usize".to_string())
    })?;
    if hidden == 0 || flat.len() != expected_len {
        return Err(candle_core::Error::Msg(format!(
            "embedding tensor data length {} does not match shape ({batch}, {hidden})",
            flat.len()
        )));
    }
    Ok(flat
        .chunks_exact(hidden)
        .take(batch)
        .map(<[f32]>::to_vec)
        .collect())
}
