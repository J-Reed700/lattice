//! The style-conditioned decoder in front of the vocoder.

use candle_core::{Module, Result, Tensor, D};
use candle_nn::{Conv1dConfig, Linear};
use rayon::prelude::*;

use super::conv::Conv;
use super::generator::Generator;
use super::source::Noise;
use super::weights::{leaky_relu, same, Weights};

const NORM_EPS: f64 = 1e-5;

/// Instance norm whose scale and shift come from the style.
pub(super) struct AdaIn {
    fc: Linear,
}

impl AdaIn {
    pub(super) fn load(weights: &Weights, prefix: &str) -> Result<Self> {
        Ok(Self {
            fc: weights.linear(&format!("{prefix}.fc"))?,
        })
    }

    /// `x` `[1, C, L]`, `style` `[1, S]`.
    pub(super) fn forward(&self, x: &Tensor, style: &Tensor) -> Result<Tensor> {
        self.forward_then(x, style, |_, y| y)
    }

    /// The norm, then `then(channel, value)` on each value, in one pass per
    /// channel with the channels in parallel: at the vocoder's frame rate,
    /// separate tensor ops cost more than the convolutions between them.
    pub(super) fn forward_then(
        &self,
        x: &Tensor,
        style: &Tensor,
        then: impl Fn(usize, f32) -> f32 + Sync,
    ) -> Result<Tensor> {
        let (batch, channels, len) = x.dims3()?;
        let h: Vec<f32> = self.fc.forward(style)?.flatten_all()?.to_vec1()?;
        let (gamma, beta) = h.split_at(channels.min(h.len()));
        let mut data: Vec<f32> = x.flatten_all()?.to_vec1()?;
        data.par_chunks_mut(len.max(1))
            .enumerate()
            .for_each(|(row, values)| {
                let channel = row % channels;
                let (Some(gamma), Some(beta)) = (gamma.get(channel), beta.get(channel)) else {
                    return;
                };
                let count = values.len() as f64;
                let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / count;
                let var = values
                    .iter()
                    .map(|&v| (f64::from(v) - mean).powi(2))
                    .sum::<f64>()
                    / count;
                let scale = (1.0 + gamma) / (var + NORM_EPS).sqrt() as f32;
                let mean = mean as f32;
                for value in values.iter_mut() {
                    *value = then(channel, (*value - mean) * scale + beta);
                }
            });
        Tensor::from_vec(data, (batch, channels, len), x.device())
    }
}

/// The depthwise transposed convolution (kernel 3, stride 2) that doubles a
/// block's length, computed as zero-stuffing plus three shifted products
/// rather than one Candle convolution per channel.
struct Upsample {
    kernel: Tensor,
    bias: Option<Tensor>,
}

impl Upsample {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (batch, channels, len) = x.dims3()?;
        let stuffed = Tensor::stack(&[x, &x.zeros_like()?], D::Minus1)?
            .reshape((batch, channels, 2 * len))?
            .pad_with_zeros(D::Minus1, 1, 1)?;
        let tap = |k: usize, offset: usize| -> Result<Tensor> {
            stuffed.narrow(D::Minus1, offset, 2 * len)?.broadcast_mul(
                &self
                    .kernel
                    .narrow(D::Minus1, k, 1)?
                    .reshape((1, channels, 1))?,
            )
        };
        let y = ((tap(0, 2)? + tap(1, 1)?)? + tap(2, 0)?)?;
        match &self.bias {
            Some(bias) => y.broadcast_add(&bias.reshape((1, channels, 1))?),
            None => Ok(y),
        }
    }
}

/// StyleTTS 2's `AdainResBlk1d`.
pub(super) struct AdainResBlock {
    norm1: AdaIn,
    norm2: AdaIn,
    conv1: Conv,
    conv2: Conv,
    shortcut: Option<Conv>,
    upsample: Option<Upsample>,
}

