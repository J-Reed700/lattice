//! misaki's fallback for words no lexicon knows: a one-layer BART that spells
//! graphemes into British phonemes (`PeterReid/graphemes_to_phonemes_en_gb`),
//! decoded greedily.

use std::collections::HashMap;
use std::path::Path;

use candle_core::{DType, Device, Module, Result, Tensor, D};
use candle_nn::{Embedding, LayerNorm, Linear, VarBuilder};

const BOS: u32 = 1;
const EOS: u32 = 2;
const UNKNOWN: u32 = 3;
/// BART's learned positions start two rows into the table.
const POSITION_OFFSET: usize = 2;

#[derive(serde::Deserialize)]
struct Config {
    d_model: usize,
    max_position_embeddings: usize,
    grapheme_chars: String,
    phoneme_chars: String,
}

struct Attention {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
}

impl Attention {
    fn load(vb: VarBuilder, width: usize) -> Result<Self> {
        Ok(Self {
            q: candle_nn::linear(width, width, vb.pp("q_proj"))?,
            k: candle_nn::linear(width, width, vb.pp("k_proj"))?,
            v: candle_nn::linear(width, width, vb.pp("v_proj"))?,
            out: candle_nn::linear(width, width, vb.pp("out_proj"))?,
        })
    }

    /// One head over `[1, T, d]` queries and `[1, S, d]` keys.
    fn forward(&self, x: &Tensor, memory: &Tensor, causal: bool) -> Result<Tensor> {
        let width = x.dim(D::Minus1)?;
        let q = (self.q.forward(x)? / (width as f64).sqrt())?;
        let k = self.k.forward(memory)?;
        let v = self.v.forward(memory)?;
        let mut scores = q.matmul(&k.t()?)?;
        if causal {
            let len = x.dim(1)?;
            let mask: Vec<f32> = (0..len)
                .flat_map(|i| (0..len).map(move |j| if j > i { f32::NEG_INFINITY } else { 0.0 }))
                .collect();
            scores = scores.broadcast_add(&Tensor::from_vec(mask, (len, len), x.device())?)?;
        }
        self.out
            .forward(&candle_nn::ops::softmax_last_dim(&scores)?.matmul(&v)?)
    }
}

struct Feedforward {
    fc1: Linear,
    fc2: Linear,
    norm: LayerNorm,
}

