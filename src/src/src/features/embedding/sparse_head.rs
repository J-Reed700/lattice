//! BGE-M3's learned sparse head.
//!
//! The published BGE-M3 checkpoint ships a second, tiny head next to the
//! encoder: `sparse_linear.pt`, a `Linear(hidden_size, 1)`. Applied per token
//! to the final hidden states and passed through ReLU, it produces one
//! non-negative weight per token — "how much does this position contribute
//! lexically". Aggregating those weights per token id (keeping the maximum)
//! gives a sparse term vector over the tokenizer vocabulary, which is what
//! `features::search::engine::sparse_search` scores against.
//!
//! # Why this is a separate module
//!
//! [`CandleEmbeddingService`](super::candle_service::CandleEmbeddingService)
//! owns model loading and the dense forward pass. The sparse head is optional,
//! architecture-gated, and its own file on disk; keeping it here means the
//! dense path is unchanged whether or not the head is present.
//!
//! # File format
//!
//! `sparse_linear.pt` is a PyTorch pickle (`torch.save(state_dict)`), which
//! Candle reads natively through `candle_core::pickle` — the loader is part of
//! `candle-core` and is not behind a cargo feature, so nothing in `Cargo.toml`
//! had to change. A `sparse_linear.safetensors` conversion is accepted first
//! when present, for anyone who would rather not ship a pickle:
//!
//! ```text
//! python -c "
//! import torch, safetensors.torch as st
//! st.save_file({k: v.contiguous() for k, v in torch.load('sparse_linear.pt', map_location='cpu').items()},
//!              'sparse_linear.safetensors')"
//! ```
//!
//! Either file holds exactly two tensors, `weight` of shape `(1, hidden)` and
//! `bias` of shape `(1,)` (some exports prefix them with `sparse_linear.`).

use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Module, Tensor};
use tokenizers::Tokenizer;

use crate::domain::value_objects::SparseEmbedding;
use crate::shared::error::AppError;
use crate::shared::result::Result;

/// The PyTorch pickle the BGE-M3 repository publishes.
pub const SPARSE_HEAD_PT: &str = "sparse_linear.pt";
/// A locally converted safetensors copy, preferred when present.
pub const SPARSE_HEAD_SAFETENSORS: &str = "sparse_linear.safetensors";

/// At most this many terms are kept per passage.
///
/// A BGE-M3 passage activates roughly one term per token, and the tail below
/// the top few hundred contributes a rounding error to the dot product while
/// multiplying the row count of `chunk_sparse_terms`. Queries are pruned far
/// harder still — see [`MAX_QUERY_TERMS`].
pub const MAX_PASSAGE_TERMS: usize = 256;

/// At most this many terms are kept for a query.
///
/// Every query term becomes one row of the SQL `VALUES` join, so this is the
/// knob that bounds the scoring query's width.
pub const MAX_QUERY_TERMS: usize = 96;

/// Where the sparse head lives for a model directory, if it is there at all.
///
/// A converted safetensors file wins over the published pickle so an operator
/// can opt out of loading pickles without editing any code.
pub fn sparse_head_path(model_dir: &Path) -> Option<PathBuf> {
    [SPARSE_HEAD_SAFETENSORS, SPARSE_HEAD_PT]
        .into_iter()
        .map(|name| model_dir.join(name))
        .find(|path| path.is_file())
}

/// The loaded `Linear(hidden, 1)` head plus the token ids it must ignore.
#[derive(Debug)]
pub struct SparseHead {
    linear: candle_nn::Linear,
    /// Token ids that never carry lexical evidence: `[CLS]`/`<s>`,
    /// `[SEP]`/`</s>`, `[PAD]`, `[UNK]`, `[MASK]`. The tokenizer's
    /// special-tokens mask already covers the template tokens on most models;
    /// this catches `[UNK]` and any model whose mask is absent.
    unused_token_ids: Vec<u32>,
}

