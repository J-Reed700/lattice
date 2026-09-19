//! Qwen3 as an encoder: one forward pass over a whole batch, no KV cache.
//!
//! `candle_transformers::models::qwen3::Model` is built for generation. It owns
//! a KV cache whose reset is private to that crate, so the only way to give a
//! second sequence a clean cache from outside is to clone the model — which is
//! what this service used to do, once per input, at batch size one. Worse, its
//! causal mask is filled with one row's worth of elements and then handed a
//! shape that claims `batch` of them, so any pass with `batch > 1` builds a
//! tensor whose storage is smaller than its shape says.
//!
//! Embedding needs neither the cache nor generation: one pass over one
//! right-padded batch, then last-token pooling. This is that model with the
//! cache dropped and the mask sized for the batch. Every block is the one the
//! upstream model uses — same `Linear`, same `RmsNorm`, same `repeat_kv`, same
//! rotary kernel — so a single-row pass here and a single-row pass there agree
//! to float noise. Dropping the cache is also what lets `forward` take `&self`.

use std::sync::Arc;

use candle_core::{DType, Device, Module, Result, Tensor};
use candle_nn::{Activation, VarBuilder};
use candle_transformers::models::qwen3::Config;
use candle_transformers::models::with_tracing::{linear_b, linear_no_bias, Linear, RmsNorm};
use candle_transformers::utils::repeat_kv;

/// Rotary sin/cos tables, shared by every layer. Every sequence in an embedding
/// batch starts at position 0, so there is no cache offset to carry.
#[derive(Debug)]
struct RotaryEmbedding {
    sin: Tensor,
    cos: Tensor,
}

impl RotaryEmbedding {
    fn new(dtype: DType, cfg: &Config, device: &Device) -> Result<Self> {
        let dim = cfg.head_dim;
        let max_seq_len = cfg.max_position_embeddings;
        let inv_freq: Vec<f32> = (0..dim)
            .step_by(2)
            .map(|i| 1f32 / cfg.rope_theta.powf(i as f64 / dim as f64) as f32)
            .collect();
        let inv_freq_len = inv_freq.len();
        let inv_freq = Tensor::from_vec(inv_freq, (1, inv_freq_len), device)?;
        let positions = Tensor::arange(0u32, max_seq_len as u32, device)?
            .to_dtype(DType::F32)?
            .reshape((max_seq_len, 1))?;
        let freqs = positions.matmul(&inv_freq)?;
        Ok(Self {
            sin: freqs.sin()?.to_dtype(dtype)?,
            cos: freqs.cos()?.to_dtype(dtype)?,
        })
    }

    /// `q` and `k` are (batch, heads, seq, head_dim).
    fn apply(&self, q: &Tensor, k: &Tensor) -> Result<(Tensor, Tensor)> {
        let (_, _, seq_len, _) = q.dims4()?;
        let cos = self.cos.narrow(0, 0, seq_len)?;
        let sin = self.sin.narrow(0, 0, seq_len)?;
        let q = candle_nn::rotary_emb::rope(&q.contiguous()?, &cos, &sin)?;
        let k = candle_nn::rotary_emb::rope(&k.contiguous()?, &cos, &sin)?;
        Ok((q, k))
    }
}

#[derive(Debug)]
struct Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
    act_fn: Activation,
}

impl Mlp {
    fn new(cfg: &Config, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            gate_proj: linear_no_bias(cfg.hidden_size, cfg.intermediate_size, vb.pp("gate_proj"))?,
            up_proj: linear_no_bias(cfg.hidden_size, cfg.intermediate_size, vb.pp("up_proj"))?,
            down_proj: linear_no_bias(cfg.intermediate_size, cfg.hidden_size, vb.pp("down_proj"))?,
            act_fn: cfg.hidden_act,
        })
    }
}

impl Module for Mlp {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let lhs = xs.apply(&self.gate_proj)?.apply(&self.act_fn)?;
        let rhs = xs.apply(&self.up_proj)?;
        (lhs * rhs)?.apply(&self.down_proj)
    }
}

#[derive(Debug)]
struct Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    num_heads: usize,
    num_kv_heads: usize,
    num_kv_groups: usize,
    head_dim: usize,
    /// `head_dim * num_attention_heads`, which is not always `hidden_size`.
    projected_size: usize,
    rotary: Arc<RotaryEmbedding>,
}

