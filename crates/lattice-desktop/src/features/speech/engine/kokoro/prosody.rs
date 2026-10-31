//! What the utterance sounds like before it is voiced: per-phoneme durations,
//! the pitch (F0) and energy curves, and the text encoding the decoder reads.

use candle_core::{Module, Result, Tensor, D};
use candle_nn::{Conv1dConfig, Embedding, Linear};

use super::conv::Conv;
use super::decoder::AdainResBlock;
use super::weights::{leaky_relu, same, Weights};

const NORM_EPS: f64 = 1e-5;

/// One direction of a PyTorch LSTM layer. The input projection runs as one
/// matmul over the sequence; only the recurrence steps one frame at a time.
struct Direction {
    input: Linear,
    recurrent: Vec<f32>,
    hidden: usize,
}

/// A one-layer bidirectional LSTM, `[1, T, in]` → `[1, T, 2·hidden]`.
pub(super) struct BiLstm {
    forward: Direction,
    backward: Direction,
}

impl Direction {
    fn load(weights: &Weights, prefix: &str, suffix: &str) -> Result<Self> {
        let w_hh = weights.get(&format!("{prefix}.weight_hh_l0{suffix}"))?;
        let hidden = w_hh.dim(1)?;
        let bias = weights
            .get(&format!("{prefix}.bias_ih_l0{suffix}"))?
            .add(&weights.get(&format!("{prefix}.bias_hh_l0{suffix}"))?)?;
        Ok(Self {
            input: Linear::new(
                weights.get(&format!("{prefix}.weight_ih_l0{suffix}"))?,
                Some(bias),
            ),
            recurrent: w_hh.flatten_all()?.to_vec1()?,
            hidden,
        })
    }