impl AdainResBlock {
    pub(super) fn load(weights: &Weights, prefix: &str, upsample: bool) -> Result<Self> {
        let shortcut = format!("{prefix}.conv1x1");
        let has_shortcut = weights.has(&format!("{shortcut}.weight_v"));
        let upsample = if upsample {
            let (kernel, bias) = weights.depthwise(&format!("{prefix}.pool"))?;
            Some(Upsample { kernel, bias })
        } else {
            None
        };
        Ok(Self {
            norm1: AdaIn::load(weights, &format!("{prefix}.norm1"))?,
            norm2: AdaIn::load(weights, &format!("{prefix}.norm2"))?,
            conv1: weights.conv(&format!("{prefix}.conv1"), same(3, 1))?,
            conv2: weights.conv(&format!("{prefix}.conv2"), same(3, 1))?,
            shortcut: if has_shortcut {
                Some(weights.conv(&shortcut, Conv1dConfig::default())?)
            } else {
                None
            },
            upsample,
        })
    }

    pub(super) fn upsamples(&self) -> bool {
        self.upsample.is_some()
    }

    pub(super) fn forward(&self, x: &Tensor, style: &Tensor) -> Result<Tensor> {
        let mut residual = leaky_relu(&self.norm1.forward(x, style)?, 0.2)?;
        if let Some(upsample) = &self.upsample {
            residual = upsample.forward(&residual)?;
        }
        let residual = self.conv1.forward(&residual)?;
        let residual = leaky_relu(&self.norm2.forward(&residual, style)?, 0.2)?;
        let residual = self.conv2.forward(&residual)?;

        let mut shortcut = x.clone();
        if self.upsample.is_some() {
            let len = shortcut.dim(D::Minus1)?;
            shortcut = shortcut.upsample_nearest1d(2 * len)?;
        }
        if let Some(conv) = &self.shortcut {
            shortcut = conv.forward(&shortcut)?;
        }
        (residual + shortcut)? * std::f64::consts::FRAC_1_SQRT_2
    }
}

pub(super) struct Decoder {
    encode: AdainResBlock,
    decode: Vec<AdainResBlock>,
    f0_conv: Conv,
    n_conv: Conv,
    asr_res: Conv,
    generator: Generator,
}

impl Decoder {
    pub(super) fn load(weights: &Weights) -> Result<Self> {
        let halve = Conv1dConfig {
            padding: 1,
            stride: 2,
            ..Default::default()
        };
        Ok(Self {
            encode: AdainResBlock::load(weights, "encode", false)?,
            decode: (0..4)
                .map(|i| AdainResBlock::load(weights, &format!("decode.{i}"), i == 3))
                .collect::<Result<Vec<_>>>()?,
            f0_conv: weights.conv("F0_conv", halve)?,
            n_conv: weights.conv("N_conv", halve)?,
            asr_res: weights.conv("asr_res.0", Conv1dConfig::default())?,
            generator: Generator::load(weights)?,
        })
    }

    /// `asr` `[1, 512, F]`, `f0` and `n` `[1, 2F]`, `style` `[1, 128]` → the
    /// waveform at 24 kHz.
    pub(super) fn forward(
        &self,
        asr: &Tensor,
        f0: &Tensor,
        n: &Tensor,
        style: &Tensor,
        noise: &mut dyn Noise,
    ) -> Result<Vec<f32>> {
        let f0_down = self.f0_conv.forward(&f0.unsqueeze(1)?)?;
        let n_down = self.n_conv.forward(&n.unsqueeze(1)?)?;
        let mut x = self
            .encode
            .forward(&Tensor::cat(&[asr, &f0_down, &n_down], 1)?, style)?;
        let asr_res = self.asr_res.forward(asr)?;
        let mut residual = true;
        for block in &self.decode {
            if residual {
                x = Tensor::cat(&[&x, &asr_res, &f0_down, &n_down], 1)?;
            }
            x = block.forward(&x, style)?;
            if block.upsamples() {
                residual = false;
            }
        }
        self.generator.forward(&x, style, f0, noise)
    }
}