impl Attention {
    fn new(cfg: &Config, rotary: Arc<RotaryEmbedding>, vb: VarBuilder) -> Result<Self> {
        if cfg.use_sliding_window {
            candle_core::bail!("Qwen3 sliding-window attention is not supported");
        }
        let head_dim = cfg.head_dim;
        let num_heads = cfg.num_attention_heads;
        let num_kv_heads = cfg.num_key_value_heads;
        if num_kv_heads == 0 || !num_heads.is_multiple_of(num_kv_heads) {
            candle_core::bail!(
                "Qwen3 config has {num_heads} attention heads and {num_kv_heads} key/value heads"
            );
        }
        Ok(Self {
            q_proj: linear_b(
                cfg.hidden_size,
                num_heads * head_dim,
                cfg.attention_bias,
                vb.pp("q_proj"),
            )?,
            k_proj: linear_b(
                cfg.hidden_size,
                num_kv_heads * head_dim,
                cfg.attention_bias,
                vb.pp("k_proj"),
            )?,
            v_proj: linear_b(
                cfg.hidden_size,
                num_kv_heads * head_dim,
                cfg.attention_bias,
                vb.pp("v_proj"),
            )?,
            o_proj: linear_b(
                num_heads * head_dim,
                cfg.hidden_size,
                cfg.attention_bias,
                vb.pp("o_proj"),
            )?,
            q_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("q_norm"))?,
            k_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("k_norm"))?,
            num_heads,
            num_kv_heads,
            num_kv_groups: num_heads / num_kv_heads,
            head_dim,
            projected_size: head_dim * num_heads,
            rotary,
        })
    }

    fn forward(&self, xs: &Tensor, mask: &Tensor) -> Result<Tensor> {
        let (batch, len, _) = xs.dims3()?;

        let q = self
            .q_proj
            .forward(xs)?
            .reshape((batch, len, self.num_heads, self.head_dim))?
            .transpose(1, 2)?;
        let k = self
            .k_proj
            .forward(xs)?
            .reshape((batch, len, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;
        let v = self
            .v_proj
            .forward(xs)?
            .reshape((batch, len, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;

        // Qwen3 normalizes each head's q and k before RoPE.
        let q = self.q_norm.forward(&q.flatten(0, 2)?)?.reshape((
            batch,
            self.num_heads,
            len,
            self.head_dim,
        ))?;
        let k = self.k_norm.forward(&k.flatten(0, 2)?)?.reshape((
            batch,
            self.num_kv_heads,
            len,
            self.head_dim,
        ))?;

        let (q, k) = self.rotary.apply(&q, &k)?;

        let k = repeat_kv(k, self.num_kv_groups)?.contiguous()?;
        let v = repeat_kv(v, self.num_kv_groups)?.contiguous()?;

        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let scores = (q.matmul(&k.transpose(2, 3)?)? * scale)?.broadcast_add(mask)?;
        let context = candle_nn::ops::softmax_last_dim(&scores)?.matmul(&v)?;

        context
            .transpose(1, 2)?
            .reshape((batch, len, self.projected_size))?
            .apply(&self.o_proj)
    }
}

#[derive(Debug)]
struct DecoderLayer {
    self_attn: Attention,
    mlp: Mlp,
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
}

impl DecoderLayer {
    fn new(cfg: &Config, rotary: Arc<RotaryEmbedding>, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            self_attn: Attention::new(cfg, rotary, vb.pp("self_attn"))?,
            mlp: Mlp::new(cfg, vb.pp("mlp"))?,
            input_layernorm: RmsNorm::new(
                cfg.hidden_size,
                cfg.rms_norm_eps,
                vb.pp("input_layernorm"),
            )?,
            post_attention_layernorm: RmsNorm::new(
                cfg.hidden_size,
                cfg.rms_norm_eps,
                vb.pp("post_attention_layernorm"),
            )?,
        })
    }

    fn forward(&self, xs: &Tensor, mask: &Tensor) -> Result<Tensor> {
        let attended = self
            .self_attn
            .forward(&self.input_layernorm.forward(xs)?, mask)?;
        let xs = (xs + attended)?;
        let projected = self
            .post_attention_layernorm
            .forward(&xs)?
            .apply(&self.mlp)?;
        xs + projected
    }
}

/// A Qwen3 transformer stack that returns every position's final hidden state.
#[derive(Debug)]
pub struct Qwen3Encoder {
    embed_tokens: candle_nn::Embedding,
    layers: Vec<DecoderLayer>,
    norm: RmsNorm,
    device: Device,
    dtype: DType,
}

impl Qwen3Encoder {
    pub fn new(cfg: &Config, vb: VarBuilder) -> Result<Self> {
        let embed_tokens =
            candle_nn::embedding(cfg.vocab_size, cfg.hidden_size, vb.pp("model.embed_tokens"))?;
        let rotary = Arc::new(RotaryEmbedding::new(vb.dtype(), cfg, vb.device())?);
        let layers_vb = vb.pp("model.layers");
        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for layer in 0..cfg.num_hidden_layers {
            layers.push(DecoderLayer::new(
                cfg,
                Arc::clone(&rotary),
                layers_vb.pp(layer),
            )?);
        }
        Ok(Self {
            embed_tokens,
            layers,
            norm: RmsNorm::new(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("model.norm"))?,
            device: vb.device().clone(),
            dtype: vb.dtype(),
        })
    }

    /// Final hidden states for `input_ids` (batch, seq) → (batch, seq, hidden).
    ///
    /// Rows are expected to be right-padded to a common length. Nothing here
    /// masks the padding, and nothing has to: attention is causal, so a real
    /// token at position `i` sees `0..=i` and never the pad tokens that follow
    /// it. The padded rows compute values of their own, which the caller
    /// discards by reading each row at its own last real position.
    pub fn forward(&self, input_ids: &Tensor) -> Result<Tensor> {
        let (_, len) = input_ids.dims2()?;
        let mask = self.causal_mask(len)?;
        let mut hidden = self.embed_tokens.forward(input_ids)?;
        for layer in &self.layers {
            hidden = layer.forward(&hidden, &mask)?;
        }
        self.norm.forward(&hidden)
    }

    /// Additive causal mask of shape (1, 1, len, len), broadcast over batch and
    /// heads. One row's worth of elements is all a causal mask ever needs — the
    /// upstream model's habit of claiming a batch dimension for it is exactly
    /// what stops that model from running a batch at all.
    fn causal_mask(&self, len: usize) -> Result<Tensor> {
        let mask: Vec<f32> = (0..len)
            .flat_map(|row| {
                (0..len).map(move |column| {
                    if column <= row {
                        0f32
                    } else {
                        f32::NEG_INFINITY
                    }
                })
            })
            .collect();
        Tensor::from_vec(mask, (1, 1, len, len), &self.device)?.to_dtype(self.dtype)
    }
}