    /// The hidden state at every step, in sequence order.
    fn run(&self, xs: &Tensor, reverse: bool) -> Result<Vec<Vec<f32>>> {
        let projected = self.input.forward(xs)?.squeeze(0)?.to_vec2::<f32>()?;
        let hidden = self.hidden;
        let mut h = vec![0f32; hidden];
        let mut c = vec![0f32; hidden];
        let mut outputs = vec![Vec::new(); projected.len()];
        let mut gates = vec![0f32; 4 * hidden];
        let steps: Box<dyn Iterator<Item = (&Vec<f32>, &mut Vec<f32>)>> = if reverse {
            Box::new(projected.iter().zip(outputs.iter_mut()).rev())
        } else {
            Box::new(projected.iter().zip(outputs.iter_mut()))
        };
        for (input, output) in steps {
            for ((gate, row), x) in gates
                .iter_mut()
                .zip(self.recurrent.chunks_exact(hidden))
                .zip(input)
            {
                *gate = x + row.iter().zip(&h).map(|(w, h)| w * h).sum::<f32>();
            }
            let mut parts = gates.chunks_exact(hidden);
            let (Some(i), Some(f), Some(g), Some(o)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                return Err(candle_core::Error::Msg("LSTM gate size".into()));
            };
            for ((((c, h), i), f), (g, o)) in c
                .iter_mut()
                .zip(h.iter_mut())
                .zip(i)
                .zip(f)
                .zip(g.iter().zip(o))
            {
                *c = sigmoid(*f) * *c + sigmoid(*i) * g.tanh();
                *h = sigmoid(*o) * c.tanh();
            }
            output.clone_from(&h);
        }
        Ok(outputs)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl BiLstm {
    pub(super) fn load(weights: &Weights, prefix: &str) -> Result<Self> {
        Ok(Self {
            forward: Direction::load(weights, prefix, "")?,
            backward: Direction::load(weights, prefix, "_reverse")?,
        })
    }

    pub(super) fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let forward = self.forward.run(xs, false)?;
        let backward = self.backward.run(xs, true)?;
        let len = forward.len();
        let width = 2 * self.forward.hidden;
        let joined: Vec<f32> = forward
            .into_iter()
            .zip(backward)
            .flat_map(|(f, b)| f.into_iter().chain(b))
            .collect();
        Tensor::from_vec(joined, (1, len, width), xs.device())
    }
}

/// Layer norm over channels whose scale and shift come from the style.
struct AdaLayerNorm {
    fc: Linear,
}

impl AdaLayerNorm {
    /// `x` `[1, T, C]`, `style` `[1, S]`.
    fn forward(&self, x: &Tensor, style: &Tensor) -> Result<Tensor> {
        let channels = x.dim(D::Minus1)?;
        let h = self.fc.forward(style)?.unsqueeze(1)?;
        let gamma = h.narrow(D::Minus1, 0, channels)?;
        let beta = h.narrow(D::Minus1, channels, channels)?;
        let mean = x.mean_keepdim(D::Minus1)?;
        let centred = x.broadcast_sub(&mean)?;
        let var = centred.sqr()?.mean_keepdim(D::Minus1)?;
        let normed = centred.broadcast_div(&(var + NORM_EPS)?.sqrt()?)?;
        normed.broadcast_mul(&(gamma + 1.0)?)?.broadcast_add(&beta)
    }
}

/// Predicts durations, then the pitch and energy of every frame.
pub(super) struct ProsodyPredictor {
    encoder: Vec<(BiLstm, AdaLayerNorm)>,
    lstm: BiLstm,
    duration_proj: Linear,
    shared: BiLstm,
    f0: Vec<AdainResBlock>,
    n: Vec<AdainResBlock>,
    f0_proj: Conv,
    n_proj: Conv,
}

impl ProsodyPredictor {
    pub(super) fn load(weights: &Weights) -> Result<Self> {
        let encoder = (0..3)
            .map(|i| {
                Ok((
                    BiLstm::load(weights, &format!("text_encoder.lstms.{}", 2 * i))?,
                    AdaLayerNorm {
                        fc: weights.linear(&format!("text_encoder.lstms.{}.fc", 2 * i + 1))?,
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let blocks = |name: &str| -> Result<Vec<AdainResBlock>> {
            Ok(vec![
                AdainResBlock::load(weights, &format!("{name}.0"), false)?,
                AdainResBlock::load(weights, &format!("{name}.1"), true)?,
                AdainResBlock::load(weights, &format!("{name}.2"), false)?,
            ])
        };
        Ok(Self {
            encoder,
            lstm: BiLstm::load(weights, "lstm")?,
            duration_proj: weights.linear("duration_proj.linear_layer")?,
            shared: BiLstm::load(weights, "shared")?,
            f0: blocks("F0")?,
            n: blocks("N")?,
            f0_proj: weights.conv("F0_proj", Conv1dConfig::default())?,
            n_proj: weights.conv("N_proj", Conv1dConfig::default())?,
        })
    }

    /// `d_en` `[1, T, 512]` → the style-conditioned encoding `[1, T, 640]`.
    pub(super) fn encode(&self, d_en: &Tensor, style: &Tensor) -> Result<Tensor> {
        let len = d_en.dim(1)?;
        let style_t = style
            .unsqueeze(1)?
            .broadcast_as((1, len, style.dim(D::Minus1)?))?
            .contiguous()?;
        let mut x = Tensor::cat(&[d_en, &style_t], D::Minus1)?;
        for (lstm, norm) in &self.encoder {
            let normed = norm.forward(&lstm.forward(&x)?, style)?;
            x = Tensor::cat(&[&normed, &style_t], D::Minus1)?;
        }
        Ok(x)
    }

    /// How many frames each phoneme lasts, at `speed`, as the reference
    /// rounds it (half to even, at least one frame).
    pub(super) fn durations(&self, encoded: &Tensor, speed: f32) -> Result<Vec<usize>> {
        let logits = self.duration_proj.forward(&self.lstm.forward(encoded)?)?;
        let frames = candle_nn::ops::sigmoid(&logits)?
            .sum(D::Minus1)?
            .squeeze(0)?
            .to_vec1::<f32>()?;
        Ok(frames
            .into_iter()
            .map(|f| (f / speed).round_ties_even().max(1.0) as usize)
            .collect())
    }

    /// `aligned` `[1, 640, F]` → pitch and energy curves `[1, 2F]`.
    pub(super) fn f0_n(&self, aligned: &Tensor, style: &Tensor) -> Result<(Tensor, Tensor)> {
        let x = self
            .shared
            .forward(&aligned.transpose(1, 2)?.contiguous()?)?
            .transpose(1, 2)?
            .contiguous()?;
        let curve = |blocks: &[AdainResBlock], proj: &Conv| -> Result<Tensor> {
            let mut y = x.clone();
            for block in blocks {
                y = block.forward(&y, style)?;
            }
            proj.forward(&y)?.squeeze(1)
        };
        Ok((
            curve(&self.f0, &self.f0_proj)?,
            curve(&self.n, &self.n_proj)?,
        ))
    }
}

/// Convolutions and an LSTM over the phoneme embeddings.
pub(super) struct TextEncoder {
    embedding: Embedding,
    cnn: Vec<(Conv, Tensor, Tensor)>,
    lstm: BiLstm,
}

impl TextEncoder {
    pub(super) fn load(weights: &Weights) -> Result<Self> {
        let table = weights.get("embedding.weight")?;
        let channels = table.dim(1)?;
        let cnn = (0..3)
            .map(|i| {
                Ok((
                    weights.conv(&format!("cnn.{i}.0"), same(5, 1))?,
                    weights.get(&format!("cnn.{i}.1.gamma"))?,
                    weights.get(&format!("cnn.{i}.1.beta"))?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            embedding: Embedding::new(table, channels),
            cnn,
            lstm: BiLstm::load(weights, "lstm")?,
        })
    }

    /// `ids` `[1, T]` → `[1, 512, T]`.
    pub(super) fn forward(&self, ids: &Tensor) -> Result<Tensor> {
        let mut x = self.embedding.forward(ids)?.transpose(1, 2)?.contiguous()?;
        for (conv, gamma, beta) in &self.cnn {
            let y = conv.forward(&x)?.transpose(1, 2)?.contiguous()?;
            let y = candle_nn::ops::layer_norm(&y, gamma, beta, NORM_EPS as f32)?;
            x = leaky_relu(&y.transpose(1, 2)?.contiguous()?, 0.2)?;
        }
        self.lstm
            .forward(&x.transpose(1, 2)?.contiguous()?)?
            .transpose(1, 2)?
            .contiguous()
    }
}