impl Feedforward {
    fn load(vb: VarBuilder, width: usize) -> Result<Self> {
        let hidden = vb.get_unchecked("fc1.weight")?.dim(0)?;
        Ok(Self {
            fc1: candle_nn::linear(width, hidden, vb.pp("fc1"))?,
            fc2: candle_nn::linear(hidden, width, vb.pp("fc2"))?,
            norm: candle_nn::layer_norm(width, 1e-5, vb.pp("final_layer_norm"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let y = self.fc2.forward(&self.fc1.forward(x)?.gelu_erf()?)?;
        self.norm.forward(&(y + x)?)
    }
}

struct Embedder {
    positions: Tensor,
    norm: LayerNorm,
}

impl Embedder {
    fn load(vb: VarBuilder, width: usize) -> Result<Self> {
        Ok(Self {
            positions: vb.get_unchecked("embed_positions.weight")?,
            norm: candle_nn::layer_norm(width, 1e-5, vb.pp("layernorm_embedding"))?,
        })
    }

    fn forward(&self, tokens: &Embedding, ids: &[u32], device: &Device) -> Result<Tensor> {
        let len = ids.len();
        let ids = Tensor::new(ids, device)?.unsqueeze(0)?;
        let positions = self
            .positions
            .narrow(0, POSITION_OFFSET, len)?
            .unsqueeze(0)?;
        self.norm.forward(&tokens.forward(&ids)?.add(&positions)?)
    }
}

pub(super) struct Fallback {
    shared: Embedding,
    logits_bias: Tensor,
    encoder_embed: Embedder,
    encoder_attention: Attention,
    encoder_norm: LayerNorm,
    encoder_ffn: Feedforward,
    decoder_embed: Embedder,
    self_attention: Attention,
    self_norm: LayerNorm,
    cross_attention: Attention,
    cross_norm: LayerNorm,
    decoder_ffn: Feedforward,
    graphemes: HashMap<char, u32>,
    phonemes: Vec<char>,
    max_len: usize,
    device: Device,
}

impl Fallback {
    pub(super) fn load(config: &Path, weights: &Path) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(config)?)
            .map_err(|e| candle_core::Error::Msg(format!("G2P config: {e}")))?;
        let device = Device::Cpu;
        let vb =
            VarBuilder::from_buffered_safetensors(std::fs::read(weights)?, DType::F32, &device)?;
        let width = config.d_model;
        let shared_table = vb.get_unchecked("model.shared.weight")?;
        let vocab = shared_table.dim(0)?;
        let encoder = vb.pp("model.encoder");
        let decoder = vb.pp("model.decoder");
        let enc_layer = encoder.pp("layers.0");
        let dec_layer = decoder.pp("layers.0");
        let mut graphemes = HashMap::new();
        for (i, c) in config.grapheme_chars.chars().enumerate() {
            graphemes.insert(c, i as u32);
        }
        Ok(Self {
            shared: Embedding::new(shared_table, width),
            logits_bias: vb.get((1, vocab), "final_logits_bias")?,
            encoder_embed: Embedder::load(encoder.clone(), width)?,
            encoder_attention: Attention::load(enc_layer.pp("self_attn"), width)?,
            encoder_norm: candle_nn::layer_norm(width, 1e-5, enc_layer.pp("self_attn_layer_norm"))?,
            encoder_ffn: Feedforward::load(enc_layer, width)?,
            decoder_embed: Embedder::load(decoder.clone(), width)?,
            self_attention: Attention::load(dec_layer.pp("self_attn"), width)?,
            self_norm: candle_nn::layer_norm(width, 1e-5, dec_layer.pp("self_attn_layer_norm"))?,
            cross_attention: Attention::load(dec_layer.pp("encoder_attn"), width)?,
            cross_norm: candle_nn::layer_norm(
                width,
                1e-5,
                dec_layer.pp("encoder_attn_layer_norm"),
            )?,
            decoder_ffn: Feedforward::load(dec_layer, width)?,
            graphemes,
            phonemes: config.phoneme_chars.chars().collect(),
            max_len: config.max_position_embeddings,
            device,
        })
    }

    /// The phonemes of `word`, as the model spells them.
    pub(super) fn read(&self, word: &str) -> Result<String> {
        let mut ids = vec![BOS];
        ids.extend(
            word.chars()
                .take(self.max_len.saturating_sub(2))
                .map(|c| self.graphemes.get(&c).copied().unwrap_or(UNKNOWN)),
        );
        ids.push(EOS);

        let x = self
            .encoder_embed
            .forward(&self.shared, &ids, &self.device)?;
        let x = self
            .encoder_norm
            .forward(&(self.encoder_attention.forward(&x, &x, false)? + &x)?)?;
        let memory = self.encoder_ffn.forward(&x)?;

        let mut output = vec![BOS];
        while output.len() < self.max_len {
            let y = self
                .decoder_embed
                .forward(&self.shared, &output, &self.device)?;
            let y = self
                .self_norm
                .forward(&(self.self_attention.forward(&y, &y, true)? + &y)?)?;
            let y = self
                .cross_norm
                .forward(&(self.cross_attention.forward(&y, &memory, false)? + &y)?)?;
            let y = self.decoder_ffn.forward(&y)?;
            let last = y.narrow(1, output.len() - 1, 1)?.squeeze(1)?;
            let logits = last
                .matmul(&self.shared.embeddings().t()?)?
                .add(&self.logits_bias)?;
            let next = logits.argmax(D::Minus1)?.squeeze(0)?.to_scalar::<u32>()?;
            if next == EOS {
                break;
            }
            output.push(next);
        }
        Ok(output
            .iter()
            .filter(|&&id| id > UNKNOWN)
            .filter_map(|&id| self.phonemes.get(id as usize))
            .collect())
    }
}