impl SparseHead {
    /// Load the head for `model_dir`, or `None` when the model does not ship
    /// one.
    ///
    /// A present-but-unreadable head is an error rather than a silent `None`:
    /// quietly indexing a BGE-M3 corpus with no sparse rows would look exactly
    /// like a model that has no sparse head at all, and only show up as
    /// missing recall months later.
    pub fn load(
        model_dir: &Path,
        hidden_size: usize,
        device: &Device,
        tokenizer: &Tokenizer,
    ) -> Result<Option<Self>> {
        let Some(path) = sparse_head_path(model_dir) else {
            return Ok(None);
        };
        let (weight, bias) = read_head_tensors(&path, device)?;
        let dims = weight.dims().to_vec();
        if dims != vec![1, hidden_size] {
            return Err(AppError::InvalidConfig(format!(
                "{} holds a {:?} weight; a sparse head must be (1, {hidden_size})",
                path.display(),
                dims
            )));
        }
        Ok(Some(Self {
            linear: candle_nn::Linear::new(weight, Some(bias)),
            unused_token_ids: unused_token_ids(tokenizer),
        }))
    }

    /// Per-token weights for a `(batch, seq, hidden)` tensor of final hidden
    /// states, as `relu(linear(h))`, returned as `(batch, seq)` rows.
    pub fn token_weights(&self, hidden_states: &Tensor) -> Result<Vec<Vec<f32>>> {
        let scored = self
            .linear
            .forward(&hidden_states.to_dtype(DType::F32).map_err(candle)?)
            .and_then(|t| t.relu())
            .and_then(|t| t.squeeze(candle_core::D::Minus1))
            .map_err(candle)?;
        match scored.dims() {
            [_, _] => scored.to_vec2::<f32>().map_err(candle),
            [_] => scored.to_vec1::<f32>().map(|row| vec![row]).map_err(candle),
            other => Err(AppError::EmbeddingFailed {
                reason: format!("sparse head produced an unexpected shape {other:?}"),
            }),
        }
    }

    /// Aggregate one sequence's token weights into a sparse term vector.
    pub fn aggregate(
        &self,
        token_ids: &[u32],
        weights: &[f32],
        special_tokens_mask: &[u32],
        max_terms: usize,
    ) -> SparseEmbedding {
        SparseEmbedding::from_token_weights(
            token_ids,
            weights,
            special_tokens_mask,
            &self.unused_token_ids,
        )
        .top_terms(max_terms)
    }

    /// The token ids this head drops. Exposed for diagnostics and tests.
    pub fn unused_token_ids(&self) -> &[u32] {
        &self.unused_token_ids
    }
}

fn candle(error: candle_core::Error) -> AppError {
    AppError::EmbeddingFailed {
        reason: format!("sparse head: {error}"),
    }
}

/// Read `weight` and `bias` out of either supported container.
fn read_head_tensors(path: &Path, device: &Device) -> Result<(Tensor, Tensor)> {
    let named: Vec<(String, Tensor)> = if path
        .extension()
        .is_some_and(|extension| extension == "safetensors")
    {
        candle_core::safetensors::load(path, device)
            .map_err(candle)?
            .into_iter()
            .collect()
    } else {
        // `candle_core::pickle` is the PyTorch `.pt` reader that ships with
        // candle-core by default; no cargo feature gates it.
        candle_core::pickle::read_all(path).map_err(candle)?
    };

    let pick = |suffix: &str| -> Option<Tensor> {
        named
            .iter()
            .find(|(name, _)| name == suffix || name.ends_with(&format!(".{suffix}")))
            .map(|(_, tensor)| tensor.clone())
    };
    let weight = pick("weight").ok_or_else(|| {
        AppError::InvalidConfig(format!("{} has no `weight` tensor", path.display()))
    })?;
    let bias = pick("bias").ok_or_else(|| {
        AppError::InvalidConfig(format!("{} has no `bias` tensor", path.display()))
    })?;
    let to_device = |tensor: Tensor| -> Result<Tensor> {
        tensor
            .to_device(device)
            .and_then(|t| t.to_dtype(DType::F32))
            .and_then(|t| t.contiguous())
            .map_err(candle)
    };
    Ok((to_device(weight)?, to_device(bias)?))
}

