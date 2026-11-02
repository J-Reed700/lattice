//! The phoneme-level ALBERT (PL-BERT) that reads the whole utterance before
//! durations are predicted.

use candle_core::{Module, Result, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear};

use super::weights::Weights;

const LAYERS: usize = 12;
const HEADS: usize = 12;
const NORM_EPS: f64 = 1e-12;

/// ALBERT's one layer, shared by all twelve passes.
struct Layer {
    query: Linear,
    key: Linear,
    value: Linear,
    dense: Linear,
    attention_norm: LayerNorm,
    ffn: Linear,
    ffn_output: Linear,
    output_norm: LayerNorm,
}

pub(super) struct Albert {
    words: Embedding,
    positions: Embedding,
    token_type: Tensor,
    embedding_norm: LayerNorm,
    mapping_in: Linear,
    layer: Layer,
}

fn layer_norm(weights: &Weights, prefix: &str) -> Result<LayerNorm> {
    Ok(LayerNorm::new(
        weights.get(&format!("{prefix}.weight"))?,
        weights.get(&format!("{prefix}.bias"))?,
        NORM_EPS,
    ))
}

impl Albert {
    pub(super) fn load(weights: &Weights) -> Result<Self> {
        let word_table = weights.get("embeddings.word_embeddings.weight")?;
        let position_table = weights.get("embeddings.position_embeddings.weight")?;
        let hidden = word_table.dim(1)?;
        let layer = "encoder.albert_layer_groups.0.albert_layers.0";
        Ok(Self {
            words: Embedding::new(word_table, hidden),
            positions: Embedding::new(position_table, hidden),
            token_type: weights
                .get("embeddings.token_type_embeddings.weight")?
                .narrow(0, 0, 1)?,
            embedding_norm: layer_norm(weights, "embeddings.LayerNorm")?,
            mapping_in: weights.linear("encoder.embedding_hidden_mapping_in")?,
            layer: Layer {
                query: weights.linear(&format!("{layer}.attention.query"))?,
                key: weights.linear(&format!("{layer}.attention.key"))?,
                value: weights.linear(&format!("{layer}.attention.value"))?,
                dense: weights.linear(&format!("{layer}.attention.dense"))?,
                attention_norm: layer_norm(weights, &format!("{layer}.attention.LayerNorm"))?,
                ffn: weights.linear(&format!("{layer}.ffn"))?,
                ffn_output: weights.linear(&format!("{layer}.ffn_output"))?,
                output_norm: layer_norm(weights, &format!("{layer}.full_layer_layer_norm"))?,
            },
        })
    }

    /// `ids` `[1, T]` → hidden states `[1, T, 768]`.
    pub(super) fn forward(&self, ids: &Tensor) -> Result<Tensor> {
        let len = ids.dim(1)?;
        let positions = Tensor::arange(0u32, len as u32, ids.device())?.unsqueeze(0)?;
        let embedded = self
            .words
            .forward(ids)?
            .add(&self.positions.forward(&positions)?)?
            .broadcast_add(&self.token_type)?;
        let mut hidden = self
            .mapping_in
            .forward(&self.embedding_norm.forward(&embedded)?)?;
        for _ in 0..LAYERS {
            hidden = self.layer.forward(&hidden)?;
        }
        Ok(hidden)
    }
}

impl Layer {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (batch, len, hidden) = x.dims3()?;
        let head_dim = hidden / HEADS;
        let heads = |t: Tensor| -> Result<Tensor> {
            t.reshape((batch, len, HEADS, head_dim))?
                .transpose(1, 2)?
                .contiguous()
        };
        let q = heads(self.query.forward(x)?)?;
        let k = heads(self.key.forward(x)?)?;
        let v = heads(self.value.forward(x)?)?;
        let scores = (q.matmul(&k.t()?)? / (head_dim as f64).sqrt())?;
        let context = candle_nn::ops::softmax_last_dim(&scores)?
            .matmul(&v)?
            .transpose(1, 2)?
            .reshape((batch, len, hidden))?;
        let attended = self
            .attention_norm
            .forward(&self.dense.forward(&context)?.add(x)?)?;
        let ffn = self
            .ffn_output
            .forward(&self.ffn.forward(&attended)?.gelu()?)?;
        self.output_norm.forward(&ffn.add(&attended)?)
    }
}