/// Token ids with no lexical content, resolved through the loaded tokenizer so
/// BERT (`[CLS]`) and RoBERTa (`<s>`) style vocabularies both work.
fn unused_token_ids(tokenizer: &Tokenizer) -> Vec<u32> {
    let mut ids: Vec<u32> = [
        "[CLS]", "[SEP]", "[PAD]", "[UNK]", "[MASK]", "<s>", "</s>", "<pad>", "<unk>", "<mask>",
    ]
    .into_iter()
    .filter_map(|token| tokenizer.token_to_id(token))
    .collect();
    if let Some(padding) = tokenizer.get_padding() {
        ids.push(padding.pad_id);
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn head_is_absent_when_no_file_ships() {
        let dir = TempDir::new().unwrap();
        assert!(sparse_head_path(dir.path()).is_none());
    }

    #[test]
    fn a_converted_safetensors_head_wins_over_the_pickle() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(SPARSE_HEAD_PT), b"pickle").unwrap();
        assert_eq!(
            sparse_head_path(dir.path()),
            Some(dir.path().join(SPARSE_HEAD_PT))
        );
        std::fs::write(dir.path().join(SPARSE_HEAD_SAFETENSORS), b"safetensors").unwrap();
        assert_eq!(
            sparse_head_path(dir.path()),
            Some(dir.path().join(SPARSE_HEAD_SAFETENSORS))
        );
    }

    #[test]
    fn a_wrongly_shaped_head_is_rejected_rather_than_ignored() {
        let dir = TempDir::new().unwrap();
        let device = Device::Cpu;
        let weight = Tensor::zeros((1, 8), DType::F32, &device).unwrap();
        let bias = Tensor::zeros(1, DType::F32, &device).unwrap();
        candle_core::safetensors::save(
            &std::collections::HashMap::from([
                ("weight".to_string(), weight),
                ("bias".to_string(), bias),
            ]),
            dir.path().join(SPARSE_HEAD_SAFETENSORS),
        )
        .unwrap();
        let tokenizer =
            tokenizers::Tokenizer::new(tokenizers::models::wordpiece::WordPiece::default());
        let error = SparseHead::load(dir.path(), 1024, &device, &tokenizer).unwrap_err();
        assert!(error.to_string().contains("sparse head must be"));
        assert!(SparseHead::load(dir.path(), 8, &device, &tokenizer)
            .unwrap()
            .is_some());
    }

    #[test]
    fn token_weights_are_relu_of_the_linear_projection() {
        let device = Device::Cpu;
        let dir = TempDir::new().unwrap();
        // w = [1, -1], b = 0 → weight(t) = relu(h0 - h1)
        let weight = Tensor::from_vec(vec![1.0f32, -1.0], (1, 2), &device).unwrap();
        let bias = Tensor::zeros(1, DType::F32, &device).unwrap();
        candle_core::safetensors::save(
            &std::collections::HashMap::from([
                ("sparse_linear.weight".to_string(), weight),
                ("sparse_linear.bias".to_string(), bias),
            ]),
            dir.path().join(SPARSE_HEAD_SAFETENSORS),
        )
        .unwrap();
        let tokenizer =
            tokenizers::Tokenizer::new(tokenizers::models::wordpiece::WordPiece::default());
        let head = SparseHead::load(dir.path(), 2, &device, &tokenizer)
            .unwrap()
            .unwrap();

        // one batch, three tokens: (3, -1), (0.5, 0.25), (-2, 1)
        let hidden =
            Tensor::from_vec(vec![3.0f32, -1.0, 0.5, 0.25, -2.0, 1.0], (1, 3, 2), &device).unwrap();
        let rows = head.token_weights(&hidden).unwrap();
        assert_eq!(rows.len(), 1);
        let row = rows.first().unwrap();
        assert!((row.first().copied().unwrap() - 4.0).abs() < 1e-5);
        assert!((row.get(1).copied().unwrap() - 0.25).abs() < 1e-5);
        assert!(
            row.get(2).copied().unwrap().abs() < 1e-5,
            "negative projections clamp to zero"
        );

        // The same head aggregates: token 7 repeats and keeps its max, the
        // masked position is dropped even though its weight is the largest.
        let sparse = head.aggregate(&[7, 7, 9], row, &[0, 0, 1], MAX_PASSAGE_TERMS);
        assert_eq!(sparse.indices, vec![7]);
        assert!((sparse.weights.first().copied().unwrap() - 4.0).abs() < 1e-5);
    }
}
